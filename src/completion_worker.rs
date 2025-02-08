use std::collections::HashMap;

use tracing::error;

use crate::{
    completion::{CompletionEvent, CompletionQueueTable, EventRegistry},
    queue_pair::QueuePairAttrTable,
    tracker::MessageMeta,
};

struct CompletionWorker {
    cq_table: CompletionQueueTable,
    qp_table: QueuePairAttrTable,
    completion_rx: flume::Receiver<Completion>,
}

impl CompletionWorker {
    fn new(
        cq_table: CompletionQueueTable,
        qp_table: QueuePairAttrTable,
        completion_rx: flume::Receiver<Completion>,
    ) -> Self {
        Self {
            cq_table,
            qp_table,
            completion_rx,
        }
    }

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
            let Some(cqh) = (if completion.is_send {
                attr.send_cq
            } else {
                attr.recv_cq
            }) else {
                continue;
            };
            let Some(cq) = self.cq_table.get(cqh) else {
                continue;
            };
            // TODO: Move completion registration and notification into the worker
            cq.ack_event(completion.msn, completion.qpn, completion.is_send);
        }
    }
}

pub(crate) struct Completion {
    qpn: u32,
    msn: u16,
    is_send: bool,
}

impl Completion {
    pub(crate) fn new_send(qpn: u32, msn: u16) -> Self {
        Self {
            qpn,
            msn,
            is_send: true,
        }
    }

    pub(crate) fn new_recv(qpn: u32, msn: u16) -> Self {
        Self {
            qpn,
            msn,
            is_send: false,
        }
    }
}
