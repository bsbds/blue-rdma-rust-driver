/// Cmd queue worker
pub(crate) mod worker;

use std::io;

use crate::{
    desc::{
        cmd::{
            CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
            CmdQueueRespDescOnlyCommonHeader,
        },
        RingBufDescUntyped,
    },
    device::{
        proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
        CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
    },
    net::config::NetworkConfig,
    ringbuffer::{Descriptor, RingBuffer},
};

use super::{abstr::DeviceCommand, DescRingBuffer};

/// Controller of the command queue
pub(crate) struct CommandController<Dev> {
    /// The command request queue
    cmd_queue: CmdQueue<Dev>,
}

impl<Dev> CommandController<Dev> {
    /// Creates a new command controller instance
    ///
    /// # Returns
    /// A new `CommandController` with an initialized command queue
    pub(crate) fn new() -> Self {
        todo!()
    }
}

impl<Dev> DeviceCommand for CommandController<Dev> {
    fn update_mtt(&self, entry: crate::queue::abstr::MttEntry) -> io::Result<()> {
        todo!()
    }

    fn update_qp(&self, entry: crate::queue::abstr::QPEntry) -> io::Result<()> {
        todo!()
    }

    fn set_network(&self, param: NetworkConfig) -> io::Result<()> {
        todo!()
    }

    fn set_raw_packet_recv_buffer(
        &self,
        buffer: crate::queue::abstr::RecvBufferMeta,
    ) -> io::Result<()> {
        todo!()
    }
}

/// Command queue for submitting commands to the device
pub(crate) struct CmdQueue<Dev> {
    /// Inner ring buffer
    inner: DescRingBuffer,
    /// The CSR proxy
    proxy: CmdQueueCsrProxy<Dev>,
}

/// Command queue descriptor types that can be submitted
#[derive(Debug, Clone, Copy)]
pub(crate) enum CmdQueueDesc {
    /// Update first stage table command
    UpdateMrTable(CmdQueueReqDescUpdateMrTable),
    /// Update second stage table command
    UpdatePGT(CmdQueueReqDescUpdatePGT),
}

impl<Dev: DeviceAdaptor> CmdQueue<Dev> {
    /// Creates a new `CmdQueue`
    pub(crate) fn new(device: Dev, ring_buffer: DescRingBuffer) -> Self {
        Self {
            inner: ring_buffer,
            proxy: CmdQueueCsrProxy(device),
        }
    }

    /// Produces command descriptors to the queue
    pub(crate) fn push(&mut self, desc: CmdQueueDesc) -> Result<(), CmdQueueDesc> {
        match desc {
            CmdQueueDesc::UpdateMrTable(d) => self
                .inner
                .push(d.into())
                .map_err(Into::into)
                .map_err(CmdQueueDesc::UpdateMrTable),
            CmdQueueDesc::UpdatePGT(d) => self
                .inner
                .push(d.into())
                .map_err(Into::into)
                .map_err(CmdQueueDesc::UpdatePGT),
        }
    }

    /// Flush
    pub(crate) fn flush(&self) -> io::Result<()> {
        self.proxy.write_head(self.inner.head())
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
struct CmdRespQueue<Dev> {
    /// Inner ring buffer
    inner: DescRingBuffer,
    /// The CSR proxy
    proxy: CmdRespQueueCsrProxy<Dev>,
}

impl<Dev: DeviceAdaptor> CmdRespQueue<Dev> {
    /// Creates a new `CmdRespQueue`
    fn new(device: Dev, ring_buffer: DescRingBuffer) -> Self {
        Self {
            inner: ring_buffer,
            proxy: CmdRespQueueCsrProxy(device),
        }
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<CmdRespQueueDesc> {
        self.inner
            .try_pop()
            .copied()
            .map(Into::into)
            .map(CmdRespQueueDesc)
    }

    /// Flush
    pub(crate) fn flush(&self) -> io::Result<()> {
        self.proxy.write_tail(self.inner.tail())
    }
}

#[cfg(test)]
mod test {
    use std::iter;

    use crate::{
        device::dummy::DummyDevice, mem::page::HostPageAllocator, queue::DescRingBufferAllocator,
        ringbuffer::new_test_ring,
    };

    use super::*;

    #[test]
    fn cmd_queue_produce_ok() {
        let ring = new_test_ring::<RingBufDescUntyped>();
        let buffer = DescRingBufferAllocator::new_host_allocator()
            .alloc()
            .unwrap();
        let mut queue = CmdQueue::new(DummyDevice::default(), buffer);
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
        let mut queue = CmdRespQueue::new(DummyDevice::default(), buffer);
        let desc = queue.try_pop().unwrap();
    }
}
