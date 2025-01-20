use std::{
    collections::{HashMap, VecDeque},
    ffi::c_void,
    fs::File,
    io::Write,
    iter,
    os::fd::RawFd,
};

use bitvec::vec::BitVec;

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
    context: *const c_void,
    /// Current number of CQEs
    cqe_count: usize,
    /// MSN to `wr_id` map, stores event that needs to be notified
    local_event: HashMap<u16, u64>,
    /// Event queue
    event_queue: VecDeque<CompletionEvent>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompletionEvent {
    /// Work request ID associated with this completion event
    wr_id: u64,
    /// Queue pair number this completion event is for
    qpn: u32,
}

impl CompletionEvent {
    /// Creates a new `CompletionEvent`
    pub(crate) fn new(wr_id: u64, qpn: u32) -> Self {
        Self { wr_id, qpn }
    }
}

impl DeviceCq {
    /// Creates a new `DeviceCq`
    pub(crate) fn new(handle: u32, num_cqe: usize, channel: File, context: *const c_void) -> Self {
        Self {
            handle,
            num_cqe,
            channel,
            context,
            cqe_count: 0,
            local_event: HashMap::new(),
            event_queue: VecDeque::new(),
        }
    }

    /// Register a local event with a given Message Sequence Number (MSN) and work request ID.
    ///
    /// # Arguments
    /// * `msn` - Message Sequence Number to register
    /// * `wr_id` - Work request ID to associate with this event
    pub(crate) fn register_local_event(&mut self, msn: u16, wr_id: u64) {
        if self.local_event.insert(msn, wr_id).is_some() {
            tracing::error!("duplicate event MSN: {msn}");
        }
    }

    /// Acknowledge an event with the given MSN and queue pair number.
    ///
    /// # Arguments
    /// * `msn` - Message Sequence Number to acknowledge
    /// * `qpn` - Queue Pair Number associated with this event
    pub(crate) fn ack_event(&mut self, msn: u16, qpn: u32) {
        if let Some(wr_id) = self.local_event.remove(&msn) {
            self.event_queue.push_back(CompletionEvent::new(wr_id, qpn));
            self.notify_completion();
        }
    }

    /// Poll the event queue for the next completion event.
    ///
    /// # Returns
    /// * `Option<CompletionEvent>` - The next completion event if available, None otherwise
    pub(crate) fn poll_event_queue(&mut self) -> Option<CompletionEvent> {
        self.event_queue.pop_front()
    }

    /// Notifies the completion event by writing to the channel fd.
    fn notify_completion(&mut self) {
        if self.cqe_count == self.num_cqe {
            return;
        }
        let event: u64 = 1;
        self.channel
            .write_all(&event.to_le_bytes())
            .unwrap_or_else(|err| unreachable!("channel not writable: {err}"));
    }
}
