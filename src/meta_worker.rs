use std::{
    hint, io,
    net::Ipv4Addr,
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
    packet::{
        ethernet::{EtherTypes, MutableEthernetPacket},
        ip::IpNextHeaderProtocols,
        ipv4::{Ipv4Flags, MutableIpv4Packet},
        udp::MutableUdpPacket,
    },
    util::MacAddr,
};
use tracing::error;

use crate::{
    completion::{CompletionQueueTable, CqManager},
    constants::PSN_MASK,
    device_protocol::{
        AckMeta, CnpMeta, FrameTx, HeaderReadMeta, HeaderWriteMeta, MetaReport, NakMeta, PacketPos,
        ReportMeta,
    },
    qp::{QpManager, QpTrackerTable},
    retransmission::message_tracker::MessageTracker,
};

/// Offset between the `now_psn` an `base_psn`
const BASE_PSN_OFFSET: u32 = 0x70;

pub(crate) struct Launch<M> {
    /// Abstract Tunnel
    inner: MetaWorker<M>,
}

impl<M: MetaReport + Send + 'static> Launch<M> {
    /// Creates a new `Launch`
    pub(crate) fn new<F: FrameTx + Send + 'static>(
        inner: M,
        qp_trackers: QpTrackerTable,
        cq_table: CompletionQueueTable,
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
    cq_table: CompletionQueueTable,
    /// Raw frame tx
    raw_frame_tx: Box<dyn FrameTx + Send + 'static>,
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
            ReportMeta::Write(HeaderWriteMeta {
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
            }) => {
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
                        let end_psn = psn.wrapping_add(psn_total).wrapping_sub(1);
                        qp.insert_messsage(msn, ack_req, end_psn);
                    }
                    PacketPos::Only => {
                        let send_cq = qp.send_cq_handle();
                        if let Some(cq) = send_cq.and_then(|h| self.cq_table.get(h)) {
                            cq.ack_event(msn, qp.qpn(), false);
                        }
                        if ack_req {
                            let bitmap = 1u128 << BASE_PSN_OFFSET;
                            let ack_frame = AckFrameBuilder::build_ack(psn, bitmap, qp.dqpn());
                            if let Err(e) = self.raw_frame_tx.send(&ack_frame) {
                                tracing::error!("failed to send ack frame");
                            }
                        }
                    }
                    PacketPos::Middle | PacketPos::Last => {}
                };
            }
            ReportMeta::Read(HeaderReadMeta {
                raddr,
                rkey,
                total_len,
                laddr,
                lkey,
            }) => todo!(),
            ReportMeta::Cnp(CnpMeta { qpn }) => todo!(),
            ReportMeta::Ack(AckMeta {
                qpn,
                msn: ack_msn,
                psn_now,
                now_bitmap,
                is_window_slided,
                is_send_by_local_hw,
                is_send_by_driver,
            }) => {
                let Some(qp) = self.qp_trackers.state_mut(qpn) else {
                    error!("qp number: {qpn} does not exist");
                    return;
                };
                let base_psn = psn_now.wrapping_sub(BASE_PSN_OFFSET) & PSN_MASK;
                if let Some(psn) = qp.ack_range(base_psn, now_bitmap, ack_msn) {
                    let msns_acked = qp.ack_message(psn);
                    let require_ack = msns_acked.iter().any(|&(_, x)| x);
                    if require_ack {
                        //let now_psn = base_psn.wrapping_sub(128 - BASE_PSN_OFFSET) & PSN_MASK;
                        let ack_frame = AckFrameBuilder::build_ack(psn_now, now_bitmap, qp.dqpn());
                        if let Err(e) = self.raw_frame_tx.send(&ack_frame) {
                            tracing::error!("failed to send ack frame");
                        }
                    }
                    let last_msn_acked = msns_acked.last().map(|&(m, _)| m);
                    let cq_handle = qp.send_cq_handle();
                    //let cq_handle = if is_send_by_local_hw {
                    //    qp.send_cq_handle()
                    //} else {
                    //    qp.recv_cq_handle()
                    //};
                    if let Some(cq) = cq_handle.and_then(|h| self.cq_table.get(h)) {
                        if let Some(last_msn_acked) = last_msn_acked {
                            cq.ack_event(last_msn_acked, qp.qpn(), false);
                        }
                    }
                }
            }
            ReportMeta::Nak(NakMeta {
                qpn,
                msn,
                psn_now,
                now_bitmap,
                pre_bitmap,
                psn_before_slide,
                is_send_by_local_hw,
                is_send_by_driver,
            }) => todo!(),
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

