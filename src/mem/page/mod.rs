/// Host physical page allocator
mod host;

/// Emulated page allocator
mod emulated;

pub(crate) use emulated::EmulatedPageAllocator;
pub(crate) use host::HostPageAllocator;

use std::{
    ffi::c_void,
    io,
    ops::{Deref, DerefMut},
    slice,
};

/// A trait for allocating contiguous physical memory pages.
///
/// The generic parameter `N` specifies the number of contiguous pages to allocate.
pub(crate) trait PageAllocator<const N: usize> {
    /// Allocates N contiguous physical memory pages.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing either:
    /// - `Ok(ContiguousPages<N>)` - The allocated contiguous pages
    /// - `Err(e)` - An I/O error if allocation fails
    fn alloc(&mut self) -> io::Result<ContiguousPages<N>>;
}

/// A wrapper around mapped memory that ensures physical memory pages are consecutive.
pub(crate) struct ContiguousPages<const N: usize> {
    /// Mmap handle
    pub(super) inner: MmapMut,
}

impl<const N: usize> ContiguousPages<N> {
    /// Returns the start address
    #[allow(clippy::as_conversions)] // converting *mut c_void to u64
    pub(crate) fn addr(&self) -> u64 {
        self.inner.ptr as u64
    }

    /// Creates a new `ContiguousPages`
    pub(super) fn new(inner: MmapMut) -> Self {
        Self { inner }
    }

    pub(crate) fn flush(&self) {
        self.inner.flush();
    }
}

impl<const N: usize> Deref for ContiguousPages<N> {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<const N: usize> DerefMut for ContiguousPages<N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<const N: usize> AsMut<[u8]> for ContiguousPages<N> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.inner
    }
}

impl<const N: usize> AsRef<[u8]> for ContiguousPages<N> {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}

/// Memory-mapped region of host memory.
#[derive(Debug)]
pub(crate) struct MmapMut {
    /// Raw pointer to the start of the mapped memory region
    ptr: *mut c_void,
    /// Length of the mapped memory region in bytes
    len: usize,
}

impl MmapMut {
    /// Creates a new `MmapMut`
    pub(crate) fn new(ptr: *mut c_void, len: usize) -> Self {
        Self { ptr, len }
    }
}

#[allow(unsafe_code)]
#[allow(clippy::as_conversions, clippy::ptr_as_ptr)] // converting among different pointer types
/// Implementations of `MmapMut`
mod mmap_mut_impl {
    use std::{
        arch::x86_64::{_mm_clflush, _mm_mfence},
        ops::{Deref, DerefMut},
        slice,
    };

    use super::MmapMut;

    impl Drop for MmapMut {
        fn drop(&mut self) {
            let _ignore = unsafe { libc::munmap(self.ptr, self.len) };
        }
    }

    unsafe impl Sync for MmapMut {}
    #[allow(unsafe_code)]
    unsafe impl Send for MmapMut {}

    #[allow(unsafe_code)]
    impl Deref for MmapMut {
        type Target = [u8];

        #[inline]
        fn deref(&self) -> &[u8] {
            unsafe { slice::from_raw_parts(self.ptr as *const u8, self.len) }
        }
    }

    #[allow(unsafe_code)]
    impl DerefMut for MmapMut {
        #[inline]
        fn deref_mut(&mut self) -> &mut [u8] {
            unsafe { slice::from_raw_parts_mut(self.ptr as *mut u8, self.len) }
        }
    }

    #[allow(unsafe_code)]
    impl AsRef<[u8]> for MmapMut {
        #[inline]
        fn as_ref(&self) -> &[u8] {
            self
        }
    }

    #[allow(unsafe_code)]
    impl AsMut<[u8]> for MmapMut {
        #[inline]
        fn as_mut(&mut self) -> &mut [u8] {
            self
        }
    }

    #[allow(unsafe_code, clippy::multiple_inherent_impl)]
    impl MmapMut {
        pub(crate) fn flush(&self) {
            const CACHE_LINE_SIZE: usize = 64;

            unsafe {
                for offset in (0..self.len).step_by(CACHE_LINE_SIZE) {
                    Self::flush_cache_line(self.ptr.cast::<u8>().add(offset));
                }

                _mm_mfence();
            }
        }

        fn flush_cache_line(addr: *const u8) {
            unsafe {
                _mm_clflush(addr);
            }
        }
    }
}
