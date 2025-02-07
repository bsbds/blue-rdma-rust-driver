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

pub(crate) struct MessageWorker {
    tracker_table: MessageTrackerTable,
    task_rx: flume::Receiver<Task>,
    comp_tx: flume::Sender<Completion>,
    ack_tx: flume::Sender<Ack>,
}

impl MessageWorker {
    fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name("message-worker".into())
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
                        self.comp_tx.send(Completion::new(qpn, message.msn().0));
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
