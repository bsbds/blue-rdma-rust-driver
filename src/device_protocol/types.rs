#![allow(clippy::struct_excessive_bools)]

// TODO: add field validations
use std::marker::PhantomData;

use crate::{mem::page::ContiguousPages, qp::convert_ibv_mtu_to_u16};

#[allow(clippy::missing_docs_in_private_items)]
/// Memory Translation Table entry
pub(crate) struct MttEntry<'a> {
    /// Reference to the mtt entry buffer, shouldn't be dropped during operations
    pub(crate) entry_buffer: &'a ContiguousPages<1>,
    pub(crate) mr_base_va: u64,
    pub(crate) mr_length: u32,
    pub(crate) mr_key: u32,
    pub(crate) pd_handler: u32,
    pub(crate) acc_flags: u8,
    pub(crate) pgt_offset: u32,
    pub(crate) dma_addr: u64,
    pub(crate) zero_based_entry_count: u32,
}

impl<'a> MttEntry<'a> {
    #[allow(clippy::too_many_arguments)]
    /// Creates a new `MttEntry`
    pub(crate) fn new(
        entry_buffer: &'a ContiguousPages<1>,
        mr_base_va: u64,
        mr_length: u32,
        mr_key: u32,
        pd_handler: u32,
        acc_flags: u8,
        pgt_offset: u32,
        dma_addr: u64,
        zero_based_entry_count: u32,
    ) -> Self {
        Self {
            entry_buffer,
            mr_base_va,
            mr_length,
            mr_key,
            pd_handler,
            acc_flags,
            pgt_offset,
            dma_addr,
            zero_based_entry_count,
        }
    }
}
/// Queue Pair entry
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Default)]
pub(crate) struct UpdateQp {
    pub(crate) ip_addr: u32,
    pub(crate) qpn: u32,
    pub(crate) peer_qpn: u32,
    pub(crate) rq_access_flags: u8,
    pub(crate) qp_type: u8,
    pub(crate) pmtu: u8,
    pub(crate) local_udp_port: u16,
    pub(crate) peer_mac_addr: u64,
}

/// Receive buffer
pub(crate) struct RecvBuffer {
    /// One page
    inner: ContiguousPages<1>,
}

/// Metadata about a receive buffer
pub(crate) struct RecvBufferMeta {
    /// Physical address of the receive buffer
    pub(crate) phys_addr: u64,
}

impl RecvBufferMeta {
    /// Creates a new `RecvBufferMeta`
    pub(crate) fn new(phys_addr: u64) -> Self {
        Self { phys_addr }
    }
}

impl RecvBuffer {
    /// Creates a new receive buffer from contiguous pages
    pub(crate) fn new(inner: ContiguousPages<1>) -> Self {
        Self { inner }
    }

    /// Gets start address about this receive buffer
    pub(crate) fn addr(&self) -> u64 {
        self.inner.addr()
    }
}

impl AsMut<[u8]> for RecvBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.inner
    }
}

impl AsRef<[u8]> for RecvBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}

/// The position of a packet
#[derive(Debug, Clone, Copy)]
pub(crate) enum PacketPos {
    /// First packet
    First,
    /// Middle packet
    Middle,
    /// Last packet
    Last,
    /// Only packet
    Only,
}

