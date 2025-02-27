use std::{
    fs::File,
    io::{self, Read, Seek},
};

/// Size of the PFN (Page Frame Number) mask in bytes
const PFN_MASK_SIZE: usize = 8;
/// PFN are bits 0-54 (see pagemap.txt in Linux Documentation)
const PFN_MASK: u64 = (1 << 55) - 1;
/// Bit indicating if a page is present in memory
const PAGE_PRESENT_BIT: u8 = 63;

#[allow(unsafe_code, clippy::cast_sign_loss)]
fn get_page_size() -> u64 {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 }
}

pub(crate) trait AddressResolver {
    /// Converts a list of virtual addresses to physical addresses
    ///
    /// # Returns
    ///
    /// A vector of optional physical addresses. `None` indicates
    /// the page is not present in physical memory.
    ///
    /// # Errors
    ///
    /// Returns an IO error if address resolving fails.
    fn virt_to_phys(&self, virt_addr: u64) -> io::Result<Option<u64>>;

    /// Converts a list of virtual addresses to physical addresses
    ///
    /// # Returns
    ///
    /// A vector of optional physical addresses. `None` indicates
    /// the page is not present in physical memory.
    ///
    /// # Errors
    ///
    /// Returns an IO error if address resolving fails.
    #[allow(clippy::as_conversions)]
    fn virt_to_phys_range(
        &self,
        start_addr: u64,
        num_pages: usize,
    ) -> io::Result<Vec<Option<u64>>> {
        let page_size = get_page_size();
        (0..num_pages as u64)
            .map(|x| self.virt_to_phys(start_addr.saturating_add(x * page_size)))
            .collect::<Result<_, _>>()
    }
}

#[cfg(emulation)]
pub(crate) type PhysAddrResolver = PhysAddrResolverEmulated;
#[cfg(not(emulation))]
pub(crate) type PhysAddrResolver = PhysAddrResolverLinuxX86;

pub(crate) struct PhysAddrResolverLinuxX86;

#[allow(
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    clippy::host_endian_bytes
)]
impl AddressResolver for PhysAddrResolverLinuxX86 {
    fn virt_to_phys(&self, virt_addr: u64) -> io::Result<Option<u64>> {
        let page_size = get_page_size();
        let mut file = File::open("/proc/self/pagemap")?;
        let virt_pfn = virt_addr / page_size;
        let offset = PFN_MASK_SIZE as u64 * virt_pfn;
        let _pos = file.seek(io::SeekFrom::Start(offset))?;
        let mut buf = [0u8; PFN_MASK_SIZE];
        file.read_exact(&mut buf)?;
        let entry = u64::from_ne_bytes(buf);

        if (entry >> PAGE_PRESENT_BIT) & 1 == 0 {
            return Ok(None);
        }

        let phy_pfn = entry & PFN_MASK;
        let phys_addr = phy_pfn * page_size + virt_addr % page_size;

        Ok(Some(phys_addr))
    }

    fn virt_to_phys_range(
        &self,
        start_addr: u64,
        num_pages: usize,
    ) -> io::Result<Vec<Option<u64>>> {
        let page_size = get_page_size();
        let mut phy_addrs = Vec::with_capacity(num_pages);
        let mut file = File::open("/proc/self/pagemap")?;
        let virt_pfn = start_addr / page_size;
        let offset = PFN_MASK_SIZE as u64 * virt_pfn;
        let _pos = file.seek(io::SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; PFN_MASK_SIZE * num_pages];
        file.read_exact(&mut buf)?;

        for chunk in buf
            .chunks(PFN_MASK_SIZE)
            .flat_map(<[u8; PFN_MASK_SIZE]>::try_from)
        {
            let entry = u64::from_ne_bytes(chunk);
            if (entry >> PAGE_PRESENT_BIT) & 1 == 0 {
                phy_addrs.push(None);
                continue;
            }
            let phys_pfn = entry & PFN_MASK;
            let phys_addr = phys_pfn * page_size + start_addr % page_size;
            phy_addrs.push(Some(phys_addr));
        }

        Ok(phy_addrs)
    }
}

pub(crate) struct PhysAddrResolverEmulated {
    heap_start_addr: u64,
}

impl PhysAddrResolverEmulated {
    pub(crate) fn new(heap_start_addr: u64) -> Self {
        Self { heap_start_addr }
    }
}

impl AddressResolver for PhysAddrResolverEmulated {
    fn virt_to_phys(&self, virt_addr: u64) -> io::Result<Option<u64>> {
        Ok(virt_addr.checked_sub(self.heap_start_addr))
    }
}
