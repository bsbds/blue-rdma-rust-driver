use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::PathBuf,
    ptr,
};

use tracing_subscriber::Layer;

use super::{
    page::{ContiguousPages, MmapMut, PageAllocator},
    DmaBuf, DmaBufAllocator, PageWithPhysAddr,
};

const CLASS_PATH: &str = "/sys/class/u-dma-buf/udmabuf0";

pub(crate) struct UDmaBufAllocator {
    fd: File,
    offset: usize,
}

impl UDmaBufAllocator {
    pub(crate) fn open() -> io::Result<Self> {
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_SYNC)
            .open("/dev/udmabuf0")?;

        Ok(Self { fd, offset: 0 })
    }

    pub(crate) fn size_total() -> io::Result<usize> {
        Self::read_attribute("size")?.parse().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to parse size: {e}"),
            )
        })
    }

    pub(crate) fn phys_addr() -> io::Result<u64> {
        let str = Self::read_attribute("phys_addr")?;
        u64::from_str_radix(str.trim_start_matches("0x"), 16).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to parse size: {e}"),
            )
        })
    }

    fn read_attribute(attr: &str) -> io::Result<String> {
        let path = PathBuf::from(CLASS_PATH).join(attr);
        let mut content = String::new();
        let _ignore = File::open(&path)?.read_to_string(&mut content)?;
        Ok(content.trim().to_owned())
    }

    #[allow(clippy::cast_possible_wrap)]
    fn create(&mut self, len: usize) -> io::Result<DmaBuf> {
        let size_total = Self::size_total()?;
        if self.offset.checked_add(len).is_none_or(|x| x > size_total) {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                format!("Failed to allocate memory of length: {len} bytes"),
            ));
        }

        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                self.fd.as_raw_fd(),
                self.offset as i64,
            )
        };

        if ptr.is_null() {
            return Err(io::Error::new(io::ErrorKind::Other, "Failed to map memory"));
        }

        let mmap = MmapMut::new(ptr, len);
        let phys_addr = Self::phys_addr()? + self.offset as u64;

        self.offset += len;

        Ok(DmaBuf::new(mmap, phys_addr))
    }
}

impl DmaBufAllocator for UDmaBufAllocator {
    fn alloc(&mut self, len: usize) -> io::Result<DmaBuf> {
        self.create(len)
    }
}

#[cfg(test)]
mod tests {
    use crate::mem::virt_to_phy::{AddressResolver, PhysAddrResolverLinuxX86};

    use super::*;
    use std::io::ErrorKind;

    #[test]
    fn allocate_pages() {
        let mut allocator = UDmaBufAllocator::open().unwrap();
        let mut x = allocator.create(0x1000).unwrap();
        let buf: &mut [u8] = x.as_mut();
        assert_eq!(buf.len(), 0x1000);
        buf.fill(1);
        let mut x = allocator.create(0x4000).unwrap();
        let buf: &mut [u8] = x.as_mut();
        assert_eq!(buf.len(), 0x4000);
        buf.fill(1);
    }
}
