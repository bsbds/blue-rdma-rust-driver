use std::{
    collections::{HashMap, VecDeque},
    ffi::c_void,
    fs::File,
    io::Write,
    iter,
    os::fd::RawFd,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use bitvec::vec::BitVec;
use parking_lot::Mutex;

use crate::constants::{MAX_CQ_CNT, MAX_MSN_WINDOW, MAX_QP_CNT};

#[derive(Default)]
pub(crate) struct CompletionQueue {
    /// Current number of CQEs
    cqe_count: AtomicUsize,
    /// Unique handle identifying this CQ
    handle: u32,
    /// Number of CQEs this CQ can hold
    num_cqe: usize,
    /// File descriptor for the completion event channel
    channel: Option<Mutex<File>>,
    /// Local Event registration
    local_event_registry: EventRegistry,
    /// Remote Event registration
    remote_event_registry: EventRegistry,
    /// Completion queue
    completion: Mutex<VecDeque<CompletionEvent>>,
}

impl CompletionQueue {
    /// Poll the event queue for the next completion event.
    ///
    /// # Returns
    /// * `Option<CompletionEvent>` - The next completion event if available, None otherwise
    pub(crate) fn poll_event_queue(&self) -> Option<CompletionEvent> {
        self.completion.lock().pop_front()
    }

    /// Acknowledge an event with the given MSN and queue pair number.
    ///
    /// # Arguments
    /// * `msn` - Message Sequence Number to acknowledge
    /// * `qpn` - Queue Pair Number associated with this event
    #[allow(clippy::as_conversions)] // u16 to usize
    pub(crate) fn ack_event(&self, last_msn_acked: u16, qpn: u32, is_local: bool) {
        let events = self
            .registry(is_local)
            .remove_completions(qpn, last_msn_acked);
        let mut event_count = events.len();
        for event in events {
            self.completion.lock().push_back(event);
        }
        // TODO: check cqe limit
        let _prev = self.cqe_count.fetch_add(event_count, Ordering::Relaxed);
        if let Some(channel) = self.channel.as_ref() {
            let buf = vec![0u8; event_count.checked_mul(8).unwrap_or_else(|| unreachable!())];
            channel
                .lock()
                .write_all(&buf)
                .unwrap_or_else(|err| unreachable!("channel not writable: {err}"));
        }
    }

    fn registry(&self, is_local: bool) -> &EventRegistry {
        if is_local {
            &self.local_event_registry
        } else {
            &self.remote_event_registry
        }
    }
}

pub(crate) struct CompletionQueueTable {
    inner: Arc<[CompletionQueue]>,
}

impl CompletionQueueTable {
    fn new() -> Self {
        Self {
            inner: iter::repeat_with(CompletionQueue::default)
                .take(MAX_CQ_CNT)
                .collect(),
        }
    }

    pub(crate) fn clone_arc(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Gets a reference to the cq associated with the given cqN
    pub(crate) fn get(&self, handle: u32) -> Option<&CompletionQueue> {
        self.inner.get(handle as usize)
    }
}

/// Manages CQs
pub(crate) struct CqManager {
    /// Bitmap tracking allocated CQ handles
    bitmap: BitVec,
    /// CQ handle to `DeviceCq` mapping
    cq_table: CompletionQueueTable,
}

#[allow(clippy::as_conversions, clippy::indexing_slicing)]
impl CqManager {
    /// Creates a new `CqManager`
    pub(crate) fn new() -> Self {
        let mut bitmap = BitVec::with_capacity(MAX_CQ_CNT);
        bitmap.resize(MAX_CQ_CNT, false);
        Self {
            bitmap,
            cq_table: CompletionQueueTable::new(),
        }
    }

    pub(crate) fn register_event(
        &self,
        handle: u32,
        qpn: u32,
        event: CompletionEvent,
        is_local: bool,
    ) {
        if let Some(cq) = self.cq_table.get(handle) {
            cq.registry(is_local).register(qpn, event);
        }
    }

    /// Allocates a new cq and returns its cqN
    #[allow(clippy::cast_possible_truncation)] // no larger than u32
    pub(crate) fn create_cq(&mut self) -> Option<u32> {
        let handle = self.bitmap.first_zero()? as u32;
        self.bitmap.set(handle as usize, true);
        Some(handle)
    }

    /// Removes and returns the cq associated with the given cqN
    pub(crate) fn destroy_cq(&mut self, handle: u32) {
        if handle as usize >= MAX_CQ_CNT {
            return;
        }
        self.bitmap.set(handle as usize, false);
    }

    /// Gets a reference to the cq associated with the given cqN
    pub(crate) fn table(&self) -> &CompletionQueueTable {
        &self.cq_table
    }
}

#[derive(Debug)]
pub(crate) struct EventRegistry {
    table: Box<[Mutex<VecDeque<CompletionEvent>>]>,
}

impl Default for EventRegistry {
    fn default() -> Self {
        Self {
            table: iter::repeat_with(Mutex::default).take(MAX_QP_CNT).collect(),
        }
    }
}

impl EventRegistry {
    pub(crate) fn remove_completions(&self, qpn: u32, last_msn_acked: u16) -> Vec<CompletionEvent> {
        let mut events = Vec::new();
        if let Some(queue) = self.table.get(qpn as usize) {
            let mut guard = queue.lock();
            while let Some(event) = guard.front() {
                let x = last_msn_acked.wrapping_sub(event.msn);
                if (x as usize) < MAX_MSN_WINDOW {
                    events.push(guard.pop_front().unwrap_or_else(|| unreachable!()));
                } else {
                    break;
                }
            }
        }
        events
    }

    pub(crate) fn register(&self, qpn: u32, event: CompletionEvent) {
        if let Some(queue) = self.table.get(qpn as usize) {
            queue.lock().push_back(event);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompletionEvent {
    /// Queue pair number this completion event is for
    pub(crate) qpn: u32,
    /// The MSN
    pub(crate) msn: u16,
    /// Userdata associated with this completion event, can be either `wr_id` or imm
    pub(crate) user_data: u64,
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
