use bilge::prelude::*;

use crate::desc::RingBufDescCommonHead;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub(crate) enum CmdQueueDescOperators {
    UpdateMrTable = 0x00,
    UpdatePgt = 0x01,
    ManageQp = 0x02,
    SetNetworkParam = 0x03,
    SetRawPacketReceiveMeta = 0x04,
}

#[bitsize(16)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct RingbufDescCmdQueueCommonHead {
    pub user_data: u8,
    pub is_success: bool,
    reserved1: u7,
}

impl RingbufDescCmdQueueCommonHead {
    fn new_with_user_data(user_data: u8) -> Self {
        let mut this: Self = 0u16.into();
        this.set_user_data(user_data);
        this
    }
}

#[bitsize(32)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct CmdQueueReqDescHeaderChunk {
    pub common_header: RingBufDescCommonHead,
    pub cmd_queue_common_header: RingbufDescCmdQueueCommonHead,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescUpdateMrTableChunk0 {
    headers: CmdQueueReqDescHeaderChunk,
    reserved0: u32,
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
    pub reserved1: u32,
    pub acc_flags: u8,
    pub pgt_offset: u17,
    reserved2: u7,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescUpdateMrTable {
    c0: CmdQueueReqDescUpdateMrTableChunk0,
    c1: CmdQueueReqDescUpdateMrTableChunk1,
    c2: CmdQueueReqDescUpdateMrTableChunk2,
    c3: CmdQueueReqDescUpdateMrTableChunk3,
}

impl CmdQueueReqDescUpdateMrTable {
    pub(crate) fn new(
        user_data: u8,
        mr_base_va: u64,
        mr_length: u32,
        mr_key: u32,
        pd_handler: u32,
        acc_flags: u8,
        pgt_offset: u32,
    ) -> Self {
        let common_header =
            RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::UpdateMrTable);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let header = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        let c0 = CmdQueueReqDescUpdateMrTableChunk0::new(header, 0);
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
        self.c0.headers()
    }
    pub(crate) fn set_headers(&mut self, headers: CmdQueueReqDescHeaderChunk) {
        self.c0.set_headers(headers);
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

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
pub(crate) struct CmdQueueReqDescUpdatePGTChunk0 {
    headers: CmdQueueReqDescHeaderChunk,
    reserved0: u32,
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

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescUpdatePGT {
    c0: CmdQueueReqDescUpdatePGTChunk0,
    c1: CmdQueueReqDescUpdatePGTChunk1,
    c2: CmdQueueReqDescUpdatePGTChunk2,
    c3: CmdQueueReqDescUpdatePGTChunk3,
}

impl CmdQueueReqDescUpdatePGT {
    pub(crate) fn new(
        user_data: u8,
        dma_addr: u64,
        start_index: u32,
        zero_based_entry_count: u32,
    ) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::UpdatePgt);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let headers = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        let c0 = CmdQueueReqDescUpdatePGTChunk0::new(headers, 0);
        let c1 = CmdQueueReqDescUpdatePGTChunk1::new(dma_addr);
        let c2 = CmdQueueReqDescUpdatePGTChunk2::new(start_index, zero_based_entry_count);
        let c3 = CmdQueueReqDescUpdatePGTChunk3::new(0);

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn headers(&self) -> CmdQueueReqDescHeaderChunk {
        self.c0.headers()
    }
    pub(crate) fn set_headers(&mut self, headers: CmdQueueReqDescHeaderChunk) {
        self.c0.set_headers(headers);
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

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueRespDescOnlyCommonHeader {
    header: CmdQueueReqDescHeaderChunk,
    rest: [u8; 28],
}

impl CmdQueueRespDescOnlyCommonHeader {
    /// Creates a new `CmdQueueReqDescUpdateMrTable` response
    pub(crate) fn new_cmd_queue_resp_desc_update_mr_table(user_data: u8) -> Self {
        let common_header =
            RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::UpdateMrTable);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let header = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        Self {
            header,
            rest: [0; 28],
        }
    }

    /// Creates a new `CmdQueueReqDescUpdatePGT` response
    pub(crate) fn new_cmd_queue_resp_desc_update_pgt(user_data: u8) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::UpdatePgt);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let header = CmdQueueReqDescHeaderChunk::new(common_header, cmd_queue_common_header);
        Self {
            header,
            rest: [0; 28],
        }
    }

    pub(crate) fn headers(&self) -> CmdQueueReqDescHeaderChunk {
        self.header
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescQpManagementChunk0 {
    pub common_header: RingBufDescCommonHead,
    pub cmd_queue_common_header: RingbufDescCmdQueueCommonHead,
    pub ip_addr: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescQpManagementChunk1 {
    pub is_valid: bool,
    pub is_error: bool,
    reserved0: u6,
    pub qpn: u24,
    reserved1: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescQpManagementChunk2 {
    pub peer_qpn: u24,
    pub rq_access_flags: u8,
    pub qp_type: u4,
    reserved2: u4,
    pub pmtu: u3,
    reserved3: u5,
    reserved4: u16,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescQpManagementChunk3 {
    pub local_udp_port: u16,
    pub peer_mac_addr: u48,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescQpManagement {
    c0: CmdQueueReqDescQpManagementChunk0,
    c1: CmdQueueReqDescQpManagementChunk1,
    c2: CmdQueueReqDescQpManagementChunk2,
    c3: CmdQueueReqDescQpManagementChunk3,
}

impl CmdQueueReqDescQpManagement {
    #[allow(clippy::too_many_arguments)] // FIXME: use builder
    pub(crate) fn new(
        user_data: u8,
        ip_addr: u32,
        qpn: u32,
        is_error: bool,
        is_valid: bool,
        peer_qpn: u32,
        rq_access_flags: u8,
        qp_type: u8,
        pmtu: u8,
        local_udp_port: u16,
        peer_mac_addr: u64,
    ) -> Self {
        let common_header = RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::ManageQp);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let c0 =
            CmdQueueReqDescQpManagementChunk0::new(common_header, cmd_queue_common_header, ip_addr);
        let c1 = CmdQueueReqDescQpManagementChunk1::new(
            is_error,
            is_valid,
            u6::from_u8(0),
            u24::masked_new(qpn),
            0,
        );
        let c2 = CmdQueueReqDescQpManagementChunk2::new(
            u24::masked_new(peer_qpn),
            rq_access_flags,
            u4::masked_new(qp_type),
            u4::from_u8(0),
            u3::masked_new(pmtu),
            u5::from_u8(0),
            0,
        );
        let c3 =
            CmdQueueReqDescQpManagementChunk3::new(local_udp_port, u48::masked_new(peer_mac_addr));

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn cmd_queue_common_header(&self) -> RingbufDescCmdQueueCommonHead {
        self.c0.cmd_queue_common_header()
    }

    pub(crate) fn set_cmd_queue_common_header(&mut self, val: RingbufDescCmdQueueCommonHead) {
        self.c0.set_cmd_queue_common_header(val);
    }

    pub(crate) fn ip_addr(&self) -> u32 {
        self.c0.ip_addr()
    }

    pub(crate) fn set_ip_addr(&mut self, val: u32) {
        self.c0.set_ip_addr(val);
    }

    pub(crate) fn qpn(&self) -> u32 {
        self.c1.qpn().into()
    }

    pub(crate) fn set_qpn(&mut self, val: u32) {
        self.c1.set_qpn(u24::masked_new(val));
    }

    pub(crate) fn is_error(&self) -> bool {
        self.c1.is_error()
    }

    pub(crate) fn set_is_error(&mut self, val: bool) {
        self.c1.set_is_error(val);
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.c1.is_valid()
    }

    pub(crate) fn set_is_valid(&mut self, val: bool) {
        self.c1.set_is_valid(val);
    }

    pub(crate) fn peer_qpn(&self) -> u32 {
        self.c2.peer_qpn().into()
    }

    pub(crate) fn set_peer_qpn(&mut self, val: u32) {
        self.c2.set_peer_qpn(u24::masked_new(val));
    }

    pub(crate) fn rq_access_flags(&self) -> u8 {
        self.c2.rq_access_flags()
    }

    pub(crate) fn set_rq_access_flags(&mut self, val: u8) {
        self.c2.set_rq_access_flags(val);
    }

    pub(crate) fn qp_type(&self) -> u8 {
        self.c2.qp_type().into()
    }

    pub(crate) fn set_qp_type(&mut self, val: u8) {
        self.c2.set_qp_type(u4::masked_new(val));
    }

    pub(crate) fn pmtu(&self) -> u8 {
        self.c2.pmtu().into()
    }

    pub(crate) fn set_pmtu(&mut self, val: u8) {
        self.c2.set_pmtu(u3::masked_new(val));
    }

    pub(crate) fn local_udp_port(&self) -> u16 {
        self.c3.local_udp_port()
    }

    pub(crate) fn set_local_udp_port(&mut self, val: u16) {
        self.c3.set_local_udp_port(val);
    }

    pub(crate) fn peer_mac_addr(&self) -> u64 {
        self.c3.peer_mac_addr().into()
    }

    pub(crate) fn set_peer_mac_addr(&mut self, val: u64) {
        self.c3.set_peer_mac_addr(u48::masked_new(val));
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetNetworkParamChunk0 {
    pub common_header: RingBufDescCommonHead,
    pub cmd_queue_common_header: RingbufDescCmdQueueCommonHead,
    reserved0: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetNetworkParamChunk1 {
    pub gateway: u32,
    pub netmask: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetNetworkParamChunk2 {
    pub ip_addr: u32,
    reserved1: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetNetworkParamChunk3 {
    pub mac_addr: u48,
    reserved2: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescSetNetworkParam {
    c0: CmdQueueReqDescSetNetworkParamChunk0,
    c1: CmdQueueReqDescSetNetworkParamChunk1,
    c2: CmdQueueReqDescSetNetworkParamChunk2,
    c3: CmdQueueReqDescSetNetworkParamChunk3,
}

impl CmdQueueReqDescSetNetworkParam {
    pub(crate) fn new(
        user_data: u8,
        gateway: u32,
        netmask: u32,
        ip_addr: u32,
        mac_addr: u64,
    ) -> Self {
        let common_header =
            RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::SetNetworkParam);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let c0 =
            CmdQueueReqDescSetNetworkParamChunk0::new(common_header, cmd_queue_common_header, 0);
        let c1 = CmdQueueReqDescSetNetworkParamChunk1::new(gateway, netmask);
        let c2 = CmdQueueReqDescSetNetworkParamChunk2::new(ip_addr, 0);
        let c3 = CmdQueueReqDescSetNetworkParamChunk3::new(u48::masked_new(mac_addr), 0);

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn cmd_queue_common_header(&self) -> RingbufDescCmdQueueCommonHead {
        self.c0.cmd_queue_common_header()
    }

    pub(crate) fn set_cmd_queue_common_header(&mut self, val: RingbufDescCmdQueueCommonHead) {
        self.c0.set_cmd_queue_common_header(val);
    }

    pub(crate) fn gateway(&self) -> u32 {
        self.c1.gateway()
    }

    pub(crate) fn set_gateway(&mut self, val: u32) {
        self.c1.set_gateway(val);
    }

    pub(crate) fn netmask(&self) -> u32 {
        self.c1.netmask()
    }

    pub(crate) fn set_netmask(&mut self, val: u32) {
        self.c1.set_netmask(val);
    }

    pub(crate) fn ip_addr(&self) -> u32 {
        self.c2.ip_addr()
    }

    pub(crate) fn set_ip_addr(&mut self, val: u32) {
        self.c2.set_ip_addr(val);
    }

    pub(crate) fn mac_addr(&self) -> u64 {
        self.c3.mac_addr().into()
    }

    pub(crate) fn set_mac_addr(&mut self, val: u64) {
        self.c3.set_mac_addr(u48::masked_new(val));
    }
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetRawPacketReceiveMetaChunk0 {
    pub common_header: RingBufDescCommonHead,
    pub cmd_queue_common_header: RingbufDescCmdQueueCommonHead,
    reserved0: u32,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetRawPacketReceiveMetaChunk1 {
    pub write_base_addr: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetRawPacketReceiveMetaChunk2 {
    reserved1: u64,
}

#[bitsize(64)]
#[derive(Clone, Copy, DebugBits, FromBits)]
struct CmdQueueReqDescSetRawPacketReceiveMetaChunk3 {
    reserved2: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdQueueReqDescSetRawPacketReceiveMeta {
    c0: CmdQueueReqDescSetRawPacketReceiveMetaChunk0,
    c1: CmdQueueReqDescSetRawPacketReceiveMetaChunk1,
    c2: CmdQueueReqDescSetRawPacketReceiveMetaChunk2,
    c3: CmdQueueReqDescSetRawPacketReceiveMetaChunk3,
}

impl CmdQueueReqDescSetRawPacketReceiveMeta {
    pub(crate) fn new(user_data: u8, write_base_addr: u64) -> Self {
        let common_header =
            RingBufDescCommonHead::new_cmd_desc(CmdQueueDescOperators::SetRawPacketReceiveMeta);
        let cmd_queue_common_header = RingbufDescCmdQueueCommonHead::new_with_user_data(user_data);
        let c0 = CmdQueueReqDescSetRawPacketReceiveMetaChunk0::new(
            common_header,
            cmd_queue_common_header,
            0,
        );
        let c1 = CmdQueueReqDescSetRawPacketReceiveMetaChunk1::new(write_base_addr);
        let c2 = CmdQueueReqDescSetRawPacketReceiveMetaChunk2::new(0);
        let c3 = CmdQueueReqDescSetRawPacketReceiveMetaChunk3::new(0);

        Self { c0, c1, c2, c3 }
    }

    pub(crate) fn cmd_queue_common_header(&self) -> RingbufDescCmdQueueCommonHead {
        self.c0.cmd_queue_common_header()
    }

    pub(crate) fn set_cmd_queue_common_header(&mut self, val: RingbufDescCmdQueueCommonHead) {
        self.c0.set_cmd_queue_common_header(val);
    }

    pub(crate) fn write_base_addr(&self) -> u64 {
        self.c1.write_base_addr()
    }

    pub(crate) fn set_write_base_addr(&mut self, val: u64) {
        self.c1.set_write_base_addr(val);
    }
}
