/// Mtt allocator
mod alloc;

use std::{collections::HashMap, io, mem::take};

use alloc::Alloc;

use crate::{
    device_protocol::MttUpdate,
    mem::{get_num_page, page::ContiguousPages, virt_to_phy::AddressResolver, PAGE_SIZE},
    ringbuffer::Syncable,
};

/// Memory Translation Table implementation
pub(crate) struct Mtt {
    /// Table memory allocator
    alloc: Alloc,
    /// Table tracks `mr_key` to `PgtEntry` mapping
    mrkey_map: HashMap<u32, PgtEntry>,
}

impl Mtt {
    /// Creates a new `Mtt`
    pub(crate) fn new() -> Self {
        Self {
            alloc: Alloc::new(),
            mrkey_map: HashMap::new(),
        }
    }

    /// Register a memory region
    pub(crate) fn register(&mut self, num_pages: usize) -> io::Result<(u32, PgtEntry)> {
        let (mr_key, pgt_entry) = self
            .alloc
            .alloc(num_pages)
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?;
        debug_assert!(
            self.mrkey_map.insert(mr_key, pgt_entry).is_none(),
            "mr_key exist"
        );

        Ok((mr_key, pgt_entry))
    }

    /// Deregister a memory region
    pub(crate) fn deregister(&mut self, mr_key: u32) -> io::Result<()> {
        let entry = self
            .mrkey_map
            .remove(&mr_key)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        if !self
            .alloc
            .dealloc(mr_key, entry.index as usize, entry.count as usize)
        {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(())
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

#[derive(Clone, Copy)]
pub(crate) struct PgtEntry {
    pub(crate) index: u32,
    pub(crate) count: u32,
}
