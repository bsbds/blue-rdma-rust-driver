use std::io;

use super::DeviceProxy;

/// A trait for interacting with device hardware through CSR operations.
pub(crate) trait DeviceAdaptor: Clone {
    /// Reads from a CSR at the specified address.
    ///
    /// # Arguments
    ///
    /// * `addr` - The 64-bit memory address of the CSR to read from
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful read, or an error if the read operation fails
    fn read_csr(&self, addr: usize) -> io::Result<u32>;

    /// Writes data to a Control and Status Register at the specified address.
    ///
    /// # Arguments
    ///
    /// * `addr` - The 64-bit memory address of the CSR to write to
    /// * `data` - The 32-bit data value to write to the register
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful write, or an error if the write operation fails
    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()>;
}

/// Trait for types that have ring buffer CSR addresses
pub(crate) trait RingBufferCsrAddr {
    /// Memory address of the head pointer register
    fn head(&self) -> usize;
    /// Memory address of the tail pointer register
    fn tail(&self) -> usize;
    /// Memory address of the low 32 bits of the base address register
    fn base_addr_low(&self) -> usize;
    /// Memory address of the high 32 bits of the base address register
    fn base_addr_high(&self) -> usize;
}

/// Marker trait for ring buffers that transfer data from host to card
pub(crate) trait ToCard {}

/// Marker trait for ring buffers that transfer data from card to host
pub(crate) trait ToHost {}

/// An adaptor to read the tail pointer and write the head pointer, using by writer.
pub(crate) trait CsrWriterAdaptor {
    /// Write the head pointer value
    fn write_head(&self, data: u32) -> io::Result<()>;
    /// Read the tail pointer value
    fn read_tail(&self) -> io::Result<u32>;
}

/// An adaptor to read the head pointer and write the tail pointer, using by reader.
pub(crate) trait CsrReaderAdaptor {
    /// Write the tail pointer value
    fn write_tail(&self, data: u32) -> io::Result<()>;
    /// Read the head pointer value
    fn read_head(&self) -> io::Result<u32>;
}

/// An adaptor to setup the base address of the ring buffer
pub(crate) trait CsrBaseAddrAdaptor {
    /// Read the base physical address of the ring buffer
    fn read_base_addr(&self) -> io::Result<u64>;
    /// Write the base physical address of the ring buffer
    fn write_base_addr(&self, phys_addr: u64) -> io::Result<()>;
}

impl<T> CsrWriterAdaptor for T
where
    T: DeviceProxy + ToCard + RingBufferCsrAddr,
    <T as DeviceProxy>::Device: DeviceAdaptor,
{
    fn write_head(&self, data: u32) -> io::Result<()> {
        self.device().write_csr(self.head(), data)
    }

    fn read_tail(&self) -> io::Result<u32> {
        self.device().read_csr(self.tail())
    }
}

impl<T> CsrReaderAdaptor for T
where
    T: DeviceProxy + ToHost + RingBufferCsrAddr,
    <T as DeviceProxy>::Device: DeviceAdaptor,
{
    fn write_tail(&self, data: u32) -> io::Result<()> {
        self.device().write_csr(self.tail(), data)
    }

    fn read_head(&self) -> io::Result<u32> {
        self.device().read_csr(self.head())
    }
}

impl<T> CsrBaseAddrAdaptor for T
where
    T: DeviceProxy + RingBufferCsrAddr,
    <T as DeviceProxy>::Device: DeviceAdaptor,
{
    #[allow(clippy::arithmetic_side_effects)]
    fn read_base_addr(&self) -> io::Result<u64> {
        let lo = self.device().read_csr(self.base_addr_low())?;
        let hi = self.device().read_csr(self.base_addr_high())?;
        Ok(u64::from(lo) + (u64::from(hi) << 32))
    }

    #[allow(clippy::as_conversions)]
    fn write_base_addr(&self, phys_addr: u64) -> io::Result<()> {
        self.device()
            .write_csr(self.base_addr_low(), (phys_addr & 0xFFFF_FFFF) as u32)?;
        self.device()
            .write_csr(self.base_addr_high(), (phys_addr >> 32) as u32)
    }
}
