use memmap2::{MmapMut, MmapOptions};
use parking_lot::Mutex;
use pci_driver::{
    backends::vfio::VfioPciDevice,
    device::PciDevice,
    regions::{MappedOwningPciRegion, OwningPciRegion, PciRegion, Permissions},
};
use pci_info::PciInfo;
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::mem::{page::HostPageAllocator, virt_to_phy::PhysAddrResolverLinuxX86};

use super::{ops_impl::HwDevice, DeviceAdaptor};

const BAR_INDEX: usize = 0;
const BAR_INDEX_DMA_ENGINE: usize = 1;
const BAR_MAP_RANGE_END: u64 = 4096;
const VENDER_ID: u16 = 0x10ee;
const DEVICE_ID: u16 = 0x903f;
const PCI_SYSFS_BUS_PATH: &str = "/sys/bus/pci/devices";

#[derive(Clone, Debug)]
pub(crate) struct VfioPciCsrAdaptor {
    bar: Arc<MappedOwningPciRegion>,
}

impl VfioPciCsrAdaptor {
    fn new(sysfs_path: impl AsRef<Path>) -> io::Result<Self> {
        let path = sysfs_path.as_ref();
        let device = VfioPciDevice::open(path).map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to open sysfs_path: {err}"),
            )
        })?;
        let bar = device.bar(BAR_INDEX).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Expected device to have BAR")
        })?;
        let mapped_bar = bar.map(..BAR_MAP_RANGE_END, Permissions::ReadWrite)?;
        Ok(Self {
            bar: Arc::new(mapped_bar),
        })
    }
}

// TODO: use u64 instead of usize
impl DeviceAdaptor for VfioPciCsrAdaptor {
    fn read_csr(&self, addr: usize) -> io::Result<u32> {
        self.bar.read_le_u32(addr as u64)
    }

    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        self.bar.write_le_u32(addr as u64, data)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SysfsPciCsrAdaptor {
    bar: Arc<Mutex<MmapMut>>,
}

#[allow(unsafe_code)]
impl SysfsPciCsrAdaptor {
    fn new(sysfs_path: impl AsRef<Path>) -> io::Result<Self> {
        let bar_path = sysfs_path.as_ref().join(format!("resource{BAR_INDEX}"));
        let file = OpenOptions::new().read(true).write(true).open(&bar_path)?;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        Ok(Self {
            bar: Arc::new(Mutex::new(mmap)),
        })
    }
}

#[allow(unsafe_code, clippy::cast_ptr_alignment)]
impl DeviceAdaptor for SysfsPciCsrAdaptor {
    fn read_csr(&self, addr: usize) -> io::Result<u32> {
        if addr % 4 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unaligned access",
            ));
        }

        let bar = self.bar.lock();
        unsafe {
            let ptr = bar.as_ptr().add(addr);
            Ok(ptr.cast::<u32>().read_volatile())
        }
    }

    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        if addr % 4 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unaligned access",
            ));
        }

        let mut bar = self.bar.lock();
        unsafe {
            let ptr = bar.as_mut_ptr().add(addr);
            ptr.cast::<u32>().write_volatile(data);
        }

        Ok(())
    }
}

pub(crate) struct PciHwDevice {
    sysfs_path: PathBuf,
}

impl PciHwDevice {
    pub(crate) fn new(sysfs_path: impl AsRef<Path>) -> Self {
        Self {
            sysfs_path: sysfs_path.as_ref().into(),
        }
    }

    pub(crate) fn open_default() -> io::Result<Self> {
        let build_err = || io::Error::new(io::ErrorKind::Other, "Failed to open device");
        let info = PciInfo::enumerate_pci().map_err(|_err| build_err())?;
        let device = info
            .iter()
            .flatten()
            .find(|d| d.vendor_id() == VENDER_ID && d.device_id() == DEVICE_ID)
            .ok_or_else(build_err)?;
        let location = device.location().map_err(|_err| build_err())?;
        let sysfs_path = PathBuf::from(PCI_SYSFS_BUS_PATH).join(location.to_string());

        Ok(Self { sysfs_path })
    }

    pub(crate) fn reset(&self) -> io::Result<()> {
        let path = self.sysfs_path.join("reset");
        fs::write(path, "1")
    }

    pub(crate) fn init_dma_engine(&self) -> io::Result<()> {
        DmaEngineConfigurator::new(&self.sysfs_path)?.configure();

        Ok(())
    }

    pub(crate) fn set_custom(&self) -> io::Result<()> {
        let mut cfg = CustomCsrConfigurator::new(&self.sysfs_path)?;
        cfg.set_loopback();
        cfg.set_drop_thresh(1);
        cfg.set_seed(0x3131_3131);

        Ok(())
    }
}

impl HwDevice for PciHwDevice {
    type Adaptor = SysfsPciCsrAdaptor;

    type PageAllocator = HostPageAllocator<1>;

    type PhysAddrResolver = PhysAddrResolverLinuxX86;

    fn new_adaptor(&self) -> io::Result<Self::Adaptor> {
        SysfsPciCsrAdaptor::new(&self.sysfs_path)
    }

    fn new_page_allocator(&self) -> Self::PageAllocator {
        HostPageAllocator
    }

    fn new_phys_addr_resolver(&self) -> Self::PhysAddrResolver {
        PhysAddrResolverLinuxX86
    }
}

pub(crate) struct DmaEngineConfigurator {
    bar: MmapMut,
}

#[allow(unsafe_code, clippy::cast_ptr_alignment)]
impl DmaEngineConfigurator {
    pub(crate) fn new(sysfs_path: impl AsRef<Path>) -> io::Result<Self> {
        let bar_path = sysfs_path
            .as_ref()
            .join(format!("resource{BAR_INDEX_DMA_ENGINE}"));
        let file = OpenOptions::new().read(true).write(true).open(&bar_path)?;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        Ok(Self { bar: mmap })
    }

    pub(crate) fn configure(&mut self) {
        const ADDR0: usize = 0x0004;
        const ADDR1: usize = 0x1004;
        unsafe {
            self.bar
                .as_mut_ptr()
                .add(ADDR0)
                .cast::<u32>()
                .write_volatile(1);
            self.bar
                .as_mut_ptr()
                .add(ADDR1)
                .cast::<u32>()
                .write_volatile(1);
        }
    }
}

pub(crate) struct CustomCsrConfigurator {
    bar: MmapMut,
}

#[allow(unsafe_code, clippy::cast_ptr_alignment)]
impl CustomCsrConfigurator {
    pub(crate) fn new(sysfs_path: impl AsRef<Path>) -> io::Result<Self> {
        let bar_path = sysfs_path.as_ref().join(format!("resource{BAR_INDEX}"));
        let file = OpenOptions::new().read(true).write(true).open(&bar_path)?;
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        Ok(Self { bar: mmap })
    }

    pub(crate) fn set_loopback(&mut self) {
        const ADDR: usize = 0x180;
        unsafe {
            self.bar
                .as_mut_ptr()
                .add(ADDR)
                .cast::<u32>()
                .write_volatile(1);
        }
    }

    pub(crate) fn set_seed(&mut self, seed: u32) {
        const ADDR: usize = 0x184;
        unsafe {
            self.bar
                .as_mut_ptr()
                .add(ADDR)
                .cast::<u32>()
                .write_volatile(seed);
        }
    }

    pub(crate) fn set_drop_thresh(&mut self, rate: u8) {
        const ADDR: usize = 0x188;
        unsafe {
            self.bar
                .as_mut_ptr()
                .add(ADDR)
                .cast::<u32>()
                .write_volatile(u32::from(rate));
        }
    }
}
