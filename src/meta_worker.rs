use std::{
    hint, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tracing::error;

use crate::{
    completion::CqManager,
    qp::QpManager,
    queue::abstr::{MetaReport, PacketPos, ReportMeta},
    retransmission::message_tracker::MessageTracker,
};

/// A worker for processing packet meta
struct MetaWorker<T> {
    /// Inner meta report queue
    inner: T,
    /// Manages QPs
    qp_manager: QpManager,
    /// Manages CQs
    cq_manager: CqManager,
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
                let Some(qp) = self.qp_manager.get_qp_mut(dqpn) else {
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
                //if let Some(end_psn) = end_psn {
                //    if qp.all_acked(end_psn) {
                //        qp.message_tracker().remove(msn);
                //        todo!("generate completion event");
                //    }
                //}
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
                let Some(qp) = self.qp_manager.get_qp_mut(qpn) else {
                    error!("qp number: {qpn} does not exist");
                    return;
                };
                if let Some(psn) = qp.ack_range(psn_now, now_bitmap, ack_msn) {
                    let acked_msns = qp.message_tracker().ack(psn);
                    let cq_handle = if is_send_by_local_hw {
                        qp.send_cq_handle()
                    } else {
                        qp.recv_cq_handle()
                    };
                    if let Some(cq) = cq_handle.and_then(|h| self.cq_manager.get_cq_mut(h)) {
                        for msn in acked_msns {
                            cq.ack_event(msn, qp.qpn());
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
