use std::{collections::VecDeque, thread};

use crate::{
    device_protocol::{QpParams, WorkReqOpCode},
    send::SendWrRdma,
    utils::{Psn, QpTable},
};

#[derive(Debug)]
pub(crate) enum SqTask {
    NewWr {
        qpn: u32,
        wr: SendQueueElem,
    },
    Ack {
        qpn: u32,
        psn: Psn,
    },
    GetRange {
        qpn: u32,
        psn_low: Psn,
        psn_high: Psn,
        tx: oneshot::Sender<Vec<SendQueueElem>>,
    },
}

#[derive(Debug)]
pub(crate) struct SqWorker {
    receiver: flume::Receiver<SqTask>,
    table: QpTable<IbvSendQueue>,
}

impl SqWorker {
    pub(crate) fn new(receiver: flume::Receiver<SqTask>) -> Self {
        Self {
            receiver,
            table: QpTable::new(),
        }
    }

    pub(crate) fn spawn(self) {
        let _handle = thread::Builder::new()
            .name("sq-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    fn run(mut self) {
        while let Ok(task) = self.receiver.recv() {
            let _ignore = self.handle(task);
        }
    }

    fn handle(&mut self, task: SqTask) -> Option<()> {
        match task {
            SqTask::NewWr { qpn, wr } => {
                self.table.get_qp_mut(qpn)?.push(wr);
            }
            SqTask::Ack { qpn, psn } => {
                self.table.get_qp_mut(qpn)?.pop_until(psn);
            }
            SqTask::GetRange {
                qpn,
                psn_low,
                psn_high,
                tx,
            } => {
                let sqes = self.table.get_qp(qpn)?.range(psn_low, psn_high);
                let _ignore = tx.send(sqes);
            }
        }

        Some(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct IbvSendQueue {
    inner: VecDeque<SendQueueElem>,
}

impl IbvSendQueue {
    pub(crate) fn push(&mut self, elem: SendQueueElem) {
        self.inner.push_back(elem);
    }

    pub(crate) fn pop_until(&mut self, psn: Psn) {
        let mut a = self.inner.partition_point(|x| x.psn < psn);
        let _drop = self.inner.drain(..a);
    }

    /// Find range [`psn_low`, `psn_high`)
    pub(crate) fn range(&self, psn_low: Psn, psn_high: Psn) -> Vec<SendQueueElem> {
        let mut a = self.inner.partition_point(|x| x.psn <= psn_low);
        let mut b = self.inner.partition_point(|x| x.psn < psn_high);
        a = a.saturating_sub(1);
        self.inner.range(a..b).copied().collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SendQueueElem {
    psn: Psn,
    wr: SendWrRdma,
    qp_param: QpParams,
}

impl SendQueueElem {
    pub(crate) fn new(wr: SendWrRdma, psn: Psn, qp_param: QpParams) -> Self {
        Self { psn, wr, qp_param }
    }

    pub(crate) fn psn(&self) -> Psn {
        self.psn
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
