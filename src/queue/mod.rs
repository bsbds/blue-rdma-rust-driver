/// Command queue implementation
pub(crate) mod cmd_queue;

/// Simple NIC tx queue implementation
pub(crate) mod simple_nic;

use std::{
    io,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use memmap2::MmapMut;

use crate::{
    desc::RingBufDescUntyped,
    mem::page::ContiguousPages,
    ringbuffer::{RingBuffer, RingCtx},
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
    fn push(&mut self, descs: Self::Desc) -> io::Result<()>;
}

/// To Host Queue
pub(crate) trait ToHostQueue {
    /// The descriptor type
    type Desc: From<RingBufDescUntyped>;

    /// Returns the next descriptor from the queue, or None if the queue is empty.
    fn pop(&mut self) -> Option<Self::Desc>;
}

/// A buffer backed by contiguous physical pages.
struct PageBuf {
    /// The underlying contiguous physical pages
    inner: ContiguousPages<1>,
}

impl PageBuf {
    /// Allocates a new `ContiguousPages` with a size of 1 page.
    fn alloc() -> io::Result<Self> {
        ContiguousPages::<1>::new().map(|inner| Self { inner })
    }
}

impl AsMut<[RingBufDescUntyped]> for PageBuf {
    #[allow(unsafe_code, clippy::transmute_ptr_to_ptr)]
    fn as_mut(&mut self) -> &mut [RingBufDescUntyped] {
        unsafe { std::mem::transmute(self.inner.as_mut()) }
    }
}

/// Ring buffer storing RDMA descriptors
struct DescRingBuffer(RingBuffer<PageBuf, RingBufDescUntyped>);

impl DescRingBuffer {
    /// Allocates a new `DescRingBuffer`
    fn alloc() -> io::Result<Self> {
        let buf = PageBuf::alloc()?;
        let ctx = RingCtx::new();
        let rb = RingBuffer::new(ctx, buf)
            .unwrap_or_else(|| unreachable!("ringbuffer creation should never fail"));
        Ok(Self(rb))
    }
}

impl Deref for DescRingBuffer {
    type Target = RingBuffer<PageBuf, RingBufDescUntyped>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DescRingBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
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

    fn push(&mut self, desc: Desc) -> io::Result<()> {
        self.inner.push(desc.into())
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
