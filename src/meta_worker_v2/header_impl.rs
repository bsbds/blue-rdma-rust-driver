use tracing::error;

use crate::{
    ack_responder::AckResponse,
    completion_v2::CompletionEvent,
    device_protocol::{HeaderReadMeta, HeaderType, HeaderWriteMeta, PacketPos, WorkReqOpCode},
    message_worker::Task,
    queue_pair::num_psn,
    send::{SendWrBase, SendWrRdma},
    tracker::{MessageMeta, Msn},
};

use super::{CompletionTask, MetaWorker, RdmaWriteTask};

impl<T> MetaWorker<T> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_header_write(&mut self, meta: HeaderWriteMeta) {
        let HeaderWriteMeta {
            pos,
            msn,
            psn,
            solicited,
            ack_req,
            is_retry,
            dqpn,
            total_len,
            raddr,
            rkey,
            imm,
            header_type,
        } = meta;
        let tracker = self
            .recv_table
            .get_mut(dqpn)
            .unwrap_or_else(|| unreachable!("qp number: d{dqpn} does not exist"));

        if matches!(pos, PacketPos::Last | PacketPos::Only) {
            match header_type {
                HeaderType::Write | HeaderType::ReadResp => {}
                HeaderType::WriteWithImm => {
                    let _ignore = self.completion_tx.send(CompletionTask::Register {
                        event: CompletionEvent::new_recv_rdma_with_imm(dqpn, msn, psn, imm),
                        is_send: false,
                    });
                }
                HeaderType::Send => {
                    let _ignore = self.completion_tx.send(CompletionTask::Register {
                        event: CompletionEvent::new_recv(dqpn, msn, psn),
                        is_send: false,
                    });
                }
            }
        }
        if let Some(psn) = tracker.ack_one(psn) {
            let _ignore = self.completion_tx.send(CompletionTask::UpdateBasePsn {
                qpn: dqpn,
                psn,
                is_send: false,
            });
        }
        /// Timeout of an `AckReq` message, notify retransmission
        if matches!(pos, PacketPos::Last | PacketPos::Only) && is_retry && ack_req {
            let _ignore = self.ack_tx.send(AckResponse::Nak {
                qpn: dqpn,
                base_psn: tracker.base_psn(),
                ack_req_packet_psn: psn,
            });
        }
    }

    pub(super) fn handle_header_read(&self, meta: HeaderReadMeta) {
        let flags = if meta.ack_req {
            ibverbs_sys::ibv_send_flags::IBV_SEND_SOLICITED.0
        } else {
            0
        };
        let base = SendWrBase::new(0, flags, meta.raddr, meta.total_len, meta.rkey, 0);
        let send_wr = SendWrRdma::new_from_base(base, meta.laddr, meta.lkey);
        let (task, _) = RdmaWriteTask::new(meta.dqpn, send_wr, WorkReqOpCode::RdmaReadResp);
        let _ignore = self.rdma_write_tx.send(task);
    }
}
