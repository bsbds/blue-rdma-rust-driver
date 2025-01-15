use std::io;

use crate::{mem::page::ContiguousPages, net::config::NetworkConfig};

/// RDMA device configuration interface
pub(crate) trait DeviceCommand {
    /// Updates Memory Translation Table entry
    fn update_mtt(&self, entry: MttEntry<'_>) -> io::Result<()>;
    /// Updates Queue Pair entry
    fn update_qp(&self, entry: QPEntry) -> io::Result<()>;
    /// Sets network parameters
    fn set_network(&self, param: NetworkConfig) -> io::Result<()>;
    /// Sets receive buffer for raw packets
    fn set_raw_packet_recv_buffer(&self, buffer: RecvBufferMeta) -> io::Result<()>;
}

/// RDMA send operations interface
pub(crate) trait RDMASend {
    /// Sends an RDMA operation
    fn send(&self, op: RDMASendOp) -> io::Result<()>;
}

/// Metadata reporting interface
pub(crate) trait MetaReport {
    /// Tries to receive operation header metadata
    fn try_recv_op_header(&self) -> io::Result<Option<MetaReportOpHeader>>;
    /// Tries to receive operation acknowledgment
    fn try_recv_ack(&self) -> io::Result<Option<Ack>>;
}

/// Simple NIC tunnel interface
pub(crate) trait SimpleNicTunnel: Send + Sync + 'static {
    /// Frame Sender
    type Sender: FrameTx;
    /// Frame Receiver
    type Receiver: FrameRx;

    /// Splits into send half and recv half
    fn into_split(self, recv_buffer: RecvBuffer) -> (Self::Sender, Self::Receiver);
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
    entry_buffer: &'a ContiguousPages<1>,
    mr_base_va: u64,
    mr_length: u32,
    mr_key: u32,
    pd_handler: u32,
    acc_flags: u8,
    pgt_offset: u32,
    dma_addr: u64,
    zero_based_entry_count: u32,
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
pub(crate) struct QPEntry;

/// Receive buffer
pub(crate) struct RecvBuffer {
    /// One page
    inner: ContiguousPages<1>,
}

/// Metadata about a receive buffer
pub(crate) struct RecvBufferMeta {
    /// Physical address of the receive buffer
    phys_addr: u64,
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

/// RDMA send operation types
pub(crate) enum RDMASendOp {
    /// Write operation
    Write,
    /// Read operation
    Read,
}

/// Metadata operation header types
pub(crate) enum MetaReportOpHeader {
    /// Write operation header
    Write,
    /// Read operation header
    Read,
    /// Congestion Notification Packet
    Cnp,
}

/// Operation acknowledgment types
pub(crate) enum Ack {
    /// Positive acknowledgment
    Ack,
    /// Negative acknowledgment
    Nak,
}
