use std::{
    io,
    ops::{Deref, DerefMut},
};

use crate::{
    mem::page::{ContiguousPages, HostPageAllocator, MmapMut, PageAllocator},
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
pub(crate) struct DescRingBuffer(RingBuffer<PageBuf, RingBufDescUntyped>);

impl DescRingBuffer {
    pub(crate) fn new(buf: MmapMut) -> Self {
        let ctx = RingCtx::new();
        let page_buf = PageBuf { inner: buf };
        let rb = RingBuffer::new(ctx, page_buf)
            .unwrap_or_else(|| unreachable!("ringbuffer creation should never fail"));
        Self(rb)
    }

    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.0.base_addr()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.0.remaining()
    }

    pub(crate) fn capacity() -> usize {
        RingBuffer::<PageBuf, RingBufDescUntyped>::capacity()
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
