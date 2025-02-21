use std::io;

use crate::{
    completion_v3::{Completion, CompletionTask, Event, MessageMeta, SendEvent, SendEventOp},
    constants::PSN_MASK,
    device_protocol::{ChunkPos, QpParams, WorkReqOpCode, WorkReqSend, WrChunkBuilder},
    fragmenter::PacketFragmenter,
    packet_retransmit::PacketRetransmitTask,
    protocol_impl_hardware::SendQueueScheduler,
    queue_pair::{num_psn, QueuePairAttrTable, SenderTable},
    send::{SendWrRdma, WrFragmenter},
    send_queue::SendQueueElem,
    timeout_retransmit::RetransmitTask,
};

pub(crate) struct RdmaWriteTask {
    qpn: u32,
    wr: SendWrRdma,
    opcode: WorkReqOpCode,
    resp_tx: oneshot::Sender<io::Result<()>>,
}

impl RdmaWriteTask {
    pub(crate) fn new(
        qpn: u32,
        wr: SendWrRdma,
        opcode: WorkReqOpCode,
    ) -> (Self, oneshot::Receiver<io::Result<()>>) {
        let (resp_tx, resp_rx) = oneshot::channel();
        (
            Self {
                qpn,
                wr,
                opcode,
                resp_tx,
            },
            resp_rx,
        )
    }
}

pub(crate) struct RdmaWriteWorker {
    rdma_write_rx: flume::Receiver<RdmaWriteTask>,
    sender_table: SenderTable,
    qp_attr_table: QueuePairAttrTable,
    send_scheduler: SendQueueScheduler,
    retransmit_tx: flume::Sender<RetransmitTask>,
    packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
    completion_tx: flume::Sender<CompletionTask>,
}

impl RdmaWriteWorker {
    pub(crate) fn new(
        rdma_write_rx: flume::Receiver<RdmaWriteTask>,
        qp_attr_table: QueuePairAttrTable,
        send_scheduler: SendQueueScheduler,
        retransmit_tx: flume::Sender<RetransmitTask>,
        packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
        completion_tx: flume::Sender<CompletionTask>,
    ) -> Self {
        Self {
            rdma_write_rx,
            sender_table: SenderTable::new(),
            qp_attr_table,
            send_scheduler,
            retransmit_tx,
            packet_retransmit_tx,
            completion_tx,
        }
    }

