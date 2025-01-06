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
    device::DeviceAdaptor,
};

use super::CmdRespQueue;

/// Worker that processes command responses from the response queue
struct CmdRespQueueWorker<Buf, Dev> {
    /// The command response queue
    queue: CmdRespQueue<Buf, Dev>,
}

impl<Buf, Dev> CmdRespQueueWorker<Buf, Dev> {
    /// Creates a new `CmdRespQueueWorker`
    fn new(queue: CmdRespQueue<Buf, Dev>) -> Self {
        Self { queue }
    }
}

/// Unique identifier for a command
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
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
#[derive(Default)]
pub(crate) struct Registration {
    /// Internal hashmap storing command ID to notification channel mappings.
    inner: HashMap<CmdId, Notify>,
}

impl Registration {
    /// Creates a new `Registration`
    pub(crate) fn new() -> Self {
        Self {
            inner: HashMap::default(),
        }
    }

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
    fn notify(&mut self, cmd_id: CmdId) {
        if let Some(notify) = self.inner.remove(&cmd_id) {
            notify.notify();
        }
    }

    /// Notifies all command ID
    #[cfg(test)]
    pub(crate) fn notify_all(&mut self) {
        for (_id, notify) in self.inner.drain() {
            notify.notify();
        }
    }
}

/// Run the command queue worker
#[allow(clippy::needless_pass_by_value)] // the Arc should be moved to the current function
fn run_worker<Buf, Dev>(mut worker: CmdRespQueueWorker<Buf, Dev>, reg: Arc<Mutex<Registration>>)
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: DeviceAdaptor,
{
    loop {
        let Some(desc) = worker.queue.try_pop() else {
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
        worker.queue.flush();
        let cmd_id = CmdId(user_data);
        reg.lock().notify(cmd_id);
    }
}

#[cfg(test)]
mod test {
    use std::{iter, time::Duration};

    use crate::{
        desc::cmd::{CmdQueueRespDescUpdateMrTable, CmdQueueRespDescUpdatePGT},
        device::dummy::DummyDevice,
        ringbuffer::new_test_ring,
    };

    use super::*;

    #[test]
    fn registration_notify_ok() {
        let mut reg = Registration::new();
        let id = CmdId(1);
        let notify = reg.register(id).unwrap();
        assert!(reg.register(id).is_some());
        assert!(!notify.notified());
        reg.notify(id);
        assert!(notify.notified());
    }

    #[allow(unsafe_code)]
    #[test]
    fn worker_notify_ok() {
        let mut reg = Arc::new(Mutex::new(Registration::new()));
        let mut ring = new_test_ring::<RingBufDescUntyped>();
        let id0 = 0;
        let id1 = 1;
        let desc0 = CmdQueueRespDescUpdateMrTable::new(id0, 0, 0, 0, 0, 0, 0);
        let desc1 = CmdQueueRespDescUpdateMrTable::new(id1, 0, 0, 0, 0, 0, 0);
        let n0 = reg.lock().register(CmdId(id0)).unwrap();
        let n1 = reg.lock().register(CmdId(id1)).unwrap();
        unsafe {
            // assume hardware produce two descriptor
            ring.push(std::mem::transmute(desc0)).unwrap();
            ring.push(std::mem::transmute(desc1)).unwrap();
        }
        let mut queue = CmdRespQueue::new(ring, DummyDevice::default());
        let worker = CmdRespQueueWorker::new(queue);
        std::thread::spawn(|| run_worker(worker, reg));
        std::thread::sleep(Duration::from_millis(1));
        assert!(n0.notified());
        assert!(n1.notified());
    }
}
