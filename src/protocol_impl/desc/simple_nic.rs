use bilge::prelude::*;

use super::RingBufDescCommonHead;

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicTxQueueDescChunk0 {
    reserved3: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicTxQueueDescChunk1 {
    reserved2: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicTxQueueDescChunk2 {
    addr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicTxQueueDescChunk3 {
    len: u32,
    reserved0: u16,
    common_header: RingBufDescCommonHead,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SimpleNicTxQueueDesc {
    c0: SimpleNicTxQueueDescChunk0,
    c1: SimpleNicTxQueueDescChunk1,
    c2: SimpleNicTxQueueDescChunk2,
    c3: SimpleNicTxQueueDescChunk3,
}

impl SimpleNicTxQueueDesc {
    pub(crate) fn new(addr: u64, len: u32) -> Self {
        let common_header = RingBufDescCommonHead::new_simple_nic_desc();
        let c3 = SimpleNicTxQueueDescChunk3::new(len, 0, common_header);
        let c2 = SimpleNicTxQueueDescChunk2::new(addr);
        let c1 = SimpleNicTxQueueDescChunk1::new(0);
        let c0 = SimpleNicTxQueueDescChunk0::new(0);
        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn addr(&self) -> u64 {
        self.c2.addr()
    }

    pub(crate) fn set_addr(&mut self, val: u64) {
        self.c2.set_addr(val);
    }

    pub(crate) fn len(&self) -> u32 {
        self.c3.len()
    }

    pub(crate) fn set_len(&mut self, val: u32) {
        self.c3.set_len(val);
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicRxQueueDescChunk0 {
    reserved3: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicRxQueueDescChunk1 {
    reserved2: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicRxQueueDescChunk2 {
    reserved1: u32,
    slot_idx: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct SimpleNicRxQueueDescChunk3 {
    len: u32,
    reserved0: u16,
    common_header: RingBufDescCommonHead,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct SimpleNicRxQueueDesc {
    c0: SimpleNicRxQueueDescChunk0,
    c1: SimpleNicRxQueueDescChunk1,
    c2: SimpleNicRxQueueDescChunk2,
    c3: SimpleNicRxQueueDescChunk3,
}

impl SimpleNicRxQueueDesc {
    pub(crate) fn slot_idx(&self) -> u32 {
        self.c2.slot_idx()
    }

    pub(crate) fn set_slot_idx(&mut self, val: u32) {
        self.c2.set_slot_idx(val);
    }

    pub(crate) fn len(&self) -> u32 {
        self.c3.len()
    }

    pub(crate) fn set_len(&mut self, val: u32) {
        self.c3.set_len(val);
    }
}
