/// Cmd queue worker
pub(crate) mod worker;

use std::io;

use crate::{
    desc::{
        cmd::{CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT},
        RingBufDescToHost, RingBufDescUntyped,
    },
    ring::{Descriptor, Ring, SyncDevice},
};

/// Command queue for submitting commands to the device
pub(crate) struct CmdQueue<Buf, Dev> {
    /// Inner ring buffer
    inner: Ring<Buf, Dev, RingBufDescUntyped>,
}

/// Command queue descriptor types that can be submitted
pub(crate) enum CmdQueueDesc {
    /// Update first stage table command
    UpdateMrTable(CmdQueueReqDescUpdateMrTable),
    /// Update second stage table command
    UpdatePGT(CmdQueueReqDescUpdatePGT),
}

impl<Buf, Dev> CmdQueue<Buf, Dev>
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: SyncDevice,
{
    /// Creates a new `CmdQueue`
    pub(crate) fn new(inner: Ring<Buf, Dev, RingBufDescUntyped>) -> Self {
        Self { inner }
    }

    /// Produces command descriptors to the queue
    pub(crate) fn produce<Descs>(&mut self, descs: Descs) -> io::Result<()>
    where
        Descs: ExactSizeIterator<Item = CmdQueueDesc>,
    {
        let descs = descs.map(|x| match x {
            CmdQueueDesc::UpdateMrTable(d) => d.into(),
            CmdQueueDesc::UpdatePGT(d) => d.into(),
        });
        self.inner.produce(descs)
    }

    /// Flush
    pub(crate) fn flush(&self) -> io::Result<()> {
        self.inner.flush_produce()
    }
}

/// Queue for receiving command responses from the device
struct CmdRespQueue<Buf, Dev> {
    /// Inner ring buffer
    inner: Ring<Buf, Dev, RingBufDescUntyped>,
}

impl<Buf, Dev> CmdRespQueue<Buf, Dev>
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: SyncDevice,
{
    /// Creates a new `CmdRespQueue`
    fn new(inner: Ring<Buf, Dev, RingBufDescUntyped>) -> Self {
        Self { inner }
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_consume(&mut self) -> Option<RingBufDescToHost<'_>> {
        self.inner.try_consume().map(Into::into)
    }
}

#[cfg(test)]
mod test {
    use std::iter;

    use crate::ring::new_test_ring;

    use super::*;

    #[test]
    fn cmd_queue_produce_ok() {
        let ring = new_test_ring::<RingBufDescUntyped>();
        let mut queue = CmdQueue::new(ring);
        let desc = CmdQueueDesc::UpdatePGT(CmdQueueReqDescUpdatePGT::new(1, 1, 1, 1));
        queue.produce(iter::once(desc)).unwrap();
    }

    #[test]
    fn cmd_resp_queue_consume_ok() {
        let mut ring = new_test_ring::<RingBufDescUntyped>();
        let desc = RingBufDescUntyped::new_valid_default();
        ring.produce(iter::once(desc)).unwrap();
        let mut queue = CmdRespQueue::new(ring);
        let desc = queue.try_consume().unwrap();
        assert!(matches!(
            desc,
            // correspond to the default op_code
            RingBufDescToHost::CmdQueueRespDescUpdateMrTable(_)
        ));
    }
}
