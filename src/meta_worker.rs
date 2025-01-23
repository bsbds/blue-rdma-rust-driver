use std::{
    hint, io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use bilge::prelude::*;
use parking_lot::Mutex;
use pnet::{
    packet::ethernet::{EtherTypes, MutableEthernetPacket},
    util::MacAddr,
};
use tracing::error;

use crate::{
    completion::{CqManager, MetaCqTable},
    constants::PSN_MASK,
    qp::{QpManager, QpTrackerTable},
    queue::abstr::{FrameTx, MetaReport, PacketPos, ReportMeta},
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
    pub(crate) fn new<F: FrameTx>(
        inner: M,
        qp_trackers: QpTrackerTable,
        cq_table: MetaCqTable,
        raw_frame_tx: F,
    ) -> Self {
        Self {
            inner: MetaWorker {
                inner,
                qp_trackers,
                cq_table,
                raw_frame_tx: Box::new(raw_frame_tx),
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
    /// Raw frame tx
    raw_frame_tx: Box<dyn FrameTx>,
}

impl<T: MetaReport> MetaWorker<T> {
    #[allow(clippy::needless_pass_by_value)] // consume the flag
    /// Run the handler loop
    fn run(mut self, is_shutdown: Arc<AtomicBool>) -> io::Result<()> {
        while !is_shutdown.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
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
                    PacketPos::First => {
                        let psn_total = qp
                            .num_psn(raddr, total_len)
                            .unwrap_or_else(|| unreachable!("parameters should be valid"));
                        let end_psn = psn.wrapping_add(psn_total);
                        qp.insert_messsage(msn, ack_req, end_psn);
                    }
                    PacketPos::Only => {
                        let send_cq = qp.send_cq_handle();
                        if let Some(cq) = send_cq.and_then(|h| self.cq_table.get_mut(h)) {
                            cq.ack_event(msn, qp.qpn());
                        }
                        if ack_req {
                            let now_psn = psn.wrapping_sub(128 - BASE_PSN_OFFSET) & PSN_MASK;
                            let ack_frame =
                                AckFrameBuilder::build_ack(now_psn, u128::MAX, qp.dqpn());
                            if let Err(e) = self.raw_frame_tx.send(&ack_frame) {
                                tracing::error!("failed to send ack frame");
                            }
                        }
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
                    let msns_acked = qp.ack_message(psn);
                    let require_ack = msns_acked.iter().any(|&(_, x)| x);
                    if require_ack {
                        let now_psn = base_psn.wrapping_sub(128 - BASE_PSN_OFFSET) & PSN_MASK;
                        let ack_frame = AckFrameBuilder::build_ack(now_psn, u128::MAX, qp.dqpn());
                        if let Err(e) = self.raw_frame_tx.send(&ack_frame) {
                            tracing::error!("failed to send ack frame");
                        }
                    }
                    let last_msn_acked = msns_acked.last().map(|&(m, _)| m);
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

#[bitsize(32)]
#[derive(Default, Clone, Copy, DebugBits, FromBits)]
pub(crate) struct AethSeg0 {
    pre_psn: u24,
    resv0: u5,
    is_send_by_driver: bool,
    is_window_slided: bool,
    is_packet_loss: bool,
}

#[bitsize(96)]
#[derive(Default, Clone, Copy, DebugBits, FromBits)]
pub(crate) struct Bth {
    psn: u24,
    resv7: u7,
    ack_req: bool,
    dqpn: u24,
    resv6: u6,
    becn: bool,
    fecn: bool,
    msn: u16,
    tver: u4,
    pad_cnt: u2,
    is_retry: bool,
    solicited: bool,
    opcode: u5,
    trans_type: u3,
}

struct AckFrameBuilder;

impl AckFrameBuilder {
    fn build_ack(now_psn: u32, now_bitmap: u128, dqpn: u32) -> Vec<u8> {
        const TRANS_TYPE_RC: u8 = 0x00;
        const OPCODE_ACKNOWLEDGE: u8 = 0x11;
        const PAYLOAD_SIZE: usize = 48;
        let mac = MacAddr::new(0x0A, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA);
        let mut payload = [0u8; PAYLOAD_SIZE];

        let mut bth = Bth::default();
        bth.set_opcode(u5::from_u8(OPCODE_ACKNOWLEDGE));
        bth.set_psn(u24::from_u32(now_psn));
        bth.set_dqpn(u24::from_u32(dqpn));
        payload[..12].copy_from_slice(&bth.value.to_le_bytes());

        let mut aeth_seg0 = AethSeg0::default();
        aeth_seg0.set_is_send_by_driver(true);
        payload[12..16].copy_from_slice(&aeth_seg0.value.to_le_bytes());
        payload[16..32].copy_from_slice(&now_bitmap.to_le_bytes());
        payload[32..].copy_from_slice(&0u128.to_le_bytes());

        Self::build_ethernet_frame(mac, mac, &payload)
    }

    fn build_ethernet_frame(src_mac: MacAddr, dst_mac: MacAddr, payload: &[u8]) -> Vec<u8> {
        const ETHERNET_HEADER_LEN: usize = 14;
        let mut buffer = vec![0u8; ETHERNET_HEADER_LEN.wrapping_add(payload.len())];
        let mut frame = MutableEthernetPacket::new(&mut buffer)
            .unwrap_or_else(|| unreachable!("Failed to create ethernet packet"));

        frame.set_source(src_mac);
        frame.set_destination(dst_mac);
        frame.set_ethertype(EtherTypes::Ipv4);
        frame.set_payload(payload);

        buffer
    }
}
