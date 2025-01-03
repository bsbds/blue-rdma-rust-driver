/// Command queue implementation
pub(crate) mod cmd_queue;

/// Simple NIC tx queue implementation
pub(crate) mod simple_nic;

use std::{io, marker::PhantomData};

use crate::{
    desc::RingBufDescUntyped,
    ring::{Ring, SyncDevice},
};

/// To Card Queue
pub(crate) trait ToCardQueue {
    /// The descriptor type
    type Desc: Into<RingBufDescUntyped>;

    /// Pushes descriptors to the queue.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the queue is full or if there is an error pushing the descriptors.
    fn push<Descs: ExactSizeIterator<Item = Self::Desc>>(&mut self, descs: Descs)
        -> io::Result<()>;
}

/// To Host Queue
pub(crate) trait ToHostQueue {
    /// The descriptor type
    type Desc: for<'a> From<&'a RingBufDescUntyped>;

    /// Returns the next descriptor from the queue, or None if the queue is empty.
    fn pop(&mut self) -> Option<Self::Desc>;
}

/// To card queue for submitting descriptors to the device
pub(super) struct ToCardQueueTyped<Buf, Dev, Desc> {
    /// Inner ring buffer
    inner: Ring<Buf, Dev, RingBufDescUntyped>,
    /// Descriptor Type
    _marker: PhantomData<Desc>,
}

impl<Buf, Dev, Desc> ToCardQueue for ToCardQueueTyped<Buf, Dev, Desc>
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: SyncDevice,
    Desc: Into<RingBufDescUntyped>,
{
    type Desc = Desc;

    fn push<Descs: ExactSizeIterator<Item = Self::Desc>>(
        &mut self,
        descs: Descs,
    ) -> io::Result<()> {
        let descs = descs.map(Into::into);
        self.inner.produce(descs)
    }
}

/// To card queue for submitting descriptors to the device
pub(super) struct ToHostQueueTyped<Buf, Dev, Desc> {
    /// Inner ring buffer
    inner: Ring<Buf, Dev, RingBufDescUntyped>,
    /// Descriptor Type
    _marker: PhantomData<Desc>,
}

impl<Buf, Dev, Desc> ToHostQueue for ToCardQueueTyped<Buf, Dev, Desc>
where
    Buf: AsMut<[RingBufDescUntyped]>,
    Dev: SyncDevice,
    Desc: for<'a> From<&'a RingBufDescUntyped>,
{
    type Desc = Desc;

    fn pop(&mut self) -> Option<Self::Desc> {
        self.inner.try_consume().map(Into::into)
    }
}
