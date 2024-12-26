use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use parking_lot::Mutex;

use crate::{
    desc::{RingBufDescToHost, RingBufDescUntyped},
    ring::SyncDevice,
};

use super::CmdRespQueue;

/// Worker that processes command responses from the response queue
struct CmdRespQueueWorker<Buf, Dev> {
    /// The command response queue
    queue: CmdRespQueue<Buf, Dev>,
}

/// Unique identifier for a command
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CmdId(pub u16);

/// Notification mechanism using atomic boolean
#[derive(Clone)]
pub(crate) struct Notify(Arc<AtomicBool>);

impl Notify {
    /// Creates a new `Notify`
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Sets the notification state to notified
    fn notify(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Returns whether this notification has been notified
    pub(crate) fn notified(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A registration structure that maps command IDs to their notification channels.
pub(crate) struct Registration {
    /// Internal hashmap storing command ID to notification channel mappings.
    inner: HashMap<CmdId, Notify>,
}

impl Registration {
    /// Register a command ID to be notified when completed
    ///
    /// # Returns
    ///
    /// * `Some(Notify)` - A new Notify instance if registration succeeded
    /// * `None` - If the command ID was already registered
    pub(crate) fn register(&mut self, cmd_id: CmdId) -> Option<Notify> {
        let notify = Notify::new();
        match self.inner.entry(cmd_id) {
            Entry::Occupied(_) => None,
            Entry::Vacant(e) => {
                let _ignore = e.insert(notify.clone());
                Some(notify)
            }
        }
    }

    /// Deregister the command ID and notify
    fn notify(&mut self, cmd_id: &CmdId) {
        if let Some(notify) = self.inner.remove(cmd_id) {
            notify.notify();
        }
    }
}

/// Run the command queue worker
#[allow(clippy::needless_pass_by_value)] // the Arc should be moved to the current function
fn run_worker<Buf, Dev>(mut worker: CmdRespQueueWorker<Buf, Dev>, reg: Arc<Mutex<Registration>>)
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: SyncDevice,
{
    loop {
        let Some(desc) = worker.queue.try_consume() else {
            continue;
        };
        let user_data = match desc {
            RingBufDescToHost::CmdQueueRespDescUpdatePGT(desc) => {
                desc.headers().cmd_queue_common_header().user_data()
            }
            RingBufDescToHost::CmdQueueRespDescUpdateMrTable(desc) => {
                desc.headers().cmd_queue_common_header().user_data()
            }
        };
        let cmd_id = CmdId(user_data);
        reg.lock().notify(&cmd_id);
    }
}
