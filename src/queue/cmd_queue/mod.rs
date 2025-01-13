/// Cmd queue worker
pub(crate) mod worker;

use std::io;

use crate::{
    desc::{
        cmd::{CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT},
        RingBufDescToHost, RingBufDescUntyped,
    },
    device::{
        proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
        CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
    },
    ringbuffer::{Descriptor, RingBuffer},
};

use super::DescRingBuffer;

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
    pub(crate) fn try_pop(&mut self) -> Option<RingBufDescToHost<'_>> {
        self.inner.try_pop().map(Into::into)
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
        assert!(matches!(
            desc,
            // correspond to the default op_code
            RingBufDescToHost::CmdQueueRespDescUpdateMrTable(_)
        ));
    }
}
