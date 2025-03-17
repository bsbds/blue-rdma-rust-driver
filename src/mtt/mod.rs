/// Mtt allocator
mod alloc;

use std::io;

use alloc::Alloc;

use crate::{
    device_protocol::MttEntry,
    mem::{get_num_page, page::ContiguousPages, virt_to_phy::AddressResolver, PAGE_SIZE},
    ringbuffer::Syncable,
};

/// Memory Translation Table implementation
pub(crate) struct Mtt {
    /// Table memory allocator
    alloc: Alloc,
}

impl Mtt {
    /// Creates a new `Mtt`
    pub(crate) fn new() -> Self {
        Self {
            alloc: Alloc::new(),
        }
    }

    /// Register a memory region
    pub(crate) fn register(&mut self, num_pages: usize) -> io::Result<(u32, Vec<PgtEntry>)> {
        let mr_key = self
            .alloc
            .alloc_mr_key()
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?;
        let pgt_entries = self
            .alloc
            .alloc_pgt_indices(num_pages)
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?;

        Ok((mr_key, pgt_entries))
    }

    /// Validates memory region parameters
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` error if:
    /// - The address + length would overflow u64
    /// - The length is larger than `u32::MAX`
    /// - The length is 0
    #[allow(clippy::arithmetic_side_effects, clippy::as_conversions)]
    fn ensure_valid(addr: u64, length: usize) -> io::Result<()> {
        if u64::MAX - addr < length as u64 || length > u32::MAX as usize || length == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }

    /// Gets starting virtual addresses for each page in memory region
    ///
    /// # Returns
    ///
    /// * `Some(Vec<u64>)` - Vector of page-aligned virtual addresses
    /// * `None` - If addr + length would overflow
    #[allow(clippy::as_conversions)]
    fn get_page_start_virt_addrs(addr: u64, length: usize) -> Option<Vec<u64>> {
        addr.checked_add(length as u64)
            .map(|end| (addr..end).step_by(PAGE_SIZE).collect())
    }

    /// Copies physical addresses into a page.
    ///
    /// # Errors
    ///
    /// Returns an error if the page is too small to hold all addresses.
    fn copy_phy_addrs_to_page<Addrs: IntoIterator<Item = u64>>(
        phy_addrs: Addrs,
        page: &mut ContiguousPages<1>,
    ) -> io::Result<()> {
        let bytes: Vec<u8> = phy_addrs.into_iter().flat_map(u64::to_ne_bytes).collect();
        page.get_mut(..bytes.len())
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?
            .copy_from_slice(&bytes);

        page.sync();

        Ok(())
    }
}

pub(crate) struct PgtEntry {
    pub(crate) index: u32,
    pub(crate) count: u32,
}
