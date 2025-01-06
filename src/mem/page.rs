use std::{io, marker::PhantomData, sync::Arc};

use memmap2::{MmapMut, MmapOptions};

use super::{virt_to_phy::virt_to_phy_range, PAGE_SIZE, PAGE_SIZE_BITS};

use std::ops::{Deref, DerefMut};

/// A wrapper around mapped memory that ensures physical memory pages are consecutive.
pub(crate) struct ContiguousPages<const N: usize> {
    /// Mmap handle
    inner: MmapMut,
}

impl<const N: usize> ContiguousPages<N> {
    /// TODO: implements allocating multiple consecutive pages
    const _OK: () = assert!(
        N == 1,
        "allocating multiple contiguous pages is currently unsupported"
    );

    /// Creates a new consecutive memory region of the specified number of pages.
    pub(crate) fn new() -> io::Result<Self> {
        let inner = Self::try_reserve_consecutive(N)?;
        Ok(Self { inner })
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
        let mmap = MmapOptions::new().len(len).map_anon()?;

        mmap.lock()?;

        Ok(mmap)
    }

    /// Checks if the physical pages backing the memory mapping are consecutive.
    #[allow(clippy::as_conversions)] // casting usize ot u64 is safe
    fn ensure_consecutive(mmap: &MmapMut) -> io::Result<bool> {
        let virt_addrs = mmap.chunks(PAGE_SIZE).map(<[u8]>::as_ptr);
        let phy_addrs = virt_to_phy_range(mmap.as_ptr(), mmap.len() >> PAGE_SIZE_BITS)?;
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn consc_mem_alloc_succ() {
        let mem = ContiguousPages::<1>::new().expect("failed to allocate");
    }
}
