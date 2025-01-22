use std::io;

use crate::{
    desc::cmd::{CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT},
    mem::{
        page::ContiguousPages,
        virt_to_phy::{virt_to_phy_range, AddressResolver},
        PAGE_SIZE,
    },
    queue::abstr::MttEntry,
};

use super::{
    pgt_alloc::{simple::SimplePgtAlloc, PgtAlloc},
    Alloc,
};

/// Memory Translation Table implementation
pub(crate) struct Mttv2<A = SimplePgtAlloc> {
    /// Table memory allocator
    alloc: Alloc<A>,
}

impl Mttv2<SimplePgtAlloc> {
    /// Creates a new `Mtt` with simple allocator
    pub(crate) fn new_simple() -> Self {
        Self {
            alloc: Alloc::new_simple(),
        }
    }
}

impl<A: PgtAlloc> Mttv2<A> {
    /// Creates a new `Mtt`
    pub(crate) fn new(alloc: Alloc<A>) -> Self {
        Self { alloc }
    }

    /// Register a memory region
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register<'a, R>(
        &mut self,
        addr_resolver: &R,
        page_buffer: &'a mut ContiguousPages<1>,
        page_buffer_phy_addr: u64,
        addr: u64,
        length: usize,
        pd_handle: u32,
        access: u8,
    ) -> io::Result<MttEntry<'a>>
    where
        R: AddressResolver + ?Sized,
    {
        Self::ensure_valid(addr, length)?;
        Self::try_pin_pages(addr, length)?;
        let num_pages = Self::get_num_page(addr, length);
        let virt_addrs = Self::get_page_start_virt_addrs(addr, length)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let phy_addrs = addr_resolver.virt_to_phys_range(addr, num_pages)?;
        if phy_addrs.iter().any(Option::is_none) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "physical address not found",
            ));
        }
        Self::copy_phy_addrs_to_page(phy_addrs.into_iter().flatten(), page_buffer)?;
        let (mr_key, index) = self
            .alloc
            .alloc(num_pages)
            .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?;
        let index_u32 = u32::try_from(index)
            .unwrap_or_else(|_| unreachable!("allocator should not alloc index larger than u32"));
        let length_u32 =
            u32::try_from(length).map_err(|_err| io::Error::from(io::ErrorKind::InvalidInput))?;
        let entry_count = u32::try_from(num_pages.saturating_sub(1))
            .map_err(|_err| io::Error::from(io::ErrorKind::InvalidInput))?;

        let entry = MttEntry::new(
            page_buffer,
            addr,
            length_u32,
            mr_key.0,
            pd_handle,
            access,
            index_u32,
            page_buffer_phy_addr,
            entry_count,
        );

        Ok(entry)
    }

    /// Pins pages in memory to prevent swapping
    ///
    /// # Errors
    ///
    /// Returns an error if the pages could not be locked in memory
    #[allow(unsafe_code, clippy::as_conversions)]
    fn try_pin_pages(addr: u64, length: usize) -> io::Result<()> {
        let result = unsafe { libc::mlock(addr as *const std::ffi::c_void, length) };
        if result != 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "failed to lock pages"));
        }
        Ok(())
    }

    /// Unpins pages
    ///
    /// # Errors
    ///
    /// Returns an error if the pages could not be locked in memory
    #[allow(unsafe_code, clippy::as_conversions)]
    fn try_unpin_pages(addr: u64, length: usize) -> io::Result<()> {
        let result = unsafe { libc::munlock(addr as *const std::ffi::c_void, length) };
        if result != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to unlock pages",
            ));
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

    /// Calculates number of pages needed for memory region
    #[allow(clippy::arithmetic_side_effects)]
    fn get_num_page(addr: u64, length: usize) -> usize {
        let num = length / PAGE_SIZE;
        if length % PAGE_SIZE != 0 {
            num + 1
        } else {
            num
        }
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

        Ok(())
    }
}
