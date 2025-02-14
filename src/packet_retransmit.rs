use std::thread;

use crate::{
    device_protocol::WorkReqSend,
    fragmenter::PacketFragmenter,
    protocol_impl_hardware::SendQueueScheduler,
    qp_table::QpTable,
    send_queue::{IbvSendQueue, SendQueueElem},
};

#[allow(variant_size_differences)]
pub(crate) enum PacketRetransmitTask {
    NewWr {
        qpn: u32,
        wr: SendQueueElem,
    },
    RetransmitRange {
        qpn: u32,
        // Inclusive
        psn_low: u32,
        // Exclusive
        psn_high: u32,
    },
    Ack {
        qpn: u32,
        psn: u32,
    },
}

impl PacketRetransmitTask {
    fn qpn(&self) -> u32 {
        match *self {
            PacketRetransmitTask::RetransmitRange { qpn, .. }
            | PacketRetransmitTask::NewWr { qpn, .. }
            | PacketRetransmitTask::Ack { qpn, .. } => qpn,
        }
    }
}

pub(crate) struct PacketRetransmitWorker {
    receiver: flume::Receiver<PacketRetransmitTask>,
    wr_sender: SendQueueScheduler,
    table: QpTable<IbvSendQueue>,
}

impl PacketRetransmitWorker {
    pub(crate) fn new(
        receiver: flume::Receiver<PacketRetransmitTask>,
        wr_sender: SendQueueScheduler,
    ) -> Self {
        Self {
            receiver,
            wr_sender,
            table: QpTable::new(),
        }
    }

    pub(crate) fn spawn(self) {
        let _handle = thread::Builder::new()
            .name("timer-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    #[allow(clippy::needless_pass_by_value)] // consume the flag
    /// Run the handler loop
    fn run(mut self) {
        while let Ok(task) = self.receiver.recv() {
            let qpn = task.qpn();
            let Some(sq) = self.table.get_qp_mut(qpn) else {
                continue;
            };
            match task {
                PacketRetransmitTask::NewWr { wr, .. } => {
                    sq.push(wr);
                }
                PacketRetransmitTask::RetransmitRange {
                    psn_low, psn_high, ..
                } => {
                    let sqes = sq.range(psn_low, psn_high);
                    let packets = sqes
                        .into_iter()
                        .flat_map(|sqe| PacketFragmenter::new(sqe.wr(), sqe.qp_param(), sqe.psn()))
                        .skip_while(|x| x.psn < psn_low)
                        .take_while(|x| x.psn < psn_high);
                    for mut packet in packets {
                        packet.set_is_retry();
                        self.wr_sender.send(packet);
                    }
                }
                PacketRetransmitTask::Ack { psn, .. } => {
                    sq.pop_until(psn);
                }
            }
        }
    }
}
