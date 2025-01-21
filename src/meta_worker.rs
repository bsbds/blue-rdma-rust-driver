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
    completion::{CqManager, MetaCqTable},
    qp::{QpManager, QpTrackerTable},
    queue::abstr::{MetaReport, PacketPos, ReportMeta},
    retransmission::message_tracker::MessageTracker,
};

/// Offset between the `now_psn` an `base_psn`
const BASE_PSN_OFFSET: u32 = 0x70;

pub(crate) struct Launch<M> {
    /// Abstract Tunnel
    inner: MetaWorker<M>,
}

impl<M: MetaReport> Launch<M> {
    /// Creates a new `Launch`
    pub(crate) fn new(inner: M, qp_trackers: QpTrackerTable, cq_table: MetaCqTable) -> Self {
        Self {
            inner: MetaWorker {
                inner,
                qp_trackers,
                cq_table,
            },
        }
    }

    /// Launches the worker thread that handles communication between the NIC device and tunnel
    pub(crate) fn launch(self, is_shutdown: Arc<AtomicBool>) {
        let _ignore = thread::spawn(|| self.inner.run(is_shutdown));
    }
}

/// A worker for processing packet meta
struct MetaWorker<T> {
    /// Inner meta report queue
    inner: T,
    /// Manages QPs
    qp_trackers: QpTrackerTable,
    /// Manages CQs
    cq_table: MetaCqTable,
}

impl<T: MetaReport> MetaWorker<T> {
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

    /// Handles the meta event
    #[allow(clippy::needless_pass_by_value)]
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
                let Some(qp) = self.qp_trackers.state_mut(dqpn) else {
                    error!("qp number: d{dqpn} does not exist");
                    return;
                };
                qp.ack_one(psn);
                match pos {
                    PacketPos::First | PacketPos::Only => {
                        let psn_total = qp
                            .num_psn(raddr, total_len)
                            .unwrap_or_else(|| unreachable!("parameters should be valid"));
                        let end_psn = psn.wrapping_add(psn_total);
                        qp.message_tracker().insert(msn, end_psn);
                    }
                    PacketPos::Middle | PacketPos::Last => {}
                };
            }
            ReportMeta::Read {
                raddr,
                rkey,
                total_len,
                laddr,
                lkey,
            } => todo!(),
            ReportMeta::Cnp { qpn } => todo!(),
            ReportMeta::Ack {
                qpn,
                msn: ack_msn,
                psn_now,
                now_bitmap,
                is_window_slided,
                is_send_by_local_hw,
                is_send_by_driver,
            } => {
                let Some(qp) = self.qp_trackers.state_mut(qpn) else {
                    error!("qp number: {qpn} does not exist");
                    return;
                };
                let base_psn = psn_now.wrapping_sub(BASE_PSN_OFFSET);
                if let Some(psn) = qp.ack_range(psn_now, now_bitmap, ack_msn) {
                    let last_msn_acked = qp.message_tracker().ack(psn);
                    let cq_handle = if is_send_by_local_hw {
                        qp.send_cq_handle()
                    } else {
                        qp.recv_cq_handle()
                    };
                    if let Some(cq) = cq_handle.and_then(|h| self.cq_table.get_mut(h)) {
                        if let Some(last_msn_acked) = last_msn_acked {
                            cq.ack_event(last_msn_acked, qp.qpn());
                        }
                    }
                }
            }
            ReportMeta::Nak {
                qpn,
                msn,
                psn_now,
                now_bitmap,
                pre_bitmap,
                psn_before_slide,
                is_send_by_local_hw,
                is_send_by_driver,
            } => todo!(),
        }
    }
}
