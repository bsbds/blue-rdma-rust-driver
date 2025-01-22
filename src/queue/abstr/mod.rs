/// Work Request Builder
mod wr;

pub(crate) use wr::*;

use std::io;

use crate::{mem::page::ContiguousPages, net::config::NetworkConfig};

/// RDMA device configuration interface
pub(crate) trait DeviceCommand {
    /// Updates Memory Translation Table entry
    fn update_mtt(&self, entry: MttEntry<'_>) -> io::Result<()>;
    /// Updates Queue Pair entry
    fn update_qp(&self, entry: QpEntry) -> io::Result<()>;
    /// Sets network parameters
    fn set_network(&self, param: NetworkConfig) -> io::Result<()>;
    /// Sets receive buffer for raw packets
    fn set_raw_packet_recv_buffer(&self, buffer: RecvBufferMeta) -> io::Result<()>;
}

/// RDMA send operations interface
pub(crate) trait WorkReqSend {
    /// Sends an RDMA operation
    fn send(&self, op: WrChunk) -> io::Result<()>;
}

/// Metadata reporting interface
pub(crate) trait MetaReport: Send + 'static {
    /// Tries to receive operation metadata
    fn try_recv_meta(&mut self) -> io::Result<Option<ReportMeta>>;
}

/// Simple NIC tunnel interface
pub(crate) trait SimpleNicTunnel: Send + 'static {
    /// Frame Sender
    type Sender: FrameTx;
    /// Frame Receiver
    type Receiver: FrameRx;

    /// Splits into send half and recv half
    fn into_split(self) -> (Self::Sender, Self::Receiver);

    fn recv_buffer_virt_addr(&self) -> u64;
}

/// Trait for transmitting frames
pub(crate) trait FrameTx: Send + 'static {
    /// Send a buffer of bytes as a frame
    fn send(&mut self, buf: &[u8]) -> io::Result<()>;
}

/// Trait for receiving frames
pub(crate) trait FrameRx: Send + 'static {
    /// Try to receive a frame, returning immediately if none available
    fn recv_nonblocking(&mut self) -> io::Result<&[u8]>;
}

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
pub(crate) struct QpEntry {
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
pub(crate) enum ReportMeta {
    /// Write operation header
    Write {
        pos: PacketPos,
        msn: u16,
        psn: u32,
        solicited: bool,
        ack_req: bool,
        is_retry: bool,
        dqpn: u32,
        total_len: u32,
        raddr: u64,
        rkey: u32,
        imm: u32,
    },
    /// Read operation header
    Read {
        raddr: u64,
        rkey: u32,
        total_len: u32,
        laddr: u64,
        lkey: u32,
    },
    /// Congestion Notification Packet
    Cnp {
        /// The initiator's QP number
        qpn: u32,
    },
    /// Positive acknowledgment
    Ack {
        qpn: u32,
        msn: u16,
        psn_now: u32,
        now_bitmap: u128,
        is_window_slided: bool,
        is_send_by_local_hw: bool,
        is_send_by_driver: bool,
    },
    /// Negative acknowledgment
    Nak {
        qpn: u32,
        msn: u16,
        psn_now: u32,
        now_bitmap: u128,
        pre_bitmap: u128,
        psn_before_slide: u32,
        is_send_by_local_hw: bool,
        is_send_by_driver: bool,
    },
}
