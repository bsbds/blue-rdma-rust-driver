mod types;

pub(crate) use types::*;

use std::io;

use crate::net::config::NetworkConfig;

/// RDMA device configuration interface
pub(crate) trait DeviceCommand {
    /// Updates Memory Translation Table entry
    fn update_mtt(&self, update: MttUpdate) -> io::Result<()>;
    /// Updates Page Table entry
    fn update_pgt(&self, update: PgtUpdate) -> io::Result<()>;
    /// Updates Queue Pair entry
    fn update_qp(&self, entry: UpdateQp) -> io::Result<()>;
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
pub(crate) trait MetaReport {
    /// Tries to receive operation metadata
    fn try_recv_meta(&mut self) -> io::Result<Option<ReportMeta>>;
}

/// Simple NIC tunnel interface
pub(crate) trait SimpleNicTunnel {
    /// Frame Sender
    type Sender: FrameTx;
    /// Frame Receiver
    type Receiver: FrameRx;

    /// Splits into send half and recv half
    fn into_split(self) -> (Self::Sender, Self::Receiver);

    fn recv_buffer_virt_addr(&self) -> u64;
}

/// Trait for transmitting frames
pub(crate) trait FrameTx {
    /// Send a buffer of bytes as a frame
    fn send(&mut self, buf: &[u8]) -> io::Result<()>;
}

/// Trait for receiving frames
pub(crate) trait FrameRx {
    /// Try to receive a frame, returning immediately if none available
    fn recv_nonblocking(&mut self) -> io::Result<&[u8]>;
}
