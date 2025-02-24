use std::io;

use crate::{
    device_protocol::DeviceCommand,
    net::config::NetworkConfig,
    protocol_impl::device::{
        proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
        CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
    },
    ringbuffer::{Descriptor, RingBuffer},
};

use super::super::desc::{
    CmdQueueReqDescQpManagement, CmdQueueReqDescSetNetworkParam,
    CmdQueueReqDescSetRawPacketReceiveMeta, CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
    CmdQueueRespDescOnlyCommonHeader, RingBufDescUntyped,
};
use super::DescRingBuffer;

/// Command queue for submitting commands to the device
pub(crate) struct CmdQueue {
    /// Inner ring buffer
    inner: DescRingBuffer,
}

/// Command queue descriptor types that can be submitted
#[derive(Debug, Clone, Copy)]
pub(crate) enum CmdQueueDesc {
    /// Update first stage table command
    UpdateMrTable(CmdQueueReqDescUpdateMrTable),
    /// Update second stage table command
    UpdatePGT(CmdQueueReqDescUpdatePGT),
    /// Manage Queue Pair operations
    ManageQP(CmdQueueReqDescQpManagement),
    /// Set network parameters
    SetNetworkParam(CmdQueueReqDescSetNetworkParam),
    /// Set metadata for raw packet receive operations
    SetRawPacketReceiveMeta(CmdQueueReqDescSetRawPacketReceiveMeta),
}

impl CmdQueue {
    /// Creates a new `CmdQueue`
    pub(crate) fn new(ring_buffer: DescRingBuffer) -> Self {
        Self { inner: ring_buffer }
    }

    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.inner.base_addr()
    }

    /// Produces command descriptors to the queue
    pub(crate) fn push(&mut self, desc: CmdQueueDesc) -> io::Result<()> {
        match desc {
            CmdQueueDesc::UpdateMrTable(d) => self.inner.push(d.into()),
            CmdQueueDesc::UpdatePGT(d) => self.inner.push(d.into()),
            CmdQueueDesc::ManageQP(d) => self.inner.push(d.into()),
            CmdQueueDesc::SetNetworkParam(d) => self.inner.push(d.into()),
            CmdQueueDesc::SetRawPacketReceiveMeta(d) => self.inner.push(d.into()),
        }
    }

    /// Returns the head pointer
    pub(crate) fn head(&self) -> u32 {
        self.inner.head()
    }
}

/// Command queue response descriptor type
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdRespQueueDesc(CmdQueueRespDescOnlyCommonHeader);

impl std::ops::Deref for CmdRespQueueDesc {
    type Target = CmdQueueRespDescOnlyCommonHeader;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Queue for receiving command responses from the device
pub(crate) struct CmdRespQueue {
    /// Inner ring buffer
    inner: DescRingBuffer,
}

impl CmdRespQueue {
    /// Creates a new `CmdRespQueue`
    pub(crate) fn new(ring_buffer: DescRingBuffer) -> Self {
        Self { inner: ring_buffer }
    }

    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.inner.base_addr()
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<CmdRespQueueDesc> {
        self.inner
            .try_pop()
            .copied()
            .map(Into::into)
            .map(CmdRespQueueDesc)
    }

    /// Return tail pointer
    pub(crate) fn tail(&self) -> u32 {
        self.inner.tail()
    }
}

#[cfg(test)]
mod test {
    use std::iter;

    use crate::{
        mem::page::HostPageAllocator, protocol_impl::device::dummy::DummyDevice,
        protocol_impl::queue::DescRingBufferAllocator, ringbuffer::new_test_ring,
    };

    use super::*;

    #[test]
    fn cmd_queue_produce_ok() {
        let ring = new_test_ring::<RingBufDescUntyped>();
        let buffer = DescRingBufferAllocator::new_host_allocator()
            .alloc()
            .unwrap();
        let mut queue = CmdQueue::new(buffer);
        let desc = CmdQueueDesc::UpdatePGT(CmdQueueReqDescUpdatePGT::new(1, 1, 1, 1));
        queue.push(desc).unwrap();
    }

    #[test]
    fn cmd_resp_queue_consume_ok() {
        let mut ring = new_test_ring::<RingBufDescUntyped>();
        let buffer = DescRingBufferAllocator::new_host_allocator()
            .alloc()
            .unwrap();
        let desc = RingBufDescUntyped::new_valid_default();
        ring.push(desc).unwrap();
        let mut queue = CmdRespQueue::new(buffer);
        let desc = queue.try_pop().unwrap();
    }
}
