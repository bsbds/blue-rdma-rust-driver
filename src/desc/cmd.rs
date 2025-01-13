#![allow(clippy::missing_docs_in_private_items)]

use bilge::prelude::*;

use crate::desc::RingBufDescCommonHead;

use super::RingBufDescUntyped;

#[bitsize(48)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct RingbufDescCmdQueueCommonHead {
    pub user_data: u16,
    pub is_success: bool,
    reserved1: u31,
}

impl RingbufDescCmdQueueCommonHead {
    fn new_with_user_data(user_data: u16) -> Self {
        let mut this: Self = u48::from_u64(0).into();
        this.set_user_data(user_data);
        this
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct CmdQueueReqDescHeaderChunk {
    pub common_header: RingBufDescCommonHead,
    pub cmd_queue_common_header: RingbufDescCmdQueueCommonHead,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescUpdateMrTableChunk1 {
    pub mr_base_va: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescUpdateMrTableChunk2 {
    pub mr_length: u32,
    pub mr_key: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescUpdateMrTableChunk3 {
    pub pd_handler: u32,
    pub acc_flags: u8,
    pub pgt_offset: u17,
    reserved1: u7,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescUpdateMrTable {
    c0: CmdQueueReqDescHeaderChunk,
    c1: CmdQueueReqDescUpdateMrTableChunk1,
    c2: CmdQueueReqDescUpdateMrTableChunk2,
    c3: CmdQueueReqDescUpdateMrTableChunk3,
}

impl CmdQueueReqDescUpdateMrTable {
    pub(crate) fn new(
        user_data: u16,
        mr_base_va: u64,
        mr_length: u32,
        mr_key: u32,
        pd_handler: u32,
        acc_flags: u8,
        pgt_offset: u32,
    ) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_queue_resp_desc_update_mr_table();
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let c0 = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        let c1 = CmdQueueReqDescUpdateMrTableChunk1::new(mr_base_va);
        let c2 = CmdQueueReqDescUpdateMrTableChunk2::new(mr_length, mr_key);
        let c3 = CmdQueueReqDescUpdateMrTableChunk3::new(
            pd_handler,
            acc_flags,
            u17::from_u32(pgt_offset),
            u7::from_u8(0),
        );

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn headers(&self) -> CmdQueueReqDescHeaderChunk {
        self.c0
    }
    pub(crate) fn set_headers(&mut self, headers: CmdQueueReqDescHeaderChunk) {
        self.c0 = headers;
    }
    pub(crate) fn mr_base_va(&self) -> u64 {
        self.c1.mr_base_va()
    }
    pub(crate) fn set_mr_base_va(&mut self, val: u64) {
        self.c1.set_mr_base_va(val);
    }
    pub(crate) fn mr_length(&self) -> u32 {
        self.c2.mr_length()
    }
    pub(crate) fn set_mr_length(&mut self, val: u32) {
        self.c2.set_mr_length(val);
    }
    pub(crate) fn mr_key(&self) -> u32 {
        self.c2.mr_key()
    }
    pub(crate) fn set_mr_key(&mut self, val: u32) {
        self.c2.set_mr_key(val);
    }
    pub(crate) fn pd_handler(&self) -> u32 {
        self.c3.pd_handler()
    }
    pub(crate) fn set_pd_handler(&mut self, val: u32) {
        self.c3.set_pd_handler(val);
    }
    pub(crate) fn acc_flags(&self) -> u8 {
        self.c3.acc_flags()
    }
    pub(crate) fn set_acc_flags(&mut self, val: u8) {
        self.c3.set_acc_flags(val);
    }
    pub(crate) fn pgt_offset(&self) -> u32 {
        self.c3.pgt_offset().into()
    }
    pub(crate) fn set_pgt_offset(&mut self, val: u32) {
        self.c3.set_pgt_offset(u17::masked_new(val));
    }
}

#[allow(unsafe_code)]
impl From<CmdQueueReqDescUpdateMrTable> for RingBufDescUntyped {
    fn from(desc: CmdQueueReqDescUpdateMrTable) -> Self {
        unsafe { std::mem::transmute(desc) }
    }
}

#[allow(unsafe_code)]
impl From<RingBufDescUntyped> for CmdQueueReqDescUpdateMrTable {
    fn from(desc: RingBufDescUntyped) -> Self {
        unsafe { std::mem::transmute(desc) }
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct CmdQueueReqDescUpdatePGTChunk1 {
    dma_addr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct CmdQueueReqDescUpdatePGTChunk2 {
    start_index: u32,
    zero_based_entry_count: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct CmdQueueReqDescUpdatePGTChunk3 {
    reserved0: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescUpdatePGT {
    c0: CmdQueueReqDescHeaderChunk,
    c1: CmdQueueReqDescUpdatePGTChunk1,
    c2: CmdQueueReqDescUpdatePGTChunk2,
    c3: CmdQueueReqDescUpdatePGTChunk3,
}

impl CmdQueueReqDescUpdatePGT {
    pub(crate) fn new(
        user_data: u16,
        dma_addr: u64,
        start_index: u32,
        zero_based_entry_count: u32,
    ) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_queue_resp_desc_update_mr_table();
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let c0 = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        let c1 = CmdQueueReqDescUpdatePGTChunk1::new(dma_addr);
        let c2 = CmdQueueReqDescUpdatePGTChunk2::new(start_index, zero_based_entry_count);
        let c3 = CmdQueueReqDescUpdatePGTChunk3::new(0);

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn headers(&self) -> CmdQueueReqDescHeaderChunk {
        self.c0
    }
    pub(crate) fn set_headers(&mut self, headers: CmdQueueReqDescHeaderChunk) {
        self.c0 = headers;
    }
    pub(crate) fn dma_addr(&self) -> u64 {
        self.c1.dma_addr()
    }
    pub(crate) fn set_dma_addr(&mut self, val: u64) {
        self.c1.set_dma_addr(val);
    }
    pub(crate) fn start_index(&self) -> u32 {
        self.c2.start_index()
    }
    pub(crate) fn set_start_index(&mut self, val: u32) {
        self.c2.set_start_index(val);
    }
    pub(crate) fn zero_based_entry_count(&self) -> u32 {
        self.c2.zero_based_entry_count()
    }
    pub(crate) fn set_zero_based_entry_count(&mut self, val: u32) {
        self.c2.set_zero_based_entry_count(val);
    }
}

#[allow(unsafe_code)]
impl From<CmdQueueReqDescUpdatePGT> for RingBufDescUntyped {
    fn from(desc: CmdQueueReqDescUpdatePGT) -> Self {
        unsafe { std::mem::transmute(desc) }
    }
}

#[allow(unsafe_code)]
impl From<RingBufDescUntyped> for CmdQueueReqDescUpdatePGT {
    fn from(desc: RingBufDescUntyped) -> Self {
        unsafe { std::mem::transmute(desc) }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueRespDescOnlyCommonHeader {
    header: CmdQueueReqDescHeaderChunk,
    rest: [u64; 3],
}

impl CmdQueueRespDescOnlyCommonHeader {
    /// Creates a new `CmdQueueReqDescUpdateMrTable` response
    pub(crate) fn new_cmd_queue_resp_desc_update_mr_table(user_data: u16) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_queue_resp_desc_update_mr_table();
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let header = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        Self {
            header,
            rest: [0; 3],
        }
    }

    /// Creates a new `CmdQueueReqDescUpdatePGT` response
    pub(crate) fn new_cmd_queue_resp_desc_update_pgt(user_data: u16) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_queue_resp_desc_update_pgt();
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let header = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        Self {
            header,
            rest: [0; 3],
        }
    }

    pub(crate) fn headers(&self) -> CmdQueueReqDescHeaderChunk {
        self.header
    }
}

#[allow(unsafe_code)]
impl From<RingBufDescUntyped> for CmdQueueRespDescOnlyCommonHeader {
    fn from(desc: RingBufDescUntyped) -> Self {
        unsafe { std::mem::transmute(desc) }
    }
}

#[allow(unsafe_code)]
impl From<CmdQueueRespDescOnlyCommonHeader> for RingBufDescUntyped {
    fn from(desc: CmdQueueRespDescOnlyCommonHeader) -> Self {
        unsafe { std::mem::transmute(desc) }
    }
}
