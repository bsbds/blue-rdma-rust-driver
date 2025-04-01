use crate::{
    ack_responder::AckResponse,
    completion::{CompletionTask, Event, MessageMeta, RecvEvent, RecvEventOp},
    constants::PSN_MASK,
    device_protocol::{
        AckMetaLocalHw, AckMetaRemoteDriver, HeaderReadMeta, HeaderType, HeaderWriteMeta,
        NakMetaLocalHw, NakMetaRemoteDriver, NakMetaRemoteHw, PacketPos, WorkReqOpCode,
    },
    packet_retransmit::PacketRetransmitTask,
    qp_table::QpTable,
    rdma_write_worker::RdmaWriteTask,
    send::{SendWrBase, SendWrRdma},
    timeout_retransmit::RetransmitTask,
    tracker::{LocalAckTracker, RemoteAckTracker},
    utils::Psn,
};

use super::ReportMeta;

pub(crate) struct MetaHandler {
    pub(super) send_table: QpTable<RemoteAckTracker>,
    pub(super) recv_table: QpTable<LocalAckTracker>,
    pub(super) ack_tx: flume::Sender<AckResponse>,
    pub(super) retransmit_tx: flume::Sender<RetransmitTask>,
    pub(super) packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
    pub(super) completion_tx: flume::Sender<CompletionTask>,
    pub(super) rdma_write_tx: flume::Sender<RdmaWriteTask>,
}

impl MetaHandler {
    pub(crate) fn new(
        ack_tx: flume::Sender<AckResponse>,
        retransmit_tx: flume::Sender<RetransmitTask>,
        packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
        completion_tx: flume::Sender<CompletionTask>,
        rdma_write_tx: flume::Sender<RdmaWriteTask>,
    ) -> Self {
        Self {
            send_table: QpTable::new(),
            recv_table: QpTable::new(),
            ack_tx,
            retransmit_tx,
            packet_retransmit_tx,
            completion_tx,
            rdma_write_tx,
        }
    }

    pub(super) fn handle_meta(&mut self, meta: ReportMeta) -> Option<()> {
        match meta {
            ReportMeta::HeaderWrite(x) => self.handle_header_write(x),
            ReportMeta::HeaderRead(x) => self.handle_header_read(x),
            ReportMeta::AckLocalHw(x) => self.handle_ack_local_hw(x),
            ReportMeta::AckRemoteDriver(x) => self.handle_ack_remote_driver(x),
            ReportMeta::NakLocalHw(x) => self.handle_nak_local_hw(x),
            ReportMeta::NakRemoteHw(x) => self.handle_nak_remote_hw(x),
            ReportMeta::NakRemoteDriver(x) => self.handle_nak_remote_driver(x),
            ReportMeta::Cnp { .. } => todo!(),
        }
    }

    fn handle_ack_local_hw(&mut self, meta: AckMetaLocalHw) -> Option<()> {
        let tracker = self.recv_table.get_qp_mut(meta.qpn)?;
        if let Some(psn) = tracker.ack_bitmap(meta.psn_now, meta.now_bitmap) {
            self.receiver_updates(meta.qpn, psn);
        }

        Some(())
    }

    fn handle_ack_remote_driver(&mut self, meta: AckMetaRemoteDriver) -> Option<()> {
        let tracker = self.send_table.get_qp_mut(meta.qpn)?;
        if let Some(psn) = tracker.ack_before(meta.psn_now) {
            self.sender_updates(meta.qpn, psn);
        }

        Some(())
    }

    fn handle_nak_local_hw(&mut self, meta: NakMetaLocalHw) -> Option<()> {
        let tracker = self.recv_table.get_qp_mut(meta.qpn)?;
        if let Some(psn) =
            tracker.nak_bitmap(meta.psn_pre, meta.pre_bitmap, meta.psn_now, meta.now_bitmap)
        {
            self.receiver_updates(meta.qpn, psn);
        }

        Some(())
    }

    fn handle_nak_remote_hw(&mut self, meta: NakMetaRemoteHw) -> Option<()> {
        let tracker = self.send_table.get_qp_mut(meta.qpn)?;
        if let Some(psn) = tracker.nak_bitmap(
            meta.msn,
            meta.psn_pre,
            meta.pre_bitmap,
            meta.psn_now,
            meta.now_bitmap,
        ) {
            self.sender_updates(meta.qpn, psn);
        }

        let _ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::RetransmitRange {
                qpn: meta.qpn,
                psn_low: meta.psn_pre,
                psn_high: meta.psn_now + 128,
            });