#[allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::big_endian_bytes
)]
impl AckFrameBuilder {
    fn build_ack(now_psn: u32, now_bitmap: u128, dqpn: u32) -> Vec<u8> {
        const TRANS_TYPE_RC: u8 = 0x00;
        const OPCODE_ACKNOWLEDGE: u8 = 0x11;
        const PAYLOAD_SIZE: usize = 48;
        let mac = MacAddr::new(0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x0A);
        let mut payload = [0u8; PAYLOAD_SIZE];

        let mut bth = Bth::default();
        bth.set_opcode(u5::from_u8(OPCODE_ACKNOWLEDGE));
        bth.set_psn(u24::from_u32(now_psn));
        bth.set_dqpn(u24::from_u32(dqpn));
        bth.set_trans_type(u3::from_u8(TRANS_TYPE_RC));
        payload[..12].copy_from_slice(&bth.value.to_be_bytes());

        let mut aeth_seg0 = AethSeg0::default();
        aeth_seg0.set_is_send_by_driver(true);
        payload[12..28].copy_from_slice(&0u128.to_be_bytes()); // prev_bitmap
        payload[28..44].copy_from_slice(&now_bitmap.to_be_bytes());
        payload[44..].copy_from_slice(&aeth_seg0.value.to_be_bytes());

        Self::build_ethernet_frame(mac, mac, &payload)
    }

    fn build_ethernet_frame(src_mac: MacAddr, dst_mac: MacAddr, payload: &[u8]) -> Vec<u8> {
        const CARD_IP_ADDRESS: u32 = 0x1122_330A;
        const UDP_PORT: u16 = 4791;
        const ETH_HEADER_LEN: usize = 14;
        const IP_HEADER_LEN: usize = 20;
        const UDP_HEADER_LEN: usize = 8;

        let total_len = ETH_HEADER_LEN + IP_HEADER_LEN + UDP_HEADER_LEN + payload.len();

        let mut buffer = vec![0u8; total_len];

        let mut eth_packet = MutableEthernetPacket::new(&mut buffer)
            .unwrap_or_else(|| unreachable!("Failed to create ethernet packet"));
        eth_packet.set_source(src_mac);
        eth_packet.set_destination(dst_mac);
        eth_packet.set_ethertype(EtherTypes::Ipv4);

        let mut ipv4_packet = MutableIpv4Packet::new(&mut buffer[ETH_HEADER_LEN..])
            .unwrap_or_else(|| unreachable!("Failed to create IPv4 packet"));
        ipv4_packet.set_version(4);
        ipv4_packet.set_header_length(5);
        ipv4_packet.set_dscp(0);
        ipv4_packet.set_ecn(0);
        ipv4_packet.set_total_length((IP_HEADER_LEN + UDP_HEADER_LEN + payload.len()) as u16);
        ipv4_packet.set_identification(0);
        ipv4_packet.set_flags(Ipv4Flags::DontFragment);
        ipv4_packet.set_fragment_offset(0);
        ipv4_packet.set_ttl(64);
        ipv4_packet.set_next_level_protocol(IpNextHeaderProtocols::Udp);
        ipv4_packet.set_source(Ipv4Addr::from_bits(CARD_IP_ADDRESS));
        ipv4_packet.set_destination(Ipv4Addr::from_bits(CARD_IP_ADDRESS));
        ipv4_packet.set_checksum(ipv4_packet.get_checksum());

        let mut udp_packet = MutableUdpPacket::new(&mut buffer[ETH_HEADER_LEN + IP_HEADER_LEN..])
            .unwrap_or_else(|| unreachable!("Failed to create UDP packet"));
        udp_packet.set_source(UDP_PORT);
        udp_packet.set_destination(UDP_PORT);
        udp_packet.set_length((UDP_HEADER_LEN + payload.len()) as u16);
        udp_packet.set_payload(payload);
        udp_packet.set_checksum(udp_packet.get_checksum());

        buffer
    }
}
