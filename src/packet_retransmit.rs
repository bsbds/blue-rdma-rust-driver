use std::{cmp::Ordering, collections::VecDeque, iter, thread};

use crate::{
    constants::{MAX_PSN_WINDOW, MAX_QP_CNT},
    device_protocol::{QpParams, WorkReqOpCode, WorkReqSend},
    fragmenter::WrPacketFragmenter,
    protocol_impl::SendQueueScheduler,
    qp_table::QpTable,
    queue_pair::qpn_index,
    send::SendWrRdma,
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
                        .flat_map(|sqe| {
                            WrPacketFragmenter::new(sqe.wr(), sqe.qp_param(), sqe.psn())
                        })
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

#[derive(Default)]
pub(crate) struct IbvSendQueue {
    inner: VecDeque<SendQueueElem>,
}

impl IbvSendQueue {
    pub(crate) fn push(&mut self, elem: SendQueueElem) {
        self.inner.push_back(elem);
    }

    pub(crate) fn pop_until(&mut self, psn: u32) {
        let mut a = self.inner.partition_point(|x| x.psn < Psn(psn));
        let _drop = self.inner.drain(..a);
    }

    /// Find range [`psn_low`, `psn_high`)
    pub(crate) fn range(&self, psn_low: u32, psn_high: u32) -> Vec<SendQueueElem> {
        let mut a = self.inner.partition_point(|x| x.psn <= Psn(psn_low));
        let mut b = self.inner.partition_point(|x| x.psn < Psn(psn_high));
        a = a.saturating_sub(1);
        self.inner.range(a..b).copied().collect()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SendQueueElem {
    psn: Psn,
    wr: SendWrRdma,
    qp_param: QpParams,
}

impl SendQueueElem {
    pub(crate) fn new(wr: SendWrRdma, psn: u32, qp_param: QpParams) -> Self {
        Self {
            wr,
            psn: Psn(psn),
            qp_param,
        }
    }

    pub(crate) fn psn(&self) -> u32 {
        self.psn.0
    }

    pub(crate) fn wr(&self) -> SendWrRdma {
        self.wr
    }

    pub(crate) fn qp_param(&self) -> QpParams {
        self.qp_param
    }

    pub(crate) fn opcode(&self) -> WorkReqOpCode {
        self.wr.opcode()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Psn(pub(crate) u32);

impl PartialOrd for Psn {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let x = self.0.wrapping_sub(other.0);
        Some(match x {
            0 => Ordering::Equal,
            x if x as usize > MAX_PSN_WINDOW => Ordering::Less,
            _ => Ordering::Greater,
        })
    }
}
