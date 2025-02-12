use std::{
    hint, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use tracing::error;

use crate::{
    ack_responder::AckResponse,
    constants::PSN_MASK,
    device_protocol::{MetaReport, PacketPos, ReportMeta},
    message_worker::Task,
    packet_retransmit::PacketRetransmitTask,
    queue_pair::{Tracker, TrackerTable},
    timeout_retransmit::RetransmitTask,
    tracker::{MessageMeta, Msn},
};

/// Offset between the `now_psn` an `base_psn`
const BASE_PSN_OFFSET: u32 = 0x70;

/// A worker for processing packet meta
pub(crate) struct MetaWorker<T> {
    /// Inner meta report queue
    inner: T,
    send_table: TrackerTable,
    recv_table: TrackerTable,
    sender_task_tx: flume::Sender<Task>,
    receiver_task_tx: flume::Sender<Task>,
    ack_tx: flume::Sender<AckResponse>,
    retransmit_tx: flume::Sender<RetransmitTask>,
    packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
}

impl<T: MetaReport + Send + 'static> MetaWorker<T> {
    pub(crate) fn new(
        inner: T,
        sender_task_tx: flume::Sender<Task>,
        receiver_task_tx: flume::Sender<Task>,
        ack_tx: flume::Sender<AckResponse>,
        retransmit_tx: flume::Sender<RetransmitTask>,
        packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
    ) -> Self {
        Self {
            inner,
            ack_tx,
            sender_task_tx,
            receiver_task_tx,
            retransmit_tx,
            packet_retransmit_tx,
            send_table: TrackerTable::new(),
            recv_table: TrackerTable::new(),
        }
    }

    pub(crate) fn spawn(self, is_shutdown: Arc<AtomicBool>) {
        let _handle = thread::Builder::new()
            .name("meta-worker".into())
            .spawn(move || self.run(is_shutdown))
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    #[allow(clippy::needless_pass_by_value)] // consume the flag
    /// Run the handler loop
    fn run(mut self, is_shutdown: Arc<AtomicBool>) -> io::Result<()> {
        while !is_shutdown.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(1));
            if let Some(meta) = self.inner.try_recv_meta()? {
                self.handle_meta(meta);
            };
        }

        Ok(())
    }

    fn handle_meta(&mut self, meta: ReportMeta) {
        match meta {
            ReportMeta::Write {
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
            } => {
                self.handle_header_write(
                    pos, msn, psn, solicited, ack_req, dqpn, total_len, raddr, is_retry,
                );
            }
            ReportMeta::Ack {
                qpn,
                msn,
                psn_now,
                now_bitmap,
                is_window_slided,
                is_send_by_local_hw,
                is_send_by_driver,
            } => self.handle_ack(qpn, msn, psn_now, now_bitmap, is_send_by_local_hw),
            ReportMeta::Nak {
                qpn,
                msn,
                psn_now,
                now_bitmap,
                pre_bitmap,
                psn_before_slide,
                is_send_by_local_hw,
                is_send_by_driver,
            } => {}
            ReportMeta::Read { .. } => todo!(),
            ReportMeta::Cnp { .. } => todo!(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_header_write(
        &mut self,
        pos: PacketPos,
        msn: u16,
        psn: u32,
        solicited: bool,
        ack_req: bool,
        dqpn: u32,
        total_len: u32,
        raddr: u64,
        is_retry: bool,
    ) {
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
        if matches!(pos, PacketPos::Last) && is_retry && ack_req {
            let _ignore = self.ack_tx.send(AckResponse::Nak {
                qpn: dqpn,
                base_psn: tracker.base_psn(),
                ack_req_packet_psn: psn,
            });
        }
    }

    fn handle_ack(
        &mut self,
        qpn: u32,
        ack_msn: u16,
        psn_now: u32,
        now_bitmap: u128,
        is_send_by_local_hw: bool,
    ) {
        if !is_send_by_local_hw {
            let _ignore = self.retransmit_tx.send(RetransmitTask::ReceiveACK { qpn });
        }
        let (table, task_tx) = if is_send_by_local_hw {
            (&mut self.recv_table, &self.receiver_task_tx)
        } else {
            (&mut self.send_table, &self.sender_task_tx)
        };

        let Some(tracker) = table.get_mut(qpn) else {
            error!("qp number: {qpn} does not exist");
            return;
        };
        if let Some(psn) = Self::update_packet_tracker(tracker, psn_now, now_bitmap, ack_msn) {
            self.send_update(task_tx, qpn, psn);
        }
    }

    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    fn handle_nak(
        &mut self,
        qpn: u32,
        ack_msn: u16,
        psn_now: u32,
        now_bitmap: u128,
        psn_pre: u32,
        pre_bitmap: u128,
        is_send_by_local_hw: bool,
    ) {
        let (table, task_tx) = if is_send_by_local_hw {
            (&mut self.recv_table, &self.receiver_task_tx)
        } else {
            (&mut self.send_table, &self.sender_task_tx)
        };
        let Some(tracker) = table.get_mut(qpn) else {
            error!("qp number: {qpn} does not exist");
            return;
        };
        let x = Self::update_packet_tracker(tracker, psn_now, now_bitmap, ack_msn);
        let y = Self::update_packet_tracker(tracker, psn_pre, pre_bitmap, ack_msn);
        for psn in x.into_iter().chain(y) {
            self.send_update(task_tx, qpn, psn);
        }
        // TODO: implement more fine-grained retransmission
        let psn_low = psn_pre.wrapping_sub(BASE_PSN_OFFSET) & PSN_MASK;
        let psn_high = psn_now.wrapping_add(128).wrapping_sub(BASE_PSN_OFFSET);
        let _ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::RetransmitRange {
                qpn,
                psn_low,
                psn_high,
            });
    }

    fn select_table(
        &mut self,
        is_send_by_local_hw: bool,
    ) -> (&mut TrackerTable, &flume::Sender<Task>) {
        if is_send_by_local_hw {
            (&mut self.recv_table, &self.receiver_task_tx)
        } else {
            (&mut self.send_table, &self.sender_task_tx)
        }
    }

    fn update_packet_tracker(
        tracker: &mut Tracker,
        psn: u32,
        bitmap: u128,
        ack_msn: u16,
    ) -> Option<u32> {
        let base_psn = psn.wrapping_sub(BASE_PSN_OFFSET) & PSN_MASK;
        tracker.ack_range(base_psn, bitmap, ack_msn)
    }

    fn send_update(&self, tx: &flume::Sender<Task>, qpn: u32, psn: u32) {
        let task = Task::UpdateBasePsn { qpn, psn };
        let _ignore = tx.send(task);
        let __ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::Ack { qpn, psn });
    }
}
