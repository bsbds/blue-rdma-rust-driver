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
use thiserror::Error;

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
pub(crate) struct RingCtx {
    /// The head pointer
    head: u32,
    /// The tail pointer
    tail: u32,
}

impl RingCtx {
    /// Creates a new `RingCtx`
    pub(crate) fn new() -> Self {
        Self { head: 0, tail: 0 }
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
}

/// A trait for descriptors in the ring buffer
pub(crate) trait Descriptor {
    /// Size in bytes of the descriptor
    const SIZE: usize;

    /// Returns `true` if the descriptor's valid bit is set, indicating it contains valid data.
    /// If the valid bit is set, it should be cleared to 0 before returning.
    fn take_valid(&mut self) -> bool;
}

pub(crate) trait Flushable {
    fn flush(&self);
}

pub(crate) trait DescBuffer<Desc>: AsMut<[Desc]> + Flushable {}

impl<Desc> Flushable for Vec<Desc> {
    fn flush(&self) {}
}

impl<Desc> DescBuffer<Desc> for Vec<Desc> {}

/// A ring buffer for RDMA operations.
///
/// # Type Parameters
///
/// * `Buf` - The underlying buffer type
/// * `Dev` - The device type
/// * `Desc` - The descriptor type used for operations
pub(crate) struct RingBuffer<Buf, Desc> {
    /// Context of the ring buffer
    ctx: RingCtx,
    /// The underlying buffer
    buf: Buf,
    /// The descriptor type
    _marker: PhantomData<Desc>,
}

impl<Buf, Desc> RingBuffer<Buf, Desc>
where
    Buf: DescBuffer<Desc>,
    Desc: Descriptor,
{
    /// Creates a new `Ring`
    pub(crate) fn new(ctx: RingCtx, mut buf: Buf) -> Option<Self> {
        (buf.as_mut().len() >= RING_BUF_LEN as usize).then_some(Self {
            ctx,
            buf,
            _marker: PhantomData,
        })
    }

    /// Appends some descriptors to the ring buffer
    pub(crate) fn push(&mut self, desc: Desc) -> io::Result<()> {
        if self.ctx.len() == RING_BUF_LEN as usize {
            return Err(io::ErrorKind::WouldBlock.into());
        }

        let buf = self.buf.as_mut();
        buf[self.ctx.head_idx()] = desc;
        self.ctx.inc_head();
        self.buf.flush();

        Ok(())
    }

    /// Appends descriptors to the ring buffer without checking if it is full.
    ///
    /// # Safety
    ///
    /// Caller must ensure there is sufficient space in the ring buffer before calling.
    pub(crate) fn force_push(&mut self, desc: Desc) {
        let buf = self.buf.as_mut();
        buf[self.ctx.head_idx()] = desc;
        self.ctx.inc_head();
        self.buf.flush();
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<&Desc> {
        let buf = self.buf.as_mut();
        let tail = self.ctx.tail_idx();
        let ready = buf[tail].take_valid();
        ready.then(|| {
            self.ctx.inc_tail();
            &buf[tail]
        })
    }

    /// Returns the head pointer
    pub(crate) fn head(&self) -> u32 {
        self.ctx.head
    }

    /// Returns the tail pointer
    pub(crate) fn tail(&self) -> u32 {
        self.ctx.tail
    }

    pub(crate) fn set_tail(&mut self, tail: u32) {
        self.ctx.tail = tail;
    }
}

impl<Buf, Desc> RingBuffer<Buf, Desc>
where
    Buf: AsRef<[Desc]>,
{
    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.buf.as_ref().as_ptr() as u64
    }
}
