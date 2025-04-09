use std::{
    io,
    ops::{Deref, DerefMut},
};

/// Tools for converting virtual address to physicall address
pub(crate) mod virt_to_phy;

/// Page implementation
pub(crate) mod page;

/// Slot allocation implementation
pub(crate) mod slot_alloc;

pub(crate) mod dmabuf;

pub(crate) mod u_dma_buf;

mod utils;

use page::MmapMut;
pub(crate) use utils::*;

/// Number of bits for a 4KB page size
#[cfg(target_arch = "x86_64")]
#[cfg(feature = "page_size_4k")]
pub(crate) const PAGE_SIZE_BITS: u8 = 12;

/// Number of bits for a 2MB huge page size
#[cfg(feature = "page_size_2m")]
pub(crate) const PAGE_SIZE_BITS: u8 = 21;

/// Size of a 2MB huge page in bytes
pub(crate) const PAGE_SIZE: usize = 1 << PAGE_SIZE_BITS;

/// Asserts system page size matches the expected page size.
///
/// # Panics
///
/// Panics if the system page size does not equal `HUGE_PAGE_2MB_SIZE`.
pub(crate) fn assert_equal_page_size() {
    assert_eq!(page_size(), PAGE_SIZE, "page size not match");
}

/// Returns the current page size
#[allow(
    unsafe_code, // Safe because sysconf(_SC_PAGESIZE) is guaranteed to return a valid value.
    clippy::as_conversions,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub(crate) fn page_size() -> usize {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

pub(crate) struct PageWithPhysAddr {
    pub(crate) page: page::ContiguousPages<1>,
    pub(crate) phys_addr: u64,
}

impl PageWithPhysAddr {
    pub(crate) fn new(page: page::ContiguousPages<1>, phys_addr: u64) -> Self {
        Self { page, phys_addr }
    }

    pub(crate) fn alloc<A, R>(allocator: &mut A, resolver: &R) -> io::Result<Self>
    where
        A: page::PageAllocator<1>,
        R: virt_to_phy::AddressResolver,
    {
        let page = allocator.alloc()?;
        let phys_addr = resolver
            .virt_to_phys(page.addr())?
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;

        Ok(Self { page, phys_addr })
    }
}

pub(crate) struct DmaBuf {
    pub(crate) buf: MmapMut,
    pub(crate) phys_addr: u64,
}

impl DmaBuf {
    pub(crate) fn new(buf: MmapMut, phys_addr: u64) -> Self {
        Self { buf, phys_addr }
    }

    pub(crate) fn phys_addr(&self) -> u64 {
        self.phys_addr
    }
}

impl Deref for DmaBuf {
    type Target = MmapMut;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl DerefMut for DmaBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl AsRef<[u8]> for DmaBuf {
    fn as_ref(&self) -> &[u8] {
        &self.buf
    }
}

impl AsMut<[u8]> for DmaBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }
}

pub(crate) trait DmaBufAllocator {
    fn alloc(&mut self, len: usize) -> io::Result<DmaBuf>;
}

impl<A: DmaBufAllocator> DmaBufAllocator for &mut A {
    fn alloc(&mut self, len: usize) -> io::Result<DmaBuf> {
        (**self).alloc(len)
    }
}
