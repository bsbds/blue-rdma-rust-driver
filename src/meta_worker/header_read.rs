use tracing::error;

use crate::{
    completion::{Event, MessageMeta, RecvEvent, RecvEventOp},
    constants::PSN_MASK,
    device_protocol::{HeaderReadMeta, WorkReqOpCode},
    send::{SendWrBase, SendWrRdma},
};

use super::{CompletionTask, MetaWorker, RdmaWriteTask};

impl<T> MetaWorker<T> {
    pub(super) fn handle_header_read(&mut self, meta: HeaderReadMeta) {
        if meta.ack_req {
            let end_psn = (meta.psn + 1) % PSN_MASK;
            let event = Event::Recv(RecvEvent::new(
                RecvEventOp::WriteAckReq,
                MessageMeta::new(meta.msn, end_psn),
            ));
            let _ignore = self.completion_tx.send(CompletionTask::Register {
                qpn: meta.dqpn,
                event,
            });
            let tracker = self
                .recv_table
                .get_mut(meta.dqpn)
                .unwrap_or_else(|| unreachable!());
            if let Some(base_psn) = tracker.ack_one(meta.psn) {
                let __ignore = self.completion_tx.send(CompletionTask::Ack {
                    qpn: meta.dqpn,
                    base_psn,
                    is_send: false,
                });
            }
        }

        let flags = if meta.ack_req {
            ibverbs_sys::ibv_send_flags::IBV_SEND_SOLICITED.0
        } else {
            0
        };

        let base = SendWrBase::new(
            0,
            flags,
            meta.raddr,
            meta.total_len,
            meta.rkey,
            0,
            WorkReqOpCode::RdmaReadResp,
        );
        let send_wr = SendWrRdma::new_from_base(base, meta.laddr, meta.lkey);
        let (task, _) = RdmaWriteTask::new(meta.dqpn, send_wr);
        let _ignore = self.rdma_write_tx.send(task);
    }
}