    pub(crate) fn spawn(self) {
        let _handle = std::thread::Builder::new()
            .name("rdma-write-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    fn run(mut self) {
        while let Ok(task) = self.rdma_write_rx.recv() {
            let RdmaWriteTask {
                qpn,
                wr,
                opcode,
                resp_tx,
            } = task;
            #[allow(clippy::wildcard_enum_match_arm)]
            let resp = match opcode {
                WorkReqOpCode::RdmaWrite
                | WorkReqOpCode::RdmaWriteWithImm
                | WorkReqOpCode::Send
                | WorkReqOpCode::SendWithImm
                | WorkReqOpCode::RdmaReadResp => self.write(qpn, wr, opcode),
                WorkReqOpCode::RdmaRead => self.rdma_read(qpn, wr),
                _ => unreachable!("opcode unsupported"),
            };
            resp_tx.send(resp);
        }
    }

    fn rdma_read(&self, qpn: u32, wr: SendWrRdma) -> io::Result<()> {
        let qp = self
            .qp_attr_table
            .get(qpn)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;

        let addr = wr.raddr();
        let length = wr.length();
        let num_psn = 1;
        let (msn, psn) = self
            .sender_table
            .map_qp_mut(qpn, |sender| sender.next_wr(num_psn))
            .flatten()
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let end_psn = (psn + num_psn) & PSN_MASK;
        let qp_params = QpParams::new(
            msn,
            qp.qp_type,
            qp.qpn,
            qp.mac_addr,
            qp.dqpn,
            qp.dqp_ip,
            qp.pmtu,
        );
        let chunk = WrChunkBuilder::new_with_opcode(WorkReqOpCode::RdmaRead)
            .set_qp_params(qp_params)
            .set_ibv_params(
                wr.send_flags() as u8,
                wr.rkey(),
                wr.length(),
                wr.lkey(),
                wr.imm(),
            )
            .set_chunk_meta(psn, wr.laddr(), wr.raddr(), wr.length(), ChunkPos::Only)
            .build();
        let flags = wr.send_flags();
        let mut ack_req = false;
        if flags & ibverbs_sys::ibv_send_flags::IBV_SEND_SIGNALED.0 != 0 {
            ack_req = true;
            let wr_id = wr.wr_id();
            let send_cq_handle = qp
                .send_cq
                .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
            let event = Event::Send(SendEvent::new(
                SendEventOp::ReadSignaled,
                MessageMeta::new(msn, end_psn),
                wr_id,
            ));
            self.completion_tx
                .send(CompletionTask::Register { qpn, event });
        }

        if ack_req {
            let _ignore = self.retransmit_tx.send(RetransmitTask::NewAckReq {
                qpn,
                last_packet_chunk: chunk,
            });
        }

        let _ignore = self.packet_retransmit_tx.send(PacketRetransmitTask::NewWr {
            qpn,
            wr: SendQueueElem::new(psn, wr, qp_params),
        });

        self.send_scheduler.send(chunk)?;

        Ok(())
    }

    fn write(&self, qpn: u32, wr: SendWrRdma, opcode: WorkReqOpCode) -> io::Result<()> {
        let qp = self
            .qp_attr_table
            .get(qpn)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let addr = wr.raddr();
        let length = wr.length();
        let num_psn =
            num_psn(qp.pmtu, addr, length).ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let (msn, psn) = self
            .sender_table
            .map_qp_mut(qpn, |sender| sender.next_wr(num_psn))
            .flatten()
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let end_psn = (psn + num_psn) & PSN_MASK;
        let flags = wr.send_flags();
        let mut ack_req = false;
        if flags & ibverbs_sys::ibv_send_flags::IBV_SEND_SIGNALED.0 != 0 {
            ack_req = true;
            let wr_id = wr.wr_id();
            let send_cq_handle = qp
                .send_cq
                .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
            #[allow(clippy::wildcard_enum_match_arm)]
            let op = match opcode {
                WorkReqOpCode::RdmaWrite | WorkReqOpCode::RdmaWriteWithImm => {
                    SendEventOp::WriteSignaled
                }
                WorkReqOpCode::Send | WorkReqOpCode::SendWithImm => SendEventOp::SendSignaled,
                WorkReqOpCode::RdmaRead => SendEventOp::ReadSignaled,
                _ => return Err(io::ErrorKind::Unsupported.into()),
            };
            let event = Event::Send(SendEvent::new(op, MessageMeta::new(msn, end_psn), wr_id));
            self.completion_tx
                .send(CompletionTask::Register { qpn, event });
        }
        let qp_params = QpParams::new(
            msn,
            qp.qp_type,
            qp.qpn,
            qp.mac_addr,
            qp.dqpn,
            qp.dqp_ip,
            qp.pmtu,
        );

        if ack_req {
            let fragmenter = PacketFragmenter::new(wr, qp_params, psn);
            let Some(last_packet_chunk) = fragmenter.into_iter().last() else {
                return Ok(());
            };
            let _ignore = self.retransmit_tx.send(RetransmitTask::NewAckReq {
                qpn,
                last_packet_chunk,
            });
        }

        let _ignore = self.packet_retransmit_tx.send(PacketRetransmitTask::NewWr {
            qpn,
            wr: SendQueueElem::new(psn, wr, qp_params),
        });

        let builder = WrChunkBuilder::new_with_opcode(opcode).set_qp_params(qp_params);
        let fragmenter = WrFragmenter::new(wr, builder, psn);
        for chunk in fragmenter {
            self.send_scheduler.send(chunk)?;
        }

        Ok(())
    }
}
