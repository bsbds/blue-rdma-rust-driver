#![allow(
    clippy::as_conversions,     // safe to converts u32 to usize
    clippy::indexing_slicing    // panic is expected behaviour
)]
#[cfg(test)]
mod test;

use std::{io, marker::PhantomData, ops::Deref};

use crate::mem::slot_alloc::RcSlot;

#[cfg(test)]
pub(crate) use test::new_test_ring;

/// A trait for devices that require synchronization of head and tail pointers.
pub(crate) trait SyncDevice {
    /// Synchronizes the head pointer of the device.
    ///
    /// # Errors
    ///
    /// Returns an error if the synchronization fails.
    fn sync_head_ptr(&self, value: u32) -> io::Result<()>;

    /// Synchronizes the tail pointer of the device.
    ///
    /// # Errors
    ///
    /// Returns an error if the synchronization fails.
    fn sync_tail_ptr(&self, value: u32) -> io::Result<()>;
}

/// Card device type
pub(crate) struct Card {
    /// Physical address of the ring buffer.
    pa: u64,
    /// Memory-mapped address of the head register
    head_remote_addr: u64,
    /// Memory-mapped address of the tail register
    tail_remote_addr: u64,
}

impl Card {
    /// Creates a new `Card`
    pub(crate) fn new(pa: u64, head_remote_addr: u64, tail_remote_addr: u64) -> Self {
        Self {
            pa,
            head_remote_addr,
            tail_remote_addr,
        }
    }

    /// Writes the head value to the remote address
    fn write_head(&self, value: u32) {
        Self::write_addr(self.head_remote_addr, value);
    }

    /// Writes the tail value to the remote address
    fn write_tail(&self, value: u32) {
        Self::write_addr(self.tail_remote_addr, value);
    }

    /// Writes a 32-bit value to the specified memory address
    #[allow(unsafe_code)]
    fn write_addr(addr: u64, value: u32) {
        unsafe {
            std::ptr::write_volatile(addr as *mut u32, value);
        }
    }
}

/// Mock device type that uses RPC for communication
pub(crate) struct Mock<Rpc> {
    /// Rpc object for interacting with the mock device
    rpc: Rpc,
}

impl<Rpc> Mock<Rpc> {
    /// Creates a new `Mock`
    pub(crate) fn new(rpc: Rpc) -> Self {
        Self { rpc }
    }
}

/// A dummy device type.
pub(crate) struct Dummy;

impl SyncDevice for Card {
    fn sync_head_ptr(&self, value: u32) -> io::Result<()> {
        self.write_head(value);

        Ok(())
    }

    fn sync_tail_ptr(&self, value: u32) -> io::Result<()> {
        self.write_tail(value);

        Ok(())
    }
}

// TODO: implement RPCs
impl<Rpc> SyncDevice for Mock<Rpc> {
    fn sync_head_ptr(&self, _value: u32) -> io::Result<()> {
        Ok(())
    }

    fn sync_tail_ptr(&self, _value: u32) -> io::Result<()> {
        Ok(())
    }
}

impl SyncDevice for Dummy {
    fn sync_head_ptr(&self, _value: u32) -> io::Result<()> {
        Ok(())
    }

    fn sync_tail_ptr(&self, _value: u32) -> io::Result<()> {
        Ok(())
    }
}

/// Number of bits used to represent the length of the ring buffer.
const RING_BUF_LEN_BITS: u8 = 7;
/// Highest bit of the ring buffer
pub(crate) const RING_BUF_LEN: u32 = 1 << RING_BUF_LEN_BITS;
/// Mask used to calculate the length of the ring buffer
const RING_BUF_LEN_MASK: u32 = (1 << RING_BUF_LEN_BITS) - 1;
/// Mask used to wrap indices around the ring buffer length.
/// Allows the highest bit to overflow for convenient wraparound.
const RING_BUF_LEN_WRAP_MASK: u32 = (1 << (RING_BUF_LEN_BITS + 1)) - 1;

/// Context of a ring buffer.
///
/// For head/tails porinter, pack guard (1 bit) and idx (31 bits) into a single u32.
pub(crate) struct RingCtx<Dev> {
    /// The head pointer
    head: u32,
    /// The tail pointer
    tail: u32,
    /// Device specific operations
    dev: Dev,
}

impl<Dev> RingCtx<Dev> {
    /// Creates a new `RingCtx`
    pub(crate) fn new(dev: Dev) -> Self {
        Self {
            head: 0,
            tail: 0,
            dev,
        }
    }

    /// Returns the current head index in the ring buffer
    fn head_idx(&self) -> usize {
        (self.head & RING_BUF_LEN_MASK) as usize
    }

