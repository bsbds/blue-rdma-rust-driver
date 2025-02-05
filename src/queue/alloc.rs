use std::{
    io,
    ops::{Deref, DerefMut},
};

use memmap2::MmapMut;

use crate::{
    desc::RingBufDescUntyped,
    mem::page::{ContiguousPages, HostPageAllocator, PageAllocator},
    ringbuffer::{RingBuffer, RingCtx},
};

/// A buffer backed by contiguous physical pages.
pub(crate) struct PageBuf {
    /// The underlying contiguous physical pages
    inner: ContiguousPages<1>,
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

/// Allocator for descriptor ring buffers
#[derive(Debug)]
pub(crate) struct DescRingBufferAllocator<A>(A);

impl<A: PageAllocator<1>> DescRingBufferAllocator<A> {
    /// Creates a new `DescRingBufferAllocator` with given page allocator
    pub(crate) fn new(page_allocator: A) -> Self {
        Self(page_allocator)
    }

    /// Allocates a new `DescRingBuffer`
    pub(crate) fn alloc(&mut self) -> io::Result<DescRingBuffer> {
        let buf = self.0.alloc().map(|inner| PageBuf { inner })?;
        let ctx = RingCtx::new();
        let rb = RingBuffer::new(ctx, buf)
            .unwrap_or_else(|| unreachable!("ringbuffer creation should never fail"));
        Ok(DescRingBuffer(rb))
    }

    pub(crate) fn into_inner(self) -> A {
        self.0
    }
}

impl DescRingBufferAllocator<HostPageAllocator<1>> {
    /// Creates a new `DescRingBufferAllocator` with default host page allocator
    pub(crate) fn new_host_allocator() -> Self {
        Self(HostPageAllocator::new())
    }
}

/// Ring buffer storing RDMA descriptors
pub(crate) struct DescRingBuffer(RingBuffer<PageBuf, RingBufDescUntyped>);

impl DescRingBuffer {
    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.0.base_addr()
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
