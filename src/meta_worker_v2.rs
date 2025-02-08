use std::{
    hint, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use tracing::error;

use crate::{
    constants::PSN_MASK,
    device_protocol::{MetaReport, PacketPos, ReportMeta},
    message_worker::Task,
    queue_pair::TrackerTable,
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
}

impl<T: MetaReport + Send + 'static> MetaWorker<T> {
    pub(crate) fn new(
        inner: T,
        sender_task_tx: flume::Sender<Task>,
        receiver_task_tx: flume::Sender<Task>,
    ) -> Self {
        Self {
            inner,
            sender_task_tx,
            receiver_task_tx,
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
            hint::spin_loop();
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
                self.handle_header_write(pos, msn, psn, solicited, ack_req, dqpn, total_len, raddr);
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
            ReportMeta::Read { .. } => todo!(),
            ReportMeta::Cnp { .. } => todo!(),
            ReportMeta::Nak { .. } => todo!(),
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
    }

    fn handle_ack(
        &mut self,
        qpn: u32,
        ack_msn: u16,
        psn_now: u32,
        now_bitmap: u128,
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
        let base_psn = psn_now.wrapping_sub(BASE_PSN_OFFSET) & PSN_MASK;
        if let Some(psn) = tracker.ack_range(base_psn, now_bitmap, ack_msn) {
            let task = Task::UpdateBasePsn { qpn, psn };
            task_tx.send(task);
        }
    }
}
