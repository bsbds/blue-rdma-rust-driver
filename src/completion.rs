use std::{
    collections::{HashMap, VecDeque},
    ffi::c_void,
    fs::File,
    io::Write,
    iter,
    os::fd::RawFd,
    sync::Arc,
};

use bitvec::vec::BitVec;
use parking_lot::Mutex;

use crate::constants::MAX_MSN_WINDOW;

/// Manages CQs
pub(crate) struct CqManager {
    /// Bitmap tracking allocated CQ handles
    bitmap: BitVec,
    /// CQ handle to `DeviceCq` mapping
    cqs: Vec<DeviceCq>,
}

#[allow(clippy::as_conversions, clippy::indexing_slicing)]
impl CqManager {
    /// Creates a new `CqManager`
    pub(crate) fn new(max_num_cqs: u32) -> Self {
        let size = max_num_cqs as usize;
        let mut bitmap = BitVec::with_capacity(size);
        bitmap.resize(size, false);
        Self {
            bitmap,
            cqs: iter::repeat_with(DeviceCq::default).take(size).collect(),
        }
    }

    pub(crate) fn new_meta_table(&self) -> MetaCqTable {
        let entries = self
            .cqs
            .iter()
            .map(|cq| MetaCqEntry {
                attr: Arc::clone(&cq.attr),
                cqe_count: 0,
            })
            .collect();
        MetaCqTable { table: entries }
    }

    pub(crate) fn register_event(&self, handle: u32, qpn: u32, event: CompletionEvent) {
        if let Some(cq) = self.get_cq(handle) {
            cq.attr.inner.lock().event_registry.register(qpn, event);
        }
    }

    /// Allocates a new cq and returns its cqN
    #[allow(clippy::cast_possible_truncation)] // no larger than u32
    pub(crate) fn create_cq(&mut self) -> Option<u32> {
        let handle = self.bitmap.first_zero()? as u32;
        self.bitmap.set(handle as usize, true);
        if let Some(cq) = self.cqs.get(handle as usize) {
            cq.attr.clear();
        }
        Some(handle)
    }

    /// Removes and returns the cq associated with the given cqN
    pub(crate) fn destroy_cq(&mut self, handle: u32) {
        if handle as usize >= self.max_num_cqs() {
            return;
        }
        self.bitmap.set(handle as usize, false);
        if let Some(cq) = self.cqs.get(handle as usize) {
            cq.attr.clear();
        }
    }

    /// Gets a reference to the cq associated with the given cqN
    pub(crate) fn get_cq(&self, handle: u32) -> Option<&DeviceCq> {
        self.cqs.get(handle as usize)
    }

    /// Gets a mutable reference to the cq associated with the given cqN
    pub(crate) fn get_cq_mut(&mut self, handle: u32) -> Option<&mut DeviceCq> {
        self.cqs.get_mut(handle as usize)
    }

    /// Returns the maximum number of Queue Pairs (cqs) supported
    fn max_num_cqs(&self) -> usize {
        self.cqs.len()
    }
}

pub(crate) struct MetaCqTable {
    table: Vec<MetaCqEntry>,
}

impl MetaCqTable {
    #[allow(clippy::as_conversions)] // u32 to usize
    pub(crate) fn get_mut(&mut self, handle: u32) -> Option<&mut MetaCqEntry> {
        self.table.get_mut(handle as usize)
    }
}

pub(crate) struct MetaCqEntry {
    attr: Arc<CqAttrShared>,
    /// Current number of CQEs
    cqe_count: usize,
}

impl MetaCqEntry {
    /// Acknowledge an event with the given MSN and queue pair number.
    ///
    /// # Arguments
    /// * `msn` - Message Sequence Number to acknowledge
    /// * `qpn` - Queue Pair Number associated with this event
    #[allow(clippy::as_conversions)] // u16 to usize
    pub(crate) fn ack_event(&mut self, last_msn_acked: u16, qpn: u32) {
        let mut attr_guard = self.attr.inner.lock();
        let Some(queue) = attr_guard.event_registry.get_mut(qpn) else {
            return;
        };
        let mut events = Vec::new();
        while let Some(event) = queue.front() {
            let x = last_msn_acked.wrapping_sub(event.msn);
            if x > 0 && (x as usize) < MAX_MSN_WINDOW {
                events.push(queue.pop_front());
            } else {
                break;
            }
        }
        let mut event_count = events.len();
        for event in events.into_iter().flatten() {
            attr_guard.event_queue.push_back(event);
        }
        // TODO: check cqe limit
        self.cqe_count = self.cqe_count.wrapping_add(event_count);
        if let Some(channel) = attr_guard.channel.as_mut() {
            let buf = vec![0u8; event_count.checked_mul(8).unwrap_or_else(|| unreachable!())];
            channel
                .write_all(&buf)
                .unwrap_or_else(|err| unreachable!("channel not writable: {err}"));
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct CqAttrShared {
    inner: Mutex<CqAttr>,
}

impl CqAttrShared {
    fn num_cqe(&self) -> usize {
        self.inner.lock().num_cqe
    }

    fn clear(&self) {
        *self.inner.lock() = CqAttr::default();
    }
}

#[derive(Debug, Default)]
pub(crate) struct CqAttr {
    /// Unique handle identifying this CQ
    handle: u32,
    /// Number of CQEs this CQ can hold
    num_cqe: usize,
    /// Event queue
    event_queue: VecDeque<CompletionEvent>,
    /// File descriptor for the completion event channel
    channel: Option<File>,
    /// Event registration
    event_registry: EventRegistry,
}

/// A completion queue implementation
#[derive(Default, Debug)]
pub(crate) struct DeviceCq {
    attr: Arc<CqAttrShared>,
}

#[derive(Debug, Default)]
pub(crate) struct EventRegistry {
    events: HashMap<u32, VecDeque<CompletionEvent>>,
}

impl EventRegistry {
    pub(crate) fn get_mut(&mut self, qpn: u32) -> Option<&mut VecDeque<CompletionEvent>> {
        self.events.get_mut(&qpn)
    }

    pub(crate) fn register(&mut self, qpn: u32, event: CompletionEvent) {
        self.events.entry(qpn).or_default().push_back(event);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompletionEvent {
    /// Queue pair number this completion event is for
    qpn: u32,
    /// The MSN
    msn: u16,
    /// Userdata associated with this completion event, can be either `wr_id` or imm
    user_data: u64,
}

impl CompletionEvent {
    /// Creates a new `CompletionEvent`
    pub(crate) fn new(qpn: u32, msn: u16, user_data: u64) -> Self {
        Self {
            qpn,
            msn,
            user_data,
        }
    }
}

impl DeviceCq {
    /// Poll the event queue for the next completion event.
    ///
    /// # Returns
    /// * `Option<CompletionEvent>` - The next completion event if available, None otherwise
    pub(crate) fn poll_event_queue(&self) -> Option<CompletionEvent> {
        self.attr.inner.lock().event_queue.pop_front()
    }
}
