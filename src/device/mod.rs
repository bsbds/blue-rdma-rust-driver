#![allow(missing_docs, clippy::missing_docs_in_private_items)]

/// Emulated device adaptor
pub(crate) mod emulated;

/// Memory-mapped I/O addresses of device registers
mod constants;

use std::io;

/// A trait for interacting with device hardware through CSR operations.
pub(crate) trait DeviceAdaptor {
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
    const HEAD: usize;
    /// Memory address of the tail pointer register
    const TAIL: usize;
    /// Memory address of the low 32 bits of the base address register
    const BASE_ADDR_LOW: usize;
    /// Memory address of the high 32 bits of the base address register
    const BASE_ADDR_HIGH: usize;
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
    T: ToCard + DeviceAdaptor + RingBufferCsrAddr,
{
    fn write_head(&self, data: u32) -> io::Result<()> {
        self.write_csr(T::HEAD, data)
    }

    fn read_tail(&self) -> io::Result<u32> {
        self.read_csr(T::TAIL)
    }
}

impl<T> CsrReaderAdaptor for T
where
    T: ToHost + DeviceAdaptor + RingBufferCsrAddr,
{
    fn write_tail(&self, data: u32) -> io::Result<()> {
        self.write_csr(Self::TAIL, data)
    }

    fn read_head(&self) -> io::Result<u32> {
        self.read_csr(Self::HEAD)
    }
}

impl<T> CsrBaseAddrAdaptor for T
where
    T: DeviceAdaptor + RingBufferCsrAddr,
{
    #[allow(clippy::arithmetic_side_effects)]
    fn read_base_addr(&self) -> io::Result<u64> {
        let lo = self.read_csr(Self::BASE_ADDR_LOW)?;
        let hi = self.read_csr(Self::BASE_ADDR_HIGH)?;
        Ok(u64::from(lo) + (u64::from(hi) << 32))
    }

    #[allow(clippy::as_conversions)]
    fn write_base_addr(&self, phys_addr: u64) -> io::Result<()> {
        self.write_csr(Self::BASE_ADDR_LOW, (phys_addr & 0xFFFF_FFFF) as u32)?;
        self.write_csr(Self::BASE_ADDR_HIGH, (phys_addr >> 32) as u32)
    }
}
