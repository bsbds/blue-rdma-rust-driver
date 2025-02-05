use std::{iter, sync::Arc, time::Duration};

use crossbeam_deque::{Injector, Steal, Stealer, Worker};

use crate::{
    device_protocol::{WorkReqSend, WrChunk},
    protocol_impl_hardware::device::CsrWriterAdaptor,
};

use super::{
    desc::{SendQueueReqDescSeg0, SendQueueReqDescSeg1},
    queue::send_queue::{SendQueue, SendQueueDesc},
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
    fn send(&self, op: WrChunk) -> std::io::Result<()> {
        self.send_wr_task(op);
        Ok(())
    }
}

pub(crate) struct SendWorkerBuilder {
    global: Arc<WrInjector>,
}

impl SendWorkerBuilder {
    pub(crate) fn new_with_global_injector(global: Arc<WrInjector>) -> Self {
        Self { global }
    }

    pub(crate) fn build_workers(
        self,
        send_queues: Vec<SendQueue>,
        adaptors: Vec<Box<dyn CsrWriterAdaptor + Send + 'static>>,
    ) -> Vec<SendWorker> {
        let workers: Vec<_> = iter::repeat_with(WrWorker::new_fifo)
            .take(send_queues.len())
            .collect();
        let stealers: Vec<_> = workers.iter().map(WrWorker::stealer).collect();
        workers
            .into_iter()
            .zip(send_queues)
            .zip(adaptors)
            .map(|((local, send_queue), csr_adaptor)| SendWorker {
                local,
                global: Arc::clone(&self.global),
                remotes: stealers.clone().into_boxed_slice(),
                send_queue,
                csr_adaptor,
            })
            .collect()
    }
}

/// Worker thread for processing send work requests
pub(crate) struct SendWorker {
    /// Local work request queue for this worker
    local: WrWorker,
    /// Global work request injector shared across workers
    global: Arc<WrInjector>,
    /// Work stealers for taking work from other workers
    remotes: Box<[WrStealer]>,
    /// Queue for submitting send requests to the NIC
    send_queue: SendQueue,
    /// Csr proxy
    csr_adaptor: Box<dyn CsrWriterAdaptor + Send + 'static>,
}

impl SendWorker {
    /// Run the worker
    pub(crate) fn run(mut self) {
        loop {
            let Some(wr) = Self::find_task(&self.local, &self.global, &self.remotes) else {
                std::thread::sleep(Duration::from_millis(10));
                std::hint::spin_loop();
                continue;
            };
            let desc0 = SendQueueReqDescSeg0::new_rdma_write(
                wr.msn,
                wr.psn,
                wr.qp_type,
                wr.dqpn,
                wr.flags,
                wr.dqp_ip,
                wr.raddr,
                wr.rkey,
                wr.total_len,
            );
            let desc1 = SendQueueReqDescSeg1::new_rdma_write(
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

            /// Retry if block
            self.send_queue.push(SendQueueDesc::Seg0(desc0)).unwrap();
            self.send_queue.push(SendQueueDesc::Seg1(desc1)).unwrap();
            if self.csr_adaptor.write_head(self.send_queue.head()).is_err() {
                tracing::error!("failed to flush queue pointer");
            }
            let tail = self.csr_adaptor.read_tail().unwrap();
            self.send_queue.set_tail(tail);
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
