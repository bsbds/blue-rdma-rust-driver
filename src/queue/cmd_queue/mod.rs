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

/// Command queue for submitting commands to the device
pub(crate) struct CmdQueue<Buf, Dev> {
    /// Inner ring buffer
    inner: RingBuffer<Buf, RingBufDescUntyped>,
    /// The CSR proxy
    proxy: CmdQueueCsrProxy<Dev>,
}

/// Command queue descriptor types that can be submitted
#[derive(Clone, Copy)]
pub(crate) enum CmdQueueDesc {
    /// Update first stage table command
    UpdateMrTable(CmdQueueReqDescUpdateMrTable),
    /// Update second stage table command
    UpdatePGT(CmdQueueReqDescUpdatePGT),
}

impl<Buf, Dev> CmdQueue<Buf, Dev>
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: DeviceAdaptor,
{
    /// Creates a new `CmdQueue`
    pub(crate) fn new(inner: RingBuffer<Buf, RingBufDescUntyped>, device: Dev) -> Self {
        Self {
            inner,
            proxy: CmdQueueCsrProxy(device),
        }
    }

    /// Produces command descriptors to the queue
    pub(crate) fn push(&mut self, desc: CmdQueueDesc) -> io::Result<()> {
        let desc = match desc {
            CmdQueueDesc::UpdateMrTable(d) => d.into(),
            CmdQueueDesc::UpdatePGT(d) => d.into(),
        };
        self.inner.push(desc)
    }

    /// Flush
    pub(crate) fn flush(&self) -> io::Result<()> {
        self.proxy.write_head(self.inner.head())
    }
}

/// Queue for receiving command responses from the device
struct CmdRespQueue<Buf, Dev> {
    /// Inner ring buffer
    inner: RingBuffer<Buf, RingBufDescUntyped>,
    /// The CSR proxy
    proxy: CmdRespQueueCsrProxy<Dev>,
}

impl<Buf, Dev> CmdRespQueue<Buf, Dev>
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: DeviceAdaptor,
{
    /// Creates a new `CmdRespQueue`
    fn new(inner: RingBuffer<Buf, RingBufDescUntyped>, device: Dev) -> Self {
        Self {
            inner,
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

    use crate::{device::dummy::DummyDevice, ringbuffer::new_test_ring};

    use super::*;

    #[test]
    fn cmd_queue_produce_ok() {
        let ring = new_test_ring::<RingBufDescUntyped>();
        let mut queue = CmdQueue::new(ring, DummyDevice::default());
        let desc = CmdQueueDesc::UpdatePGT(CmdQueueReqDescUpdatePGT::new(1, 1, 1, 1));
        queue.push(desc).unwrap();
    }

    #[test]
    fn cmd_resp_queue_consume_ok() {
        let mut ring = new_test_ring::<RingBufDescUntyped>();
        let desc = RingBufDescUntyped::new_valid_default();
        ring.push(desc).unwrap();
        let mut queue = CmdRespQueue::new(ring, DummyDevice::default());
        let desc = queue.try_pop().unwrap();
        assert!(matches!(
            desc,
            // correspond to the default op_code
            RingBufDescToHost::CmdQueueRespDescUpdateMrTable(_)
        ));
    }
}
