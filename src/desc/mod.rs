use bilge::prelude::*;

/// Size of a descriptor in bytes.
pub(crate) const DESC_SIZE: usize = 32;

// NOTE: The `#[bitsize]` macro errors shown by rust-analyzer is a false-positive

/// A trait for converting a 32-byte array into a descriptor type.
pub(crate) trait DescFromBytes {
    /// Creates a new descriptor from raw bytes.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A 32-byte array containing the raw descriptor data
    ///
    /// # Safety
    ///
    /// This function uses transmute to convert raw bytes into a descriptor.
    /// The caller must ensure the bytes represent a valid descriptor layout.
    fn from_bytes(bytes: [u8; DESC_SIZE]) -> Self;
}

/// Implements the `DescFromBytes` trait for the specified types.
macro_rules! impl_from_bytes {
    ($($t:ty),*) => {
        $(
            impl DescFromBytes for $t {
                #[allow(unsafe_code)]
                fn from_bytes(bytes: [u8; DESC_SIZE]) -> Self {
                    unsafe { std::mem::transmute(bytes) }
                }
            }
        )*
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueDescBthRethChunk0 {
    pub expected_psn: u24,
    pub req_status: u8,

    // BTH
    pub trans: u3,
    pub opcode: u5,
    pub dqpn: u24,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueDescBthRethChunk1 {
    pub psn: u24,
    pub solicited: bool,
    pub ack_req: bool,
    pub pad_cnt: u2,
    pub reserved1: u4,

    // RETH
    pub rkey: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueDescBthRethChunk2 {
    pub va: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct MetaReportQueueDescBthRethChunk3 {
    pub dlen: u32,

    pub msn: u24,
    reserved: u7,
    pub can_auto_ack: bool,
}

/// RDMA Normal Packet Header Descriptor
#[allow(clippy::missing_docs_in_private_items)]
pub(crate) struct MetaReportQueueDescBthReth {
    c0: MetaReportQueueDescBthRethChunk0,
    c1: MetaReportQueueDescBthRethChunk1,
    c2: MetaReportQueueDescBthRethChunk2,
    c3: MetaReportQueueDescBthRethChunk3,
}

#[allow(missing_docs, clippy::missing_docs_in_private_items, unused)] // method delegations
impl MetaReportQueueDescBthReth {
    pub(crate) fn expected_psn(&self) -> u32 {
        self.c0.expected_psn().into()
    }
    pub(crate) fn set_expected_psn(&mut self, val: u32) {
        self.c0.set_expected_psn(u24::masked_new(val));
    }
    pub(crate) fn req_status(&self) -> u8 {
        self.c0.req_status()
    }
    pub(crate) fn set_req_status(&mut self, val: u8) {
        self.c0.set_req_status(val);
    }
    pub(crate) fn trans(&self) -> u8 {
        self.c0.trans().into()
    }
    pub(crate) fn set_trans(&mut self, val: u8) {
        self.c0.set_trans(u3::masked_new(val));
    }
    pub(crate) fn opcode(&self) -> u8 {
        self.c0.opcode().into()
    }
    pub(crate) fn set_opcode(&mut self, val: u8) {
        self.c0.set_opcode(u5::masked_new(val));
    }
    pub(crate) fn dqpn(&self) -> u32 {
        self.c0.dqpn().into()
    }
    pub(crate) fn set_dqpn(&mut self, val: u32) {
        self.c0.set_dqpn(u24::masked_new(val));
    }
    pub(crate) fn psn(&self) -> u32 {
        self.c1.psn().into()
    }
    pub(crate) fn set_psn(&mut self, val: u32) {
        self.c1.set_psn(u24::masked_new(val));
    }
    pub(crate) fn solicited(&self) -> bool {
        self.c1.solicited()
    }
    pub(crate) fn set_solicited(&mut self, val: bool) {
        self.c1.set_solicited(val);
    }
    pub(crate) fn ack_req(&self) -> bool {
        self.c1.ack_req()
    }
    pub(crate) fn set_ack_req(&mut self, val: bool) {
        self.c1.set_ack_req(val);
    }
    pub(crate) fn pad_cnt(&self) -> u8 {
        self.c1.pad_cnt().into()
    }
    pub(crate) fn set_pad_cnt(&mut self, val: u8) {
        self.c1.set_pad_cnt(u2::masked_new(val));
    }
    pub(crate) fn rkey(&self) -> u32 {
        self.c1.rkey()
    }
    pub(crate) fn set_rkey(&mut self, val: u32) {
        self.c1.set_rkey(val);
    }
    pub(crate) fn va(&self) -> u64 {
        self.c2.va()
    }
    pub(crate) fn set_va(&mut self, val: u64) {
        self.c2.set_va(val);
    }
    pub(crate) fn dlen(&self) -> u32 {
        self.c3.dlen()
    }
    pub(crate) fn set_dlen(&mut self, val: u32) {
        self.c3.set_dlen(val);
    }
    pub(crate) fn msn(&self) -> u32 {
        self.c3.msn().into()
    }
    pub(crate) fn set_msn(&mut self, val: u32) {
        self.c3.set_msn(u24::masked_new(val));
    }
    pub(crate) fn can_auto_ack(&self) -> bool {
        self.c3.can_auto_ack()
    }
    pub(crate) fn set_can_auto_ack(&mut self, val: bool) {
        self.c3.set_can_auto_ack(val);
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk0 {
    pub reserved1: u48,
    pub pkey: u16,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk1 {
    pub dqp_ip: u32,
    pub rkey: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk2 {
    pub raddr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SendQueueReqDescSeg0Chunk3 {
    pub common_header: u64,
}

#[allow(clippy::missing_docs_in_private_items)]
pub(crate) struct SendQueueReqDescSeg0 {
    c0: SendQueueReqDescSeg0Chunk0,
    c1: SendQueueReqDescSeg0Chunk1,
    c2: SendQueueReqDescSeg0Chunk2,
    c3: SendQueueReqDescSeg0Chunk3,
}

#[allow(missing_docs, clippy::missing_docs_in_private_items)] // method delegations
impl SendQueueReqDescSeg0 {
    pub(crate) fn reserved1(&self) -> u64 {
        self.c0.reserved1().into()
    }
    pub(crate) fn set_reserved1(&mut self, val: u64) {
        self.c0.set_reserved1(val.into());
    }

    pub(crate) fn pkey(&self) -> u16 {
        self.c0.pkey()
    }
    pub(crate) fn set_pkey(&mut self, val: u16) {
        self.c0.set_pkey(val);
    }

    pub(crate) fn dqp_ip(&self) -> u32 {
        self.c1.dqp_ip()
    }
    pub(crate) fn set_dqp_ip(&mut self, val: u32) {
        self.c1.set_dqp_ip(val);
    }

    pub(crate) fn rkey(&self) -> u32 {
        self.c1.rkey()
    }
    pub(crate) fn set_rkey(&mut self, val: u32) {
        self.c1.set_rkey(val);
    }

    pub(crate) fn raddr(&self) -> u64 {
        self.c2.raddr()
    }
    pub(crate) fn set_raddr(&mut self, val: u64) {
        self.c2.set_raddr(val);
    }

    pub(crate) fn common_header(&self) -> u64 {
        self.c3.common_header()
    }
    pub(crate) fn set_common_header(&mut self, val: u64) {
        self.c3.set_common_header(val);
    }
}

impl_from_bytes!(MetaReportQueueDescBthReth, SendQueueReqDescSeg0);
