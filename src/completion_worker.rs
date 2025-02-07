use std::collections::HashMap;

use tracing::error;

use crate::{
    completion::{CompletionEvent, CompletionQueueTable, EventRegistry},
    queue_pair::QueuePairAttrTable,
    tracker::MessageMeta,
};

struct CompletionWorker {
    worker_type: WorkerType,
    cq_table: CompletionQueueTable,
    qp_table: QueuePairAttrTable,
    completion_rx: flume::Receiver<Completion>,
}

impl CompletionWorker {
    fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name("completion-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    fn run(mut self) {
        while let Ok(completion) = self.completion_rx.recv() {
            let Some(attr) = self.qp_table.get(completion.qpn) else {
                error!("invalid qpn");
                continue;
            };
            let Some(cqh) = (match self.worker_type {
                WorkerType::Send => attr.send_cq,
                WorkerType::Recv => attr.recv_cq,
            }) else {
                continue;
            };
            let Some(cq) = self.cq_table.get(cqh) else {
                continue;
            };
            // TODO: Move completion registration and notification into the worker
            cq.ack_event(completion.msn, completion.qpn);
        }
    }
}

pub(crate) struct Completion {
    qpn: u32,
    msn: u16,
}

impl Completion {
    pub(crate) fn new(qpn: u32, msn: u16) -> Self {
        Self { qpn, msn }
    }
}

pub(crate) enum WorkerType {
    Send,
    Recv,
}
