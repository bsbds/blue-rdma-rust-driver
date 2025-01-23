use bilge::prelude::*;

use crate::desc::RingBufDescCommonHead;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub(crate) enum WorkReqOpCode {
    RdmaWrite = 0,
    RdmaWriteWithImm = 1,
    Send = 2,
    SendWithImm = 3,
    RdmaRead = 4,
    AtomicCmpAndSwp = 5,
    AtomicFetchAndAdd = 6,
    LocalInv = 7,
    BindMw = 8,
    SendWithInv = 9,
    Tso = 10,
    Driver1 = 11,
    RdmaReadResp = 12,
    RdmaAck = 13,
    Flush = 14,
    AtomicWrite = 15,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk0 {
    pub common_header: RingBufDescCommonHead,
    pub msn: u16,
    pub total_len: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk1 {
    pub rkey: u32,
    pub dqp_ip: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk2 {
    pub raddr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk3 {
    pub psn: u24,
    pub qp_type: u4,
    reserved0: u4,
    pub dqpn: u24,
    pub flags: u5,
    reserved1: u3,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SendQueueReqDescSeg0 {
    c0: SendQueueReqDescSeg0Chunk0,
    c1: SendQueueReqDescSeg0Chunk1,
    c2: SendQueueReqDescSeg0Chunk2,
    c3: SendQueueReqDescSeg0Chunk3,
}

impl SendQueueReqDescSeg0 {
    pub(crate) fn new_rdma_write(
        msn: u16,
        psn: u32,
        qp_type: u8,
        dqpn: u32,
        flags: u8,
        dqp_ip: u32,
        raddr: u64,
        rkey: u32,
        total_len: u32,
    ) -> Self {
        Self::new_inner(
            WorkReqOpCode::RdmaWrite,
            msn,
            psn,
            qp_type,
            dqpn,
            flags,
            dqp_ip,
            raddr,
            rkey,
            total_len,
        )
    }

    pub(crate) fn new_inner(
        op_code: WorkReqOpCode,
        msn: u16,
        psn: u32,
        qp_type: u8,
        dqpn: u32,
        flags: u8,
        dqp_ip: u32,
        raddr: u64,
        rkey: u32,
        total_len: u32,
    ) -> Self {
        let mut common_header = RingBufDescCommonHead::new_send_desc(op_code);
        common_header.set_has_next(true);
        let c0 = SendQueueReqDescSeg0Chunk0::new(common_header, msn, total_len);
        let c1 = SendQueueReqDescSeg0Chunk1::new(rkey, dqp_ip);
        let c2 = SendQueueReqDescSeg0Chunk2::new(raddr);
        let c3 = SendQueueReqDescSeg0Chunk3::new(
            u24::masked_new(psn),
            u4::masked_new(qp_type),
            u4::from_u8(0),
            u24::masked_new(dqpn),
            u5::masked_new(flags),
            u3::from_u8(0),
        );

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn msn(&self) -> u16 {
        self.c0.msn()
    }

    pub(crate) fn set_msn(&mut self, val: u16) {
        self.c0.set_msn(val);
    }

    pub(crate) fn psn(&self) -> u32 {
        self.c3.psn().into()
    }

    pub(crate) fn set_psn(&mut self, val: u32) {
        self.c3.set_psn(u24::masked_new(val));
    }

    pub(crate) fn qp_type(&self) -> u8 {
        self.c3.qp_type().into()
    }

    pub(crate) fn set_qp_type(&mut self, val: u8) {
        self.c3.set_qp_type(u4::masked_new(val));
    }

    pub(crate) fn dqpn(&self) -> u32 {
        self.c3.dqpn().into()
    }

    pub(crate) fn set_dqpn(&mut self, val: u32) {
        self.c3.set_dqpn(u24::masked_new(val));
    }

    pub(crate) fn flags(&self) -> u8 {
        self.c3.flags().into()
    }

    pub(crate) fn set_flags(&mut self, val: u8) {
        self.c3.set_flags(u5::masked_new(val));
    }

    pub(crate) fn dqp_ip(&self) -> u32 {
        self.c1.dqp_ip()
    }

    pub(crate) fn set_dqp_ip(&mut self, val: u32) {
        self.c1.set_dqp_ip(val);
    }

    pub(crate) fn raddr(&self) -> u64 {
        self.c2.raddr()
    }

    pub(crate) fn set_raddr(&mut self, val: u64) {
        self.c2.set_raddr(val);
    }

    pub(crate) fn rkey(&self) -> u32 {
        self.c1.rkey()
    }

    pub(crate) fn set_rkey(&mut self, val: u32) {
        self.c1.set_rkey(val);
    }

    pub(crate) fn total_len(&self) -> u32 {
        self.c0.total_len()
    }

    pub(crate) fn set_total_len(&mut self, val: u32) {
        self.c0.set_total_len(val);
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg1Chunk0 {
    pub common_header: RingBufDescCommonHead,
    pub pmtu: u3,
    pub is_first: bool,
    pub is_last: bool,
    pub is_retry: bool,
    pub enable_ecn: bool,
    reserved0: u1,
    pub sqpn_low_8bits: u8,
    pub imm: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg1Chunk1 {
    pub mac_addr: u48,
    pub sqpn_high_16bits: u16,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg1Chunk2 {
    pub lkey: u32,
    pub len: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg1Chunk3 {
    pub laddr: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SendQueueReqDescSeg1 {
    c0: SendQueueReqDescSeg1Chunk0,
    c1: SendQueueReqDescSeg1Chunk1,
    c2: SendQueueReqDescSeg1Chunk2,
    c3: SendQueueReqDescSeg1Chunk3,
}

impl SendQueueReqDescSeg1 {
    pub(crate) fn new_rdma_write(
        pmtu: u8,
        is_first: bool,
        is_last: bool,
        is_retry: bool,
        enable_ecn: bool,
        sqpn: u32,
        imm: u32,
        mac_addr: u64,
        lkey: u32,
        len: u32,
        laddr: u64,
    ) -> Self {
        Self::new_inner(
            WorkReqOpCode::RdmaWrite,
            pmtu,
            is_first,
            is_last,
            is_retry,
            enable_ecn,
            sqpn,
            imm,
            mac_addr,
            lkey,
            len,
            laddr,
        )
    }

    #[allow(clippy::as_conversions, clippy::cast_possible_truncation)] // truncation is expected
                                                                       // behavior
    pub(crate) fn new_inner(
        op_code: WorkReqOpCode,
        pmtu: u8,
        is_first: bool,
        is_last: bool,
        is_retry: bool,
        enable_ecn: bool,
        sqpn: u32,
        imm: u32,
        mac_addr: u64,
        lkey: u32,
        len: u32,
        laddr: u64,
    ) -> Self {
        let common_header = RingBufDescCommonHead::new_send_desc(op_code);
        let c0 = SendQueueReqDescSeg1Chunk0::new(
            common_header,
            u3::masked_new(pmtu),
            is_first,
            is_last,
            is_retry,
            enable_ecn,
            u1::from_u8(0),
            sqpn as u8,
            imm,
        );
        let c1 = SendQueueReqDescSeg1Chunk1::new(u48::masked_new(mac_addr), (sqpn >> 8) as u16);
        let c2 = SendQueueReqDescSeg1Chunk2::new(lkey, len);
        let c3 = SendQueueReqDescSeg1Chunk3::new(laddr);

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn pmtu(&self) -> u8 {
        self.c0.pmtu().into()
    }

    pub(crate) fn set_pmtu(&mut self, val: u8) {
        self.c0.set_pmtu(u3::masked_new(val));
    }

    pub(crate) fn is_first(&self) -> bool {
        self.c0.is_first()
    }

    pub(crate) fn set_is_first(&mut self, val: bool) {
        self.c0.set_is_first(val);
    }

    pub(crate) fn is_last(&self) -> bool {
        self.c0.is_last()
    }

    pub(crate) fn set_is_last(&mut self, val: bool) {
        self.c0.set_is_last(val);
    }

    pub(crate) fn is_retry(&self) -> bool {
        self.c0.is_retry()
    }

    pub(crate) fn set_is_retry(&mut self, val: bool) {
        self.c0.set_is_retry(val);
    }

    pub(crate) fn enable_ecn(&self) -> bool {
        self.c0.enable_ecn()
    }

    pub(crate) fn set_enable_ecn(&mut self, val: bool) {
        self.c0.set_enable_ecn(val);
    }

    pub(crate) fn sqpn_low_8bits(&self) -> u8 {
        self.c0.sqpn_low_8bits()
    }

    pub(crate) fn set_sqpn_low_8bits(&mut self, val: u8) {
        self.c0.set_sqpn_low_8bits(val);
    }

    pub(crate) fn imm(&self) -> u32 {
        self.c0.imm()
    }

    pub(crate) fn set_imm(&mut self, val: u32) {
        self.c0.set_imm(val);
    }

    pub(crate) fn mac_addr(&self) -> u64 {
        self.c1.mac_addr().into()
    }

    pub(crate) fn set_mac_addr(&mut self, val: u64) {
        self.c1.set_mac_addr(u48::masked_new(val));
    }

    pub(crate) fn sqpn_high_16bits(&self) -> u16 {
        self.c1.sqpn_high_16bits()
    }

    pub(crate) fn set_sqpn_high_16bits(&mut self, val: u16) {
        self.c1.set_sqpn_high_16bits(val);
    }

    pub(crate) fn lkey(&self) -> u32 {
        self.c2.lkey()
    }

    pub(crate) fn set_lkey(&mut self, val: u32) {
        self.c2.set_lkey(val);
    }

    pub(crate) fn len(&self) -> u32 {
        self.c2.len()
    }

    pub(crate) fn set_len(&mut self, val: u32) {
        self.c2.set_len(val);
    }

    pub(crate) fn laddr(&self) -> u64 {
        self.c3.laddr()
    }

    pub(crate) fn set_laddr(&mut self, val: u64) {
        self.c3.set_laddr(val);
    }
}
