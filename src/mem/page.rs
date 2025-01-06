use std::{io, marker::PhantomData, sync::Arc};

use memmap2::{MmapMut, MmapOptions};

use super::{virt_to_phy::virt_to_phy_range, PAGE_SIZE, PAGE_SIZE_BITS};

use std::ops::{Deref, DerefMut};

/// A wrapper around mapped memory that ensures physical memory pages are consecutive.
pub(crate) struct ContiguousPages {
    /// Mmap handle
    inner: MmapMut,
}

impl ContiguousPages {
    /// Creates a new consecutive memory region of the specified number of pages.
    pub(crate) fn new(num_pages: usize) -> io::Result<Self> {
        /// TODO: implements allocating multiple consecutive pages
        assert_eq!(num_pages, 1, "currently only supports allocating one page");
        let inner = Self::try_reserve_consecutive(num_pages)?;
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

impl Deref for ContiguousPages {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ContiguousPages {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl AsMut<[u8]> for ContiguousPages {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.inner
    }
}

impl AsRef<[u8]> for ContiguousPages {
    fn as_ref(&self) -> &[u8] {
        &self.inner
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn consc_mem_alloc_succ() {
        let mem = ContiguousPages::new(1).expect("failed to allocate");
    }
}
