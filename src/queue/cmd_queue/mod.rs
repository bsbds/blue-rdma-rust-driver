/// Cmd queue worker
pub(crate) mod worker;

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
    /// Produces command descriptors to the queue
    pub(crate) fn produce<Descs>(&mut self, descs: Descs)
    where
        Descs: ExactSizeIterator<Item = CmdQueueDesc>,
    {
        let descs = descs.map(|x| match x {
            CmdQueueDesc::UpdateMrTable(d) => d.into(),
            CmdQueueDesc::UpdatePGT(d) => d.into(),
        });
        self.inner.produce(descs);
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
    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_consume(&mut self) -> Option<RingBufDescToHost<'_>> {
        self.inner.try_consume().map(Into::into)
    }
}
