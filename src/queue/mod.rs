/// Command queue implementation
pub(crate) mod cmd_queue;

/// Simple NIC tx queue implementation
pub(crate) mod simple_nic;

use std::{io, marker::PhantomData};

use memmap2::MmapMut;

use crate::{desc::RingBufDescUntyped, ringbuffer::RingBuffer};

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

struct RingPageBuf {
    inner: MmapMut,
}

impl AsMut<[RingBufDescUntyped]> for RingPageBuf {
    #[allow(unsafe_code)]
    fn as_mut(&mut self) -> &mut [RingBufDescUntyped] {
        unsafe { std::mem::transmute(self.inner.as_mut()) }
    }
}

type DescRingBuffer = RingBuffer<RingPageBuf, RingBufDescUntyped>;

/// To Host Queue
pub(crate) trait ToHostQueue {
    /// The descriptor type
    type Desc: From<RingBufDescUntyped>;

    /// Returns the next descriptor from the queue, or None if the queue is empty.
    fn pop(&mut self) -> Option<Self::Desc>;
}

/// To card queue for submitting descriptors to the device
pub(crate) struct ToCardQueueTyped<Desc> {
    /// Inner ring buffer
    inner: DescRingBuffer,
    /// Descriptor Type
    _marker: PhantomData<Desc>,
}

impl<Desc> ToCardQueue for ToCardQueueTyped<Desc>
where
    Desc: Into<RingBufDescUntyped>,
{
    type Desc = Desc;

    fn push<Descs: ExactSizeIterator<Item = Self::Desc>>(
        &mut self,
        descs: Descs,
    ) -> io::Result<()> {
        let descs = descs.map(Into::into);
        self.inner.push(descs)
    }
}

/// To card queue for submitting descriptors to the device
pub(super) struct ToHostQueueTyped<Desc> {
    /// Inner ring buffer
    inner: DescRingBuffer,
    /// Descriptor Type
    _marker: PhantomData<Desc>,
}

impl<Desc> ToHostQueue for ToHostQueueTyped<Desc>
where
    Desc: From<RingBufDescUntyped>,
{
    type Desc = Desc;

    fn pop(&mut self) -> Option<Self::Desc> {
        self.inner.try_pop().copied().map(Into::into)
    }
}
