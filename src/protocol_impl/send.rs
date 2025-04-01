use std::{io, iter, sync::Arc, time::Duration};

use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use tracing::error;

use crate::{
    device_protocol::{WorkReqSend, WrChunk},
    mem::PageWithPhysAddr,
    protocol_impl::device::CsrWriterAdaptor,
};

use super::{
    desc::{SendQueueReqDescSeg0, SendQueueReqDescSeg1},
    device::{
        mode::Mode,
        proxy::{build_send_queue_proxies, SendQueueProxy},
        CsrBaseAddrAdaptor, DeviceAdaptor,
    },
    queue::{
        send_queue::{SendQueue, SendQueueDesc},
        DescRingBuffer,
    },
};

/// Injector
type WrInjector = Injector<WrChunk>;
/// Stealer
type WrStealer = Stealer<WrChunk>;
/// Worker
type WrWorker = Worker<WrChunk>;

/// Schedules send work requests across worker threads
pub(crate) struct SendQueueScheduler {
    /// Work request injector for distributing work to worker threads
    injector: Arc<WrInjector>,
}

impl SendQueueScheduler {
    pub(crate) fn new() -> Self {
        Self {
            injector: WrInjector::new().into(),
        }
    }

    pub(crate) fn clone_arc(&self) -> Self {
        Self {
            injector: Arc::clone(&self.injector),
        }
    }

    pub(crate) fn injector(&self) -> Arc<WrInjector> {
        Arc::clone(&self.injector)
    }

    /// Submits a work request chunk to be processed by worker threads
    ///
    /// # Arguments
    /// * `wr` - The work request chunk to be scheduled
    fn send_wr_task(&self, wr: WrChunk) {
        self.injector.push(wr);
    }
}

impl WorkReqSend for SendQueueScheduler {
    fn send(&self, op: WrChunk) -> io::Result<()> {
        self.send_wr_task(op);
        Ok(())
    }
}

/// Worker thread for processing send work requests
pub(crate) struct SendWorker<Dev> {
    /// id of the worker
    id: usize,
    /// Local work request queue for this worker
    local: WrWorker,
    /// Global work request injector shared across workers
    global: Arc<WrInjector>,
    /// Work stealers for taking work from other workers
    remotes: Box<[WrStealer]>,
    /// Queue for submitting send requests to the NIC
    send_queue: SendQueue,
    /// Csr proxy
    csr_adaptor: SendQueueProxy<Dev>,
}

impl<Dev: DeviceAdaptor + Send + 'static> SendWorker<Dev> {
    pub(crate) fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name(format!("send-worker-{}", self.id))
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn thread: {err}"));
    }

    /// Run the worker
    pub(crate) fn run(mut self) {
        loop {
            let Some(wr) = Self::find_task(&self.local, &self.global, &self.remotes) else {
                continue;
            };
            let desc0 = SendQueueReqDescSeg0::new(
                wr.opcode,
                wr.msn,
                wr.psn.into_inner(),
                wr.qp_type,
                wr.dqpn,
                wr.flags,
                wr.dqp_ip,
                wr.raddr,
                wr.rkey,
                wr.total_len,
            );
            let desc1 = SendQueueReqDescSeg1::new(
                wr.opcode,
                wr.pmtu,
                wr.is_first,
                wr.is_last,
                wr.is_retry,
                wr.enable_ecn,
                wr.sqpn,
                wr.imm,
                wr.mac_addr,
                wr.lkey,
                wr.len,
                wr.laddr,
            );

            if self.send_queue.push(SendQueueDesc::Seg0(desc0)).is_err() {
                self.local.push(wr);
                continue;
            }
            if self.send_queue.push(SendQueueDesc::Seg1(desc1)).is_err() {
                self.local.push(wr);
                continue;
            }
            if self.csr_adaptor.write_head(self.send_queue.head()).is_err() {
                error!("failed to flush queue pointer");
            }
            if let Ok(tail_ptr) = self.csr_adaptor.read_tail() {
                self.send_queue.set_tail(tail_ptr);
            }
        }
    }

    /// Find a task
    fn find_task<T>(local: &Worker<T>, global: &Injector<T>, stealers: &[Stealer<T>]) -> Option<T> {
        // Pop a task from the local queue, if not empty.
        local.pop().or_else(|| {
            // Otherwise, we need to look for a task elsewhere.
            iter::repeat_with(|| {
                // Try stealing a batch of tasks from the global queue.
                global
                    .steal_batch_and_pop(local)
                    // Or try stealing a task from one of the other threads.
                    .or_else(|| stealers.iter().map(Stealer::steal).collect())
            })
            // Loop while no task was stolen and any steal operation needs to be retried.
            .find(|s| !s.is_retry())
            // Extract the stolen task, if there is one.
            .and_then(Steal::success)
        })
    }
}

pub(crate) fn spawn_send_workers<Dev>(
    dev: &Dev,
    pages: Vec<PageWithPhysAddr>,
    mode: Mode,
    global_injector: &Arc<WrInjector>,
) -> io::Result<()>
where
    Dev: DeviceAdaptor + Clone + Send + 'static,
{
    let mut sq_proxies = build_send_queue_proxies(dev.clone(), mode);
    for (proxy, page) in sq_proxies.iter_mut().zip(pages.iter()) {
        proxy.write_base_addr(page.phys_addr)?;
    }
    let send_queues: Vec<_> = pages
        .into_iter()
        .map(|p| SendQueue::new(DescRingBuffer::new(p.page)))
        .collect();
    let workers: Vec<_> = iter::repeat_with(WrWorker::new_fifo)
        .take(send_queues.len())
        .collect();
    let stealers: Vec<_> = workers.iter().map(WrWorker::stealer).collect();
    workers
        .into_iter()
        .zip(send_queues)
        .zip(sq_proxies)
        .enumerate()
        .map(|(id, ((local, send_queue), csr_adaptor))| SendWorker {
            id,
            local,
            global: Arc::clone(global_injector),
            remotes: stealers
                .clone()
                .into_iter()
                .enumerate()
                .filter_map(|(i, x)| (i != id).then_some(x))
                .collect(),
            send_queue,
            csr_adaptor,
        })
        .for_each(SendWorker::spawn);

    Ok(())
}
