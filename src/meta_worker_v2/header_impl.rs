use tracing::error;

use crate::{
    ack_responder::AckResponse,
    device_protocol::{HeaderWriteMeta, PacketPos},
    message_worker::Task,
    tracker::{MessageMeta, Msn},
};

use super::MetaWorker;

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
        } = meta;
        let Some(tracker) = self.recv_table.get_mut(dqpn) else {
            error!("qp number: d{dqpn} does not exist");
            return;
        };
        /// new messages
        if matches!(pos, PacketPos::First | PacketPos::Only) {
            let task = Task::AppendMessage {
                qpn: dqpn,
                meta: MessageMeta::new(Msn(msn), psn, ack_req),
            };
            let _ignore = self.receiver_task_tx.send(task);
        }
        if let Some(psn) = tracker.ack_one(psn) {
            let task = Task::UpdateBasePsn { qpn: dqpn, psn };
            self.receiver_task_tx.send(task);
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
}