#[allow(clippy::missing_docs_in_private_items)]
/// Metadata from meta report queue
#[derive(Debug, Clone, Copy)]
pub(crate) enum ReportMeta {
    /// Write operation header
    Write(HeaderWriteMeta),
    /// Read operation header
    Read(HeaderReadMeta),
    /// Congestion Notification Packet
    Cnp(CnpMeta),
    /// Positive acknowledgment
    Ack(AckMeta),
    /// Negative acknowledgment
    Nak(NakMeta),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeaderWriteMeta {
    pub(crate) pos: PacketPos,
    pub(crate) msn: u16,
    pub(crate) psn: u32,
    pub(crate) solicited: bool,
    pub(crate) ack_req: bool,
    pub(crate) is_retry: bool,
    pub(crate) dqpn: u32,
    pub(crate) total_len: u32,
    pub(crate) raddr: u64,
    pub(crate) rkey: u32,
    pub(crate) imm: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeaderReadMeta {
    pub(crate) raddr: u64,
    pub(crate) rkey: u32,
    pub(crate) total_len: u32,
    pub(crate) laddr: u64,
    pub(crate) lkey: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CnpMeta {
    /// The initiator's QP number
    pub(crate) qpn: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AckMeta {
    pub(crate) qpn: u32,
    pub(crate) msn: u16,
    pub(crate) psn_now: u32,
    pub(crate) now_bitmap: u128,
    pub(crate) is_window_slided: bool,
    pub(crate) is_send_by_local_hw: bool,
    pub(crate) is_send_by_driver: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NakMeta {
    pub(crate) qpn: u32,
    pub(crate) msn: u16,
    pub(crate) psn_now: u32,
    pub(crate) now_bitmap: u128,
    pub(crate) pre_bitmap: u128,
    pub(crate) psn_before_slide: u32,
    pub(crate) is_send_by_local_hw: bool,
    pub(crate) is_send_by_driver: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Initial;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WithQpParams;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WithIbvParams;
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WithChunkInfo;

/// Work Request Builder
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WrChunkBuilder<S> {
    inner: WrChunk,
    _state: PhantomData<S>,
}

impl WrChunkBuilder<Initial> {
    pub(crate) fn new() -> Self {
        Self {
            inner: WrChunk::default(),
            _state: PhantomData,
        }
    }

    pub(crate) fn new_read_resp() -> Self {
        let mut inner = WrChunk {
            opcode: WorkReqOpCode::RdmaReadResp,
            ..Default::default()
        };
        Self {
            inner,
            _state: PhantomData,
        }
    }

    #[allow(clippy::unused_self, clippy::too_many_arguments)]
    pub(crate) fn set_qp_params(self, qp_params: QpParams) -> WrChunkBuilder<WithQpParams> {
        WrChunkBuilder {
            inner: WrChunk {
                qp_type: qp_params.qp_type,
                sqpn: qp_params.sqpn,
                mac_addr: qp_params.mac_addr,
                dqpn: qp_params.dqpn,
                dqp_ip: qp_params.dqp_ip,
                pmtu: qp_params.pmtu,
                msn: qp_params.msn,
                ..Default::default()
            },
            _state: PhantomData,
        }
    }
}

impl WrChunkBuilder<WithQpParams> {
    pub(crate) fn set_ibv_params(
        mut self,
        flags: u8,
        rkey: u32,
        total_len: u32,
        lkey: u32,
        imm: u32,
    ) -> WrChunkBuilder<WithIbvParams> {
        self.inner.flags = flags;
        self.inner.rkey = rkey;
        self.inner.total_len = total_len;
        self.inner.lkey = lkey;
        self.inner.imm = imm;

        WrChunkBuilder {
            inner: self.inner,
            _state: PhantomData,
        }
    }

    pub(crate) fn pmtu(&self) -> u16 {
        convert_ibv_mtu_to_u16(self.inner.pmtu).unwrap_or_else(|| unreachable!("invalid ibv_mtu"))
    }
}

impl WrChunkBuilder<WithIbvParams> {
    pub(crate) fn set_chunk_meta(
        mut self,
        psn: u32,
        laddr: u64,
        raddr: u64,
        len: u32,
        pos: ChunkPos,
    ) -> WrChunkBuilder<WithChunkInfo> {
        self.inner.psn = psn;
        self.inner.laddr = laddr;
        self.inner.raddr = raddr;
        self.inner.len = len;
        match pos {
            ChunkPos::First => self.inner.is_first = true,
            ChunkPos::Last => self.inner.is_last = true,
            ChunkPos::Middle => {}
            ChunkPos::Only => {
                self.inner.is_first = true;
                self.inner.is_last = true;
            }
        }

        WrChunkBuilder {
            inner: self.inner,
            _state: PhantomData,
        }
    }

    pub(crate) fn pmtu(&self) -> u16 {
        convert_ibv_mtu_to_u16(self.inner.pmtu).unwrap_or_else(|| unreachable!("invalid ibv_mtu"))
    }
}

impl WrChunkBuilder<WithChunkInfo> {
    pub(crate) fn set_is_retry(mut self) -> Self {
        self.inner.is_retry = true;
        self
    }

    pub(crate) fn set_enable_ecn(mut self) -> Self {
        self.inner.enable_ecn = true;
        self
    }

    pub(crate) fn set_is_read_resp(mut self) -> Self {
        self.inner.opcode = WorkReqOpCode::RdmaReadResp;
        self
    }

    pub(crate) fn build(self) -> WrChunk {
        self.inner
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WrChunk {
    pub(crate) opcode: WorkReqOpCode,
    pub(crate) qp_type: u8,
    pub(crate) sqpn: u32,
    pub(crate) mac_addr: u64,
    pub(crate) dqpn: u32,
    pub(crate) dqp_ip: u32,
    pub(crate) pmtu: u8,
    pub(crate) flags: u8,
    pub(crate) raddr: u64,
    pub(crate) rkey: u32,
    pub(crate) total_len: u32,
    pub(crate) lkey: u32,
    pub(crate) imm: u32,
    pub(crate) laddr: u64,
    pub(crate) len: u32,
    pub(crate) is_first: bool,
    pub(crate) is_last: bool,
    pub(crate) msn: u16,
    pub(crate) psn: u32,
    pub(crate) is_retry: bool,
    pub(crate) enable_ecn: bool,
}

impl WrChunk {
    pub(crate) fn set_is_retry(&mut self) {
        self.is_retry = true;
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChunkPos {
    #[default]
    First,
    Middle,
    Last,
    Only,
}

impl ChunkPos {
    pub(crate) fn next(self) -> Self {
        match self {
            ChunkPos::First | ChunkPos::Middle => ChunkPos::Middle,
            ChunkPos::Last => ChunkPos::Last,
            ChunkPos::Only => ChunkPos::Only,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QpParams {
    pub(crate) msn: u16,
    pub(crate) qp_type: u8,
    pub(crate) sqpn: u32,
    pub(crate) mac_addr: u64,
    pub(crate) dqpn: u32,
    pub(crate) dqp_ip: u32,
    pub(crate) pmtu: u8,
}

impl QpParams {
    pub(crate) fn new(
        msn: u16,
        qp_type: u8,
        sqpn: u32,
        mac_addr: u64,
        dqpn: u32,
        dqp_ip: u32,
        pmtu: u8,
    ) -> Self {
        Self {
            msn,
            qp_type,
            sqpn,
            mac_addr,
            dqpn,
            dqp_ip,
            pmtu,
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub(crate) enum WorkReqOpCode {
    #[default]
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