    /// Returns the current tail index in the ring buffer
    fn tail_idx(&self) -> usize {
        (self.tail & RING_BUF_LEN_MASK) as usize
    }

    /// Returns the current length of data in the ring buffer
    fn len(&self) -> usize {
        let dlt = self.head.wrapping_sub(self.tail);
        (dlt & RING_BUF_LEN_WRAP_MASK) as usize
    }

    /// Returns true if the ring buffer is empty
    fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    /// Returns true if the ring buffer is full
    fn is_full(&self) -> bool {
        self.head ^ self.tail == RING_BUF_LEN
    }

    /// Increments the head pointer of the ring buffer
    fn inc_head(&mut self) {
        self.head = self.head.wrapping_add(1) & RING_BUF_LEN_WRAP_MASK;
    }

    /// Increments the tail pointer of the ring buffer
    fn inc_tail(&mut self) {
        self.tail = self.tail.wrapping_add(1) & RING_BUF_LEN_WRAP_MASK;
    }

    /// Returns a reference to the associated device
    fn dev(&self) -> &Dev {
        &self.dev
    }
}

impl<Dev: SyncDevice> RingCtx<Dev> {
    /// Synchronizes the head pointer of the device.
    ///
    /// # Errors
    ///
    /// Returns an error if the synchronization fails.
    fn sync_head_ptr(&self) -> io::Result<()> {
        self.dev.sync_head_ptr(self.head)
    }

    /// Synchronizes the tail pointer of the device.
    ///
    /// # Errors
    ///
    /// Returns an error if the synchronization fails.
    fn sync_tail_ptr(&self) -> io::Result<()> {
        self.dev.sync_head_ptr(self.head)
    }
}

/// A trait for descriptors in the ring buffer
pub(crate) trait Descriptor {
    /// Size in bytes of the descriptor
    const SIZE: usize;

    /// Returns `true` if the descriptor's valid bit is set, indicating it contains valid data.
    /// If the valid bit is set, it should be cleared to 0 before returning.
    fn try_consume(&mut self) -> bool;
}

/// A ring buffer for RDMA operations.
///
/// # Type Parameters
///
/// * `Buf` - The underlying buffer type
/// * `Dev` - The device type
/// * `Desc` - The descriptor type used for operations
pub(crate) struct RingBuffer<Buf, Dev, Desc> {
    /// Context of the ring buffer
    ctx: RingCtx<Dev>,
    /// The underlying buffer
    buf: Buf,
    /// The descriptor type
    _marker: PhantomData<Desc>,
}

impl<Buf, Dev, Desc> RingBuffer<Buf, Dev, Desc>
where
    Buf: AsMut<[Desc]>,
    Dev: SyncDevice,
    Desc: Descriptor,
{
    /// Creates a new `Ring`
    pub(crate) fn new(ctx: RingCtx<Dev>, mut buf: Buf) -> Option<Self> {
        (buf.as_mut().len() >= RING_BUF_LEN as usize).then_some(Self {
            ctx,
            buf,
            _marker: PhantomData,
        })
    }

    /// Appends some descriptors to the ring buffer
    pub(crate) fn push<Descs: ExactSizeIterator<Item = Desc>>(
        &mut self,
        descs: Descs,
    ) -> io::Result<()> {
        if descs
            .len()
            .checked_add(self.ctx.len())
            .is_none_or(|len| len > RING_BUF_LEN as usize)
        {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let buf = self.buf.as_mut();
        for entry in descs {
            buf[self.ctx.head_idx()] = entry;
            self.ctx.inc_head();
        }

        Ok(())
    }

    /// Appends descriptors to the ring buffer without checking if it is full.
    ///
    /// # Safety
    ///
    /// Caller must ensure there is sufficient space in the ring buffer before calling.
    pub(crate) fn force_push<Descs: Iterator<Item = Desc>>(&mut self, descs: Descs) {
        let buf = self.buf.as_mut();
        for entry in descs {
            buf[self.ctx.head_idx()] = entry;
            self.ctx.inc_head();
        }
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<&Desc> {
        let buf = self.buf.as_mut();
        let tail = self.ctx.tail_idx();
        let ready = buf[tail].try_consume();
        ready.then(|| {
            self.ctx.inc_tail();
            &buf[tail]
        })
    }

    /// Flushes any pending produce operations by synchronizing the head pointer.
    pub(crate) fn flush_push(&self) -> io::Result<()> {
        self.ctx.sync_head_ptr()
    }

    /// Flushes any pending consume operations by synchronizing the tail pointer.
    pub(crate) fn flush_pop(&self) {
        self.ctx.sync_tail_ptr();
    }
}
