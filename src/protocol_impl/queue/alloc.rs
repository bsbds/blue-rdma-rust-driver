use std::{
    io,
    ops::{Deref, DerefMut},
};

use crate::{
    mem::{
        page::{ContiguousPages, HostPageAllocator, MmapMut, PageAllocator},
        DmaBuf, DmaBufAllocator,
    },
    protocol_impl::desc::DESC_SIZE,
    ringbuf::{DmaRingBuf, RING_BUF_LEN},
    ringbuffer::{DescBuffer, RingBuffer, RingCtx, Syncable},
};

use super::super::desc::RingBufDescUntyped;

/// A buffer backed by contiguous physical pages.
pub(crate) struct PageBuf {
    /// The underlying contiguous physical pages
    inner: MmapMut,
}

impl PageBuf {
    pub(crate) fn new(inner: MmapMut) -> Self {
        Self { inner }
    }
}

impl Syncable for PageBuf {
    fn sync(&self) {
        self.inner.sync();
    }
}

impl AsMut<[RingBufDescUntyped]> for PageBuf {
    #[allow(unsafe_code, clippy::transmute_ptr_to_ptr)]
    fn as_mut(&mut self) -> &mut [RingBufDescUntyped] {
        unsafe { std::mem::transmute(self.inner.as_mut()) }
    }
}

impl AsRef<[RingBufDescUntyped]> for PageBuf {
    #[allow(unsafe_code, clippy::transmute_ptr_to_ptr)]
    fn as_ref(&self) -> &[RingBufDescUntyped] {
        unsafe { std::mem::transmute(self.inner.as_ref()) }
    }
}

impl DescBuffer<RingBufDescUntyped> for PageBuf {}

/// Ring buffer storing RDMA descriptors
pub(crate) struct DescRingBuffer(DmaRingBuf<RingBufDescUntyped>);

impl DescRingBuffer {
    pub(crate) fn new(buf: MmapMut) -> Self {
        let rb = DmaRingBuf::new(buf);
        Self(rb)
    }

    pub(crate) fn remaining(&self) -> usize {
        self.0.remaining()
    }

    pub(crate) fn capacity() -> usize {
        RingBuffer::<PageBuf, RingBufDescUntyped>::capacity()
    }

    pub(crate) fn pop(&mut self) -> Option<RingBufDescUntyped> {
        self.0.pop(RingBufDescUntyped::is_valid)
    }
}

impl Deref for DescRingBuffer {
    type Target = DmaRingBuf<RingBufDescUntyped>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DescRingBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub(crate) struct DescRingBufAllocator<A> {
    dma_buf_allocator: A,
}

impl<A: DmaBufAllocator> DescRingBufAllocator<A> {
    pub(crate) fn new(dma_buf_allocator: A) -> Self {
        Self { dma_buf_allocator }
    }

    pub(crate) fn alloc(&mut self) -> io::Result<DmaBuf> {
        self.dma_buf_allocator.alloc(RING_BUF_LEN * DESC_SIZE)
    }
}
