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
    cqs: Vec<Option<DeviceCq>>,
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
            cqs: iter::repeat_with(|| None).take(size).collect(),
        }
    }

    /// Allocates a new cq and returns its cqN
    #[allow(clippy::cast_possible_truncation)] // no larger than u32
    pub(crate) fn create_cq(&mut self, cq: DeviceCq) -> Option<u32> {
        let cqn = self.bitmap.first_zero()? as u32;
        self.bitmap.set(cqn as usize, true);
        self.cqs[cqn as usize] = Some(cq);
        Some(cqn)
    }

    /// Removes and returns the cq associated with the given cqN
    pub(crate) fn destroy_cq(&mut self, cqn: u32) -> Option<DeviceCq> {
        if cqn as usize >= self.max_num_cqs() {
            return None;
        }
        self.bitmap.set(cqn as usize, false);
        self.cqs[cqn as usize].take()
    }

    /// Gets a reference to the cq associated with the given cqN
    pub(crate) fn get_cq(&self, handle: u32) -> Option<&DeviceCq> {
        if handle as usize >= self.max_num_cqs() {
            return None;
        }
        self.cqs[handle as usize].as_ref()
    }

    /// Gets a mutable reference to the cq associated with the given cqN
    pub(crate) fn get_cq_mut(&mut self, cqn: u32) -> Option<&mut DeviceCq> {
        if cqn as usize >= self.max_num_cqs() {
            return None;
        }
        self.cqs[cqn as usize].as_mut()
    }

    /// Returns the maximum number of Queue Pairs (cqs) supported
    fn max_num_cqs(&self) -> usize {
        self.cqs.len()
    }
}

/// A completion queue implementation
#[derive(Debug)]
pub(crate) struct DeviceCq {
    /// Unique handle identifying this CQ
    handle: u32,
    /// Number of CQEs this CQ can hold
    num_cqe: usize,
    /// File descriptor for the completion event channel
    channel: File,
    /// Opaque pointer stored the user context
    context_addr: u64,
    /// Current number of CQEs
    cqe_count: usize,
    /// Event registration
    event_reg: Arc<EventRegistry>,
    /// Event queue
    event_queue: VecDeque<CompletionEvent>,
}

#[derive(Debug)]
pub(crate) struct EventRegistry {
    events: Mutex<HashMap<u32, VecDeque<CompletionEvent>>>,
}

impl EventRegistry {
    pub(crate) fn new() -> Self {
        Self {
            events: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn register(&self, qpn: u32, event: CompletionEvent) {
        let mut events = self.events.lock();
        events.entry(qpn).or_default().push_back(event);
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
    /// Creates a new `DeviceCq`
    pub(crate) fn new(
        handle: u32,
        num_cqe: usize,
        channel: File,
        context_addr: u64,
        event_reg: Arc<EventRegistry>,
    ) -> Self {
        Self {
            handle,
            num_cqe,
            channel,
            context_addr,
            cqe_count: 0,
            event_reg,
            event_queue: VecDeque::new(),
        }
    }

    ///// Register a local event with a given Message Sequence Number (MSN) and work request ID.
    /////
    ///// # Arguments
    ///// * `msn` - Message Sequence Number to register
    ///// * `wr_id` - Work request ID to associate with this event
    //pub(crate) fn register_local_event(&mut self, msn: u16, wr_id: u64) {
    //    if self.local_event.insert(msn, wr_id).is_some() {
    //        tracing::error!("duplicate event MSN: {msn}");
    //    }
    //}

    /// Acknowledge an event with the given MSN and queue pair number.
    ///
    /// # Arguments
    /// * `msn` - Message Sequence Number to acknowledge
    /// * `qpn` - Queue Pair Number associated with this event
    #[allow(clippy::as_conversions)] // u16 to usize
    pub(crate) fn ack_event(&mut self, last_msn_acked: u16, qpn: u32) {
        let mut event_count: usize = 0;
        {
            let mut event_regitry_gurad = self.event_reg.events.lock();
            let Some(queue) = event_regitry_gurad.get_mut(&qpn) else {
                return;
            };

            while let Some(event) = queue.front().copied() {
                let x = last_msn_acked.wrapping_sub(event.msn);
                if x > 0 && (x as usize) < MAX_MSN_WINDOW {
                    let _ignore = queue.pop_front();
                    self.event_queue.push_back(event);
                    event_count = event_count.wrapping_add(1);
                } else {
                    break;
                }
            }
        }
        self.notify_completion(event_count);
    }

    /// Poll the event queue for the next completion event.
    ///
    /// # Returns
    /// * `Option<CompletionEvent>` - The next completion event if available, None otherwise
    pub(crate) fn poll_event_queue(&mut self) -> Option<CompletionEvent> {
        self.event_queue.pop_front()
    }

    /// Notifies the completion event by writing to the channel fd.
    fn notify_completion(&mut self, count: usize) {
        if self.cqe_count == self.num_cqe {
            return;
        }
        let buf = vec![0u8; count.checked_mul(8).unwrap_or_else(|| unreachable!())];
        self.channel
            .write_all(&buf)
            .unwrap_or_else(|err| unreachable!("channel not writable: {err}"));
    }
}
