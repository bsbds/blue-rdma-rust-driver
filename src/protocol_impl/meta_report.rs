use std::io;

use tracing::error;

use crate::{
    constants::PSN_MASK,
    device_protocol::{
        AckMetaLocalHw, AckMetaRemoteDriver, CnpMeta, HeaderReadMeta, HeaderWriteMeta, MetaReport,
        NakMetaLocalHw, NakMetaRemoteDriver, NakMetaRemoteHw, ReportMeta,
    },
};

use super::{
    device::{proxy::MetaReportQueueProxy, CsrReaderAdaptor, DeviceAdaptor, RingBufferCsrAddr},
    queue::meta_report_queue::{MetaReportQueue, MetaReportQueueDesc},
};

pub(crate) struct MetaReportQueueCtx<Dev> {
    queue: MetaReportQueue,
    proxy: MetaReportQueueProxy<Dev>,
}

impl<Dev> MetaReportQueueCtx<Dev> {
    pub(crate) fn new(queue: MetaReportQueue, proxy: MetaReportQueueProxy<Dev>) -> Self {
        Self { queue, proxy }
    }
}

/// Handler for meta report queues
pub(crate) struct MetaReportQueueHandler<Dev> {
    /// All four meta report queues
    inner: Vec<MetaReportQueueCtx<Dev>>,
    /// Current position, used for round robin polling
    pos: usize,
}

impl<Dev> MetaReportQueueHandler<Dev> {
    pub(crate) fn new(inner: Vec<MetaReportQueueCtx<Dev>>) -> Self {
        Self { inner, pos: 0 }
    }

    fn remap_psn(psn: u32) -> u32 {
        // 128 (window size) - 16 (first stride)
        const OFFSET: u32 = 112;
        psn.wrapping_sub(OFFSET) & PSN_MASK
    }
}

impl<Dev: DeviceAdaptor> MetaReport for MetaReportQueueHandler<Dev> {
    #[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // should never overflow
    fn try_recv_meta(&mut self) -> io::Result<Option<ReportMeta>> {
        let num_queues = self.inner.len();
        for i in 0..num_queues {
            let idx = (self.pos + i) % num_queues;
            let ctx = &mut self.inner[idx];
            let Some(desc) = ctx.queue.try_pop() else {
                continue;
            };
            let _ignore = ctx.proxy.write_tail(ctx.queue.tail());
            if let Ok(head_ptr) = ctx.proxy.read_head() {
                ctx.queue.set_head(head_ptr);
            }

            self.pos = (idx + 1) % num_queues;
            let meta = match desc {
                MetaReportQueueDesc::WritePacketInfo(d) => {
                    ReportMeta::HeaderWrite(HeaderWriteMeta {
                        pos: d.packet_pos(),
                        msn: d.msn(),
                        psn: d.psn(),
                        solicited: d.solicited(),
                        ack_req: d.ack_req(),
                        is_retry: d.is_retry(),
                        dqpn: d.dqpn(),
                        total_len: d.total_len(),
                        raddr: d.raddr(),
                        rkey: d.rkey(),
                        imm: d.imm_data(),
                        header_type: d.header_type(),
                    })
                }
                MetaReportQueueDesc::ReadPacketInfo((f, n)) => {
                    ReportMeta::HeaderRead(HeaderReadMeta {
                        dqpn: f.dqpn(),
                        raddr: f.raddr(),
                        rkey: f.rkey(),
                        total_len: n.total_len(),
                        laddr: n.laddr(),
                        lkey: n.lkey(),
                        ack_req: f.ack_req(),
                        msn: f.msn(),
                        psn: f.psn(),
                    })
                }
                MetaReportQueueDesc::CnpPacketInfo(d) => ReportMeta::Cnp(CnpMeta { qpn: d.dqpn() }),
                MetaReportQueueDesc::Ack(d) => {
                    match (d.is_send_by_driver(), d.is_send_by_local_hw()) {
                        (true, false) => ReportMeta::AckRemoteDriver(AckMetaRemoteDriver {
                            qpn: d.qpn(),
                            psn_now: d.psn_now(),
                        }),
                        (false, true) => ReportMeta::AckLocalHw(AckMetaLocalHw {
                            qpn: d.qpn(),
                            psn_now: Self::remap_psn(d.psn_now()),
                            now_bitmap: d.now_bitmap(),
                        }),
                        (false, false) | (true, true) => unreachable!("invalid ack branch"),
                    }
                }
                MetaReportQueueDesc::Nak((f, n)) => {
                    match (f.is_send_by_driver(), f.is_send_by_local_hw()) {
                        (true, false) => ReportMeta::NakRemoteDriver(NakMetaRemoteDriver {
                            qpn: f.qpn(),
                            psn_now: f.psn_now(),
                            psn_pre: f.psn_before_slide(),
                        }),
                        (false, true) => ReportMeta::NakLocalHw(NakMetaLocalHw {
                            qpn: f.qpn(),
                            msn: f.msn(),
                            psn_now: Self::remap_psn(f.psn_now()),
                            now_bitmap: f.now_bitmap(),
                            psn_pre: Self::remap_psn(f.psn_before_slide()),
                            pre_bitmap: n.pre_bitmap(),
                        }),
                        (false, false) => ReportMeta::NakRemoteHw(NakMetaRemoteHw {
                            qpn: f.qpn(),
                            msn: f.msn(),
                            psn_now: Self::remap_psn(f.psn_now()),
                            now_bitmap: f.now_bitmap(),
                            psn_pre: Self::remap_psn(f.psn_before_slide()),
                            pre_bitmap: n.pre_bitmap(),
                        }),
                        (true, true) => unreachable!("invalid nak branch"),
                    }
                }
            };
            return Ok(Some(meta));
        }
        Ok(None)
    }
}
