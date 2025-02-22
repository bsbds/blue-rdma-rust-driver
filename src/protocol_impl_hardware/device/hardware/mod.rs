use pci_driver::{
    backends::vfio::VfioPciDevice,
    device::PciDevice,
    regions::{MappedOwningPciRegion, OwningPciRegion, PciRegion, Permissions},
};
use pci_info::PciInfo;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::mem::{page::HostPageAllocator, virt_to_phy::PhysAddrResolverLinuxX86};

use super::{ops_impl::HwDevice, DeviceAdaptor};

const BAR_INDEX: usize = 0;
const BAR_MAP_RANGE_END: u64 = 4096;
const VENDER_ID: u16 = 0x10ee;
const DEVICE_ID: u16 = 0x903f;
const PCI_SYSFS_BUS_PATH: &str = "/sys/bus/pci/devices";

#[derive(Clone, Debug)]
pub(crate) struct PciCsrAdaptor {
    bar: Arc<MappedOwningPciRegion>,
}

impl PciCsrAdaptor {
    fn new(sysfs_path: impl AsRef<Path>) -> io::Result<Self> {
        let device =
            VfioPciDevice::open(sysfs_path).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let bar = device.bar(BAR_INDEX).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Expected device to have BAR")
        })?;
        let mapped_bar = bar.map(..BAR_MAP_RANGE_END, Permissions::ReadWrite)?;
        Ok(Self {
            bar: Arc::new(mapped_bar),
        })
    }

    fn read_csr(&self, addr: u64) -> io::Result<u32> {
        self.bar.read_le_u32(addr)
    }

    fn write_csr(&self, addr: u64, data: u32) -> io::Result<()> {
        self.bar.write_le_u32(addr, data)
    }
}

// TODO: use u64 instead of usize
impl DeviceAdaptor for PciCsrAdaptor {
    fn read_csr(&self, addr: usize) -> io::Result<u32> {
        self.read_csr(addr as u64)
    }

    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        self.write_csr(addr as u64, data)
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
        let bus_number = location.bus_number();
        let bus_id = format!("{:04x}:{:02x}", bus_number.segment(), bus_number.bus());
        let sysfs_path = PathBuf::from(PCI_SYSFS_BUS_PATH).join(bus_id);

        Ok(Self { sysfs_path })
    }
}

impl HwDevice for PciHwDevice {
    type Adaptor = PciCsrAdaptor;

    type PageAllocator = HostPageAllocator<1>;

    type PhysAddrResolver = PhysAddrResolverLinuxX86;

    fn new_adaptor(&self) -> io::Result<Self::Adaptor> {
        PciCsrAdaptor::new(&self.sysfs_path)
    }

    fn new_page_allocator(&self) -> Self::PageAllocator {
        HostPageAllocator
    }

    fn new_phys_addr_resolver(&self) -> Self::PhysAddrResolver {
        PhysAddrResolverLinuxX86
    }
}
