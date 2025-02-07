use tracing::error;

use crate::{
    ack_responder::Ack,
    completion::CompletionQueueTable,
    completion_worker::Completion,
    tracker::{MessageMeta, MessageTracker, MessageTrackerTable},
};

pub(crate) enum Task {
    AppendMessage { qpn: u32, meta: MessageMeta },
    UpdateBasePsn { qpn: u32, psn: u32 },
}

pub(crate) struct SenderMessageWorker {
    tracker_table: MessageTrackerTable,
    task_rx: flume::Receiver<Task>,
    comp_tx: flume::Sender<Completion>,
}

pub(crate) struct ReceiverMessageWorker {
    tracker_table: MessageTrackerTable,
    task_rx: flume::Receiver<Task>,
    comp_tx: flume::Sender<Completion>,
    ack_tx: flume::Sender<Ack>,
}

impl SenderMessageWorker {
    pub(crate) fn new(task_rx: flume::Receiver<Task>, comp_tx: flume::Sender<Completion>) -> Self {
        Self {
            tracker_table: MessageTrackerTable::new(),
            task_rx,
            comp_tx,
        }
    }

    fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name("send-message-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    fn run(mut self) {
        while let Ok(task) = self.task_rx.recv() {
            match task {
                Task::AppendMessage { qpn, meta } => {
                    let Some(tracker) = self.tracker_table.get_qp_mut(qpn) else {
                        error!("invalid qpn: {qpn}");
                        continue;
                    };
                    tracker.append(meta);
                }
                Task::UpdateBasePsn { qpn, psn } => {
                    let Some(tracker) = self.tracker_table.get_qp_mut(qpn) else {
                        error!("invalid qpn: {qpn}");
                        continue;
                    };
                    let completed = tracker.ack(psn);
                    for message in completed {
                        self.comp_tx
                            .send(Completion::new_send(qpn, message.msn().0));
                    }
                }
            }
        }
    }
}

impl ReceiverMessageWorker {
    pub(crate) fn new(
        task_rx: flume::Receiver<Task>,
        comp_tx: flume::Sender<Completion>,
        ack_tx: flume::Sender<Ack>,
    ) -> Self {
        Self {
            tracker_table: MessageTrackerTable::new(),
            task_rx,
            comp_tx,
            ack_tx,
        }
    }

    fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name("recv-message-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    fn run(mut self) {
        while let Ok(task) = self.task_rx.recv() {
            match task {
                Task::AppendMessage { qpn, meta } => {
                    let Some(tracker) = self.tracker_table.get_qp_mut(qpn) else {
                        error!("invalid qpn: {qpn}");
                        continue;
                    };
                    tracker.append(meta);
                }
                Task::UpdateBasePsn { qpn, psn } => {
                    let Some(tracker) = self.tracker_table.get_qp_mut(qpn) else {
                        error!("invalid qpn: {qpn}");
                        continue;
                    };
                    let completed = tracker.ack(psn);
                    for message in completed {
                        self.comp_tx
                            .send(Completion::new_recv(qpn, message.msn().0));
                        if message.ack_req() {
                            let _ignore =
                                self.ack_tx
                                    .send(Ack::new(qpn, message.msn().0, message.psn()));
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn spawn_bi_workers(
    sender_task_rx: flume::Receiver<Task>,
    receiver_task_rx: flume::Receiver<Task>,
    comp_tx: flume::Sender<Completion>,
    ack_tx: flume::Sender<Ack>,
) {
    let sender_worker = SenderMessageWorker::new(sender_task_rx, comp_tx.clone());
    let receiver_worker = ReceiverMessageWorker::new(receiver_task_rx, comp_tx, ack_tx);
    sender_worker.spawn();
    receiver_worker.spawn();
}
