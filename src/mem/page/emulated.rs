use std::{ffi::c_void, io, ops::Range};

use crate::mem::PAGE_SIZE;

use super::{ContiguousPages, MmapMut, PageAllocator};

/// A page allocator for allocating pages of emulated physical memory
#[derive(Debug)]
pub(crate) struct EmulatedPageAllocator<const N: usize> {
    /// Inner
    inner: Vec<MmapMut>,
}

impl<const N: usize> EmulatedPageAllocator<N> {
    /// TODO: implements allocating multiple consecutive pages
    const _OK: () = assert!(
        N == 1,
        "allocating multiple contiguous pages is currently unsupported"
    );

    /// Creates a new `EmulatedPageAllocator`
    #[allow(clippy::as_conversions)] // usize to *mut c_void is safe
    pub(crate) fn new(addr_range: Range<usize>) -> Self {
        let inner: Vec<_> = addr_range
            .step_by(PAGE_SIZE)
            .map(|addr| MmapMut::new(addr as *mut c_void, PAGE_SIZE))
            .collect();

        Self { inner }
    }
}

impl<const N: usize> PageAllocator<N> for EmulatedPageAllocator<N> {
    #[allow(unsafe_code)]
    fn alloc(&mut self) -> io::Result<ContiguousPages<N>> {
        self.inner
            .pop()
            .map(ContiguousPages::new)
            .ok_or(io::ErrorKind::OutOfMemory.into())
    }
}
