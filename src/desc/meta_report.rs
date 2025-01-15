#![allow(clippy::missing_docs_in_private_items)]

use bilge::prelude::*;

use crate::desc::RingBufDescCommonHead;

use super::RingBufDescUntyped;

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueuePacketBasicInfoDescChunk0 {
    pub common_header: RingBufDescCommonHead,
    pub msn: u16,
    pub psn: u24,
    pub ecn_marked: bool,
    pub solicited: bool,
    pub ack_req: bool,
    pub is_retry: bool,
    reserved0: u4,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueuePacketBasicInfoDescChunk1 {
    pub dqpn: u24,
    reserved1: u8,
    pub total_len: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueuePacketBasicInfoDescChunk2 {
    pub raddr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueuePacketBasicInfoDescChunk3 {
    pub rkey: u32,
    pub imm_data: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MetaReportQueuePacketBasicInfoDesc {
    c0: MetaReportQueuePacketBasicInfoDescChunk0,
    c1: MetaReportQueuePacketBasicInfoDescChunk1,
    c2: MetaReportQueuePacketBasicInfoDescChunk2,
    c3: MetaReportQueuePacketBasicInfoDescChunk3,
}

impl MetaReportQueuePacketBasicInfoDesc {
    pub(crate) fn msn(&self) -> u16 {
        self.c0.msn()
    }

    pub(crate) fn set_msn(&mut self, val: u16) {
        self.c0.set_msn(val);
    }

    pub(crate) fn psn(&self) -> u32 {
        self.c0.psn().into()
    }

    pub(crate) fn set_psn(&mut self, val: u32) {
        self.c0.set_psn(u24::masked_new(val));
    }

    pub(crate) fn ecn_marked(&self) -> bool {
        self.c0.ecn_marked()
    }

    pub(crate) fn set_ecn_marked(&mut self, val: bool) {
        self.c0.set_ecn_marked(val);
    }

    pub(crate) fn solicited(&self) -> bool {
        self.c0.solicited()
    }

    pub(crate) fn set_solicited(&mut self, val: bool) {
        self.c0.set_solicited(val);
    }

    pub(crate) fn ack_req(&self) -> bool {
        self.c0.ack_req()
    }

    pub(crate) fn set_ack_req(&mut self, val: bool) {
        self.c0.set_ack_req(val);
    }

    pub(crate) fn is_retry(&self) -> bool {
        self.c0.is_retry()
    }

    pub(crate) fn set_is_retry(&mut self, val: bool) {
        self.c0.set_is_retry(val);
    }

    pub(crate) fn dqpn(&self) -> u32 {
        self.c1.dqpn().into()
    }

    pub(crate) fn set_dqpn(&mut self, val: u32) {
        self.c1.set_dqpn(u24::masked_new(val));
    }

    pub(crate) fn total_len(&self) -> u32 {
        self.c1.total_len()
    }

    pub(crate) fn set_total_len(&mut self, val: u32) {
        self.c1.set_total_len(val);
    }

    pub(crate) fn raddr(&self) -> u64 {
        self.c2.raddr()
    }

    pub(crate) fn set_raddr(&mut self, val: u64) {
        self.c2.set_raddr(val);
    }

    pub(crate) fn rkey(&self) -> u32 {
        self.c3.rkey()
    }

    pub(crate) fn set_rkey(&mut self, val: u32) {
        self.c3.set_rkey(val);
    }

    pub(crate) fn imm_data(&self) -> u32 {
        self.c3.imm_data()
    }

    pub(crate) fn set_imm_data(&mut self, val: u32) {
        self.c3.set_imm_data(val);
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueReadReqExtendInfoDescChunk0 {
    pub common_header: RingBufDescCommonHead,
    reserved0: u16,
    pub total_len: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueReadReqExtendInfoDescChunk1 {
    pub laddr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueReadReqExtendInfoDescChunk2 {
    pub lkey: u32,
    reserved1: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueReadReqExtendInfoDescChunk3 {
    reserved2: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MetaReportQueueReadReqExtendInfoDesc {
    c0: MetaReportQueueReadReqExtendInfoDescChunk0,
    c1: MetaReportQueueReadReqExtendInfoDescChunk1,
    c2: MetaReportQueueReadReqExtendInfoDescChunk2,
    c3: MetaReportQueueReadReqExtendInfoDescChunk3,
}

impl MetaReportQueueReadReqExtendInfoDesc {
    pub(crate) fn total_len(&self) -> u32 {
        self.c0.total_len()
    }

    pub(crate) fn set_total_len(&mut self, val: u32) {
        self.c0.set_total_len(val);
    }

    pub(crate) fn laddr(&self) -> u64 {
        self.c1.laddr()
    }

    pub(crate) fn set_laddr(&mut self, val: u64) {
        self.c1.set_laddr(val);
    }

    pub(crate) fn lkey(&self) -> u32 {
        self.c2.lkey()
    }

    pub(crate) fn set_lkey(&mut self, val: u32) {
        self.c2.set_lkey(val);
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueAckDescChunk0 {
    pub common_header: RingBufDescCommonHead,
    reserved0: u4,
    pub is_send_by_local_hw: bool,
    pub is_send_by_driver: bool,
    pub is_window_slided: bool,
    pub is_packet_lost: bool,
    reserved1: u8,
    pub psn_before_slide: u24,
    reserved2: u8,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueAckDescChunk1 {
    pub psn_now: u24,
    pub qpn: u24,
    pub msn: u16,
}

#[bitsize(128)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueAckDescChunk2 {
    pub now_bitmap: u128,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MetaReportQueueAckDesc {
    c0: MetaReportQueueAckDescChunk0,
    c1: MetaReportQueueAckDescChunk1,
    c2: MetaReportQueueAckDescChunk2,
}

impl MetaReportQueueAckDesc {
    pub(crate) fn is_send_by_local_hw(&self) -> bool {
        self.c0.is_send_by_local_hw()
    }

    pub(crate) fn set_is_send_by_local_hw(&mut self, val: bool) {
        self.c0.set_is_send_by_local_hw(val);
    }

    pub(crate) fn is_send_by_driver(&self) -> bool {
        self.c0.is_send_by_driver()
    }

    pub(crate) fn set_is_send_by_driver(&mut self, val: bool) {
        self.c0.set_is_send_by_driver(val);
    }

    pub(crate) fn is_window_slided(&self) -> bool {
        self.c0.is_window_slided()
    }

    pub(crate) fn set_is_window_slided(&mut self, val: bool) {
        self.c0.set_is_window_slided(val);
    }

    pub(crate) fn is_packet_lost(&self) -> bool {
        self.c0.is_packet_lost()
    }

    pub(crate) fn set_is_packet_lost(&mut self, val: bool) {
        self.c0.set_is_packet_lost(val);
    }

    pub(crate) fn psn_before_slide(&self) -> u32 {
        self.c0.psn_before_slide().into()
    }

    pub(crate) fn set_psn_before_slide(&mut self, val: u32) {
        self.c0.set_psn_before_slide(u24::masked_new(val));
    }

    pub(crate) fn psn_now(&self) -> u32 {
        self.c1.psn_now().into()
    }

    pub(crate) fn set_psn_now(&mut self, val: u32) {
        self.c1.set_psn_now(u24::masked_new(val));
    }

    pub(crate) fn qpn(&self) -> u32 {
        self.c1.qpn().into()
    }

    pub(crate) fn set_qpn(&mut self, val: u32) {
        self.c1.set_qpn(u24::masked_new(val));
    }

    pub(crate) fn msn(&self) -> u16 {
        self.c1.msn()
    }

    pub(crate) fn set_msn(&mut self, val: u16) {
        self.c1.set_msn(val);
    }

    pub(crate) fn now_bitmap(&self) -> u128 {
        self.c2.now_bitmap()
    }

    pub(crate) fn set_now_bitmap(&mut self, val: u128) {
        self.c2.set_now_bitmap(val);
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueAckExtraDescChunk0 {
    pub common_header: RingBufDescCommonHead,
    reserved0: u16,
    reserved1: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueAckExtraDescChunk1 {
    reserved2: u64,
}

#[bitsize(128)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueAckExtraDescChunk2 {
    pub pre_bitmap: u128,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MetaReportQueueAckExtraDesc {
    c0: MetaReportQueueAckExtraDescChunk0,
    c1: MetaReportQueueAckExtraDescChunk1,
    c2: MetaReportQueueAckExtraDescChunk2,
}

impl MetaReportQueueAckExtraDesc {
    pub(crate) fn pre_bitmap(&self) -> u128 {
        self.c2.pre_bitmap()
    }

    pub(crate) fn set_pre_bitmap(&mut self, val: u128) {
        self.c2.set_pre_bitmap(val);
    }
}