        Some(())
    }

    #[allow(clippy::unnecessary_wraps)]
    fn handle_nak_remote_driver(&self, meta: NakMetaRemoteDriver) -> Option<()> {
        let _ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::RetransmitRange {
                qpn: meta.qpn,
                psn_low: meta.psn_pre,
                psn_high: meta.psn_now,
            });

        Some(())
    }

    pub(crate) fn sender_updates(&self, qpn: u32, base_psn: Psn) {
        let _ignore = self
            .completion_tx
            .send(CompletionTask::AckSend { qpn, base_psn });
        let __ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::Ack { qpn, psn: base_psn });
    }

    pub(crate) fn receiver_updates(&self, qpn: u32, base_psn: Psn) {
        let _ignore = self
            .completion_tx
            .send(CompletionTask::AckRecv { qpn, base_psn });
        let __ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::Ack { qpn, psn: base_psn });
    }

    pub(super) fn handle_header_read(&mut self, meta: HeaderReadMeta) -> Option<()> {
        if meta.ack_req {
            let end_psn = meta.psn + 1;
            let event = Event::Recv(RecvEvent::new(
                RecvEventOp::WriteAckReq,
                MessageMeta::new(meta.msn, end_psn),
            ));
            let _ignore = self.completion_tx.send(CompletionTask::Register {
                qpn: meta.dqpn,
                event,
            });
            let tracker = self.recv_table.get_qp_mut(meta.dqpn)?;
            if let Some(base_psn) = tracker.ack_one(meta.psn) {
                let __ignore = self.completion_tx.send(CompletionTask::AckRecv {
                    qpn: meta.dqpn,
                    base_psn,
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

        Some(())
    }

    pub(super) fn handle_header_write(&mut self, meta: HeaderWriteMeta) -> Option<()> {
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
        let tracker = self.recv_table.get_qp_mut(dqpn)?;

        if matches!(pos, PacketPos::Last | PacketPos::Only) {
            let end_psn = psn + 1;
            match header_type {
                HeaderType::Write => {}
                HeaderType::WriteWithImm => {
                    let event = Event::Recv(RecvEvent::new(
                        RecvEventOp::WriteWithImm { imm },
                        MessageMeta::new(msn, end_psn),
                    ));
                    let _ignore = self
                        .completion_tx
                        .send(CompletionTask::Register { qpn: dqpn, event });
                }
                HeaderType::Send => {
                    let event = Event::Recv(RecvEvent::new(
                        RecvEventOp::Recv,
                        MessageMeta::new(msn, end_psn),
                    ));
                    let _ignore = self
                        .completion_tx
                        .send(CompletionTask::Register { qpn: dqpn, event });
                }
                HeaderType::SendWithImm => {
                    let event = Event::Recv(RecvEvent::new(
                        RecvEventOp::RecvWithImm { imm },
                        MessageMeta::new(msn, end_psn),
                    ));
                    let _ignore = self
                        .completion_tx
                        .send(CompletionTask::Register { qpn: dqpn, event });
                }
                HeaderType::ReadResp => {
                    let event = Event::Recv(RecvEvent::new(
                        RecvEventOp::ReadResp,
                        MessageMeta::new(msn, end_psn),
                    ));
                    let _ignore = self
                        .completion_tx
                        .send(CompletionTask::Register { qpn: dqpn, event });
                }
            }
            if ack_req {
                let event = Event::Recv(RecvEvent::new(
                    RecvEventOp::WriteAckReq,
                    MessageMeta::new(msn, end_psn),
                ));
                let _ignore = self
                    .completion_tx
                    .send(CompletionTask::Register { qpn: dqpn, event });
            }
        }
        if let Some(base_psn) = tracker.ack_one(psn) {
            let _ignore = self.completion_tx.send(CompletionTask::AckRecv {
                qpn: dqpn,
                base_psn,
            });
        }
        /// Timeout of an `AckReq` message, notify retransmission
        if matches!(pos, PacketPos::Last | PacketPos::Only) && is_retry && ack_req {
            let _ignore = self.ack_tx.send(AckResponse::Nak {
                qpn: dqpn,
                base_psn: tracker.base_psn(),
                ack_req_packet_psn: psn - 1,
            });
        }

        Some(())
    }
}
