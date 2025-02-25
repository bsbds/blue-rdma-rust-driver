use std::{ffi::c_void, io, marker::PhantomData, sync::Arc};

use std::ops::{Deref, DerefMut};

use crate::mem::{virt_to_phy::virt_to_phy_range, PAGE_SIZE, PAGE_SIZE_BITS};

use super::{ContiguousPages, MmapMut, PageAllocator};

/// A page allocator for allocating pages of host memory
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct HostPageAllocator<const N: usize>;

impl<const N: usize> PageAllocator<N> for HostPageAllocator<N> {
    fn alloc(&mut self) -> io::Result<ContiguousPages<N>> {
        let inner = Self::try_reserve_consecutive(N)?;
        Ok(ContiguousPages { inner })
    }
}

impl<const N: usize> HostPageAllocator<N> {
    /// TODO: implements allocating multiple consecutive pages
    const _OK: () = assert!(
        N == 1,
        "allocating multiple contiguous pages is currently unsupported"
    );

    /// Creates a new `HostPageAllocator`
    pub(crate) fn new() -> Self {
        Self
    }

    /// Attempts to reserve consecutive physical memory pages.
    fn try_reserve_consecutive(num_pages: usize) -> io::Result<MmapMut> {
        let mmap = Self::reserve(num_pages)?;
        if Self::ensure_consecutive(&mmap)? {
            return Ok(mmap);
        }

        Err(io::Error::from(io::ErrorKind::OutOfMemory))
    }

    /// Reserves memory pages using mmap.
    #[allow(unsafe_code)]
    fn reserve(num_pages: usize) -> io::Result<MmapMut> {
        /// Number of bits representing a 4K page size
        const PAGE_SIZE_4K_BITS: u8 = 12;
        let len = PAGE_SIZE
            .checked_mul(num_pages)
            .ok_or(io::Error::from(io::ErrorKind::Unsupported))?;
        #[cfg(feature = "page_size_2m")]
        let mmap = MmapOptions::new()
            .len(len)
            .huge(Some(PAGE_SIZE_BITS))
            .map_anon()?;
        #[cfg(feature = "page_size_4k")]
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_ANON,
                -1,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        unsafe {
            if libc::mlock(ptr, len) != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        Ok(MmapMut::new(ptr, len))
    }

    /// Checks if the physical pages backing the memory mapping are consecutive.
    #[allow(clippy::as_conversions)] // casting usize ot u64 is safe
    fn ensure_consecutive(mmap: &MmapMut) -> io::Result<bool> {
        let virt_addrs = mmap.chunks(PAGE_SIZE).map(<[u8]>::as_ptr);
        let phy_addrs = virt_to_phy_range(mmap.as_ptr() as u64, mmap.len() >> PAGE_SIZE_BITS)?;
        if phy_addrs.iter().any(Option::is_none) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "physical address not found",
            ));
        }
        let is_consec = phy_addrs
            .iter()
            .flatten()
            .skip(1)
            .zip(phy_addrs.iter().flatten())
            .all(|(a, b)| a.saturating_sub(*b) == PAGE_SIZE as u64);

        Ok(is_consec)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn consc_mem_alloc_succ() {
        let mem = HostPageAllocator::<1>::new()
            .alloc()
            .expect("failed to allocate");
    }
}
