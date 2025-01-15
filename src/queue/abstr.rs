use std::io;

use crate::{mem::page::ContiguousPages, net::config::NetworkConfig};

/// RDMA device configuration interface
pub(crate) trait DeviceCommand {
    /// Updates Memory Translation Table entry
    fn update_mtt(&self, entry: MttEntry) -> io::Result<()>;
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
pub(crate) trait SimpleNicTunnel {
    /// Sends a raw frame
    fn send_frame(&self, frame_data: &[u8]) -> io::Result<()>;
    /// Receives a raw frame
    fn recv_frame(&self, buf: &mut [u8]) -> io::Result<()>;
}

/// Memory Translation Table entry
pub(crate) struct MttEntry;
/// Queue Pair entry
pub(crate) struct QPEntry;

/// Receive buffer
pub(crate) struct RecvBuffer {
    inner: ContiguousPages<1>,
}

pub(crate) struct RecvBufferMeta {
    addr: u64,
}

impl RecvBuffer {
    pub(crate) fn new(inner: ContiguousPages<1>) -> Self {
        Self { inner }
    }

    pub(crate) fn meta(&self) -> RecvBufferMeta {
        RecvBufferMeta {
            addr: self.inner.addr(),
        }
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
}

/// Operation acknowledgment types
pub(crate) enum Ack {
    /// Positive acknowledgment
    Ack,
    /// Negative acknowledgment
    Nak,
}
