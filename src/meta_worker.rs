use std::{
    hint, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tracing::error;

use crate::{
    qp::QpManager,
    queue::abstr::{MetaReport, ReportMeta},
};

/// A worker for processing packet meta
struct MetaWorker<T> {
    /// Inner meta report queue
    inner: T,
    /// Manages QPs
    qp_manager: QpManager,
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
                msn,
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
                qp.ack_range(psn_now, now_bitmap);
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
