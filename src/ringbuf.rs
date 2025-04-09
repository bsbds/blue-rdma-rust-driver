use crate::mem::page::MmapMut;

/// Number of bits used to represent the length of the ring buffer.
const RING_BUF_LEN_BITS: u8 = 12;
/// Highest bit of the ring buffer
pub(crate) const RING_BUF_LEN: usize = 1 << RING_BUF_LEN_BITS;
/// Mask used to calculate the length of the ring buffer
const RING_BUF_LEN_MASK: usize = (1 << RING_BUF_LEN_BITS) - 1;
/// Mask used to wrap indices around the ring buffer length.
/// Allows the highest bit to overflow for convenient wraparound.
const RING_BUF_LEN_WRAP_MASK: usize = (1 << (RING_BUF_LEN_BITS + 1)) - 1;

pub(crate) struct DmaRingBuf<T> {
    ptr: *mut T,
    head: usize,
    tail: usize,
}

#[allow(unsafe_code)]
impl<T: Copy> DmaRingBuf<T> {
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn new(buf: MmapMut) -> Self {
        assert!(buf.len >= RING_BUF_LEN * size_of::<T>(), "invalid length");
        Self {
            ptr: buf.ptr.cast(),
            head: 0,
            tail: 0,
        }
    }

    pub(crate) fn push(&mut self, value: T) -> bool {
        if self.len() == RING_BUF_LEN {
            return false;
        }
        unsafe {
            self.ptr.add(self.head).write_volatile(value);
        }

        self.inc_head();

        true
    }

    pub(crate) fn pop<F>(&mut self, cond: F) -> Option<T>
    where
        F: FnOnce(&T) -> bool,
    {
        let value = unsafe { self.ptr.add(self.tail).read_volatile() };
        if cond(&value) {
            unsafe {
                self.ptr.add(self.tail).write_volatile(std::mem::zeroed());
            }
            self.inc_tail();
            return Some(value);
        }

        None
    }

    /// Returns the current head index in the ring buffer
    pub(crate) fn head(&self) -> usize {
        self.head & RING_BUF_LEN_MASK
    }

    /// Returns the current tail index in the ring buffer
    pub(crate) fn tail(&self) -> usize {
        self.tail & RING_BUF_LEN_MASK
    }

    /// Returns the current length of data in the ring buffer
    pub(crate) fn len(&self) -> usize {
        let dlt = self.head.wrapping_sub(self.tail);
        dlt & RING_BUF_LEN_WRAP_MASK
    }

    pub(crate) fn remaining(&self) -> usize {
        RING_BUF_LEN - self.len()
    }

    /// Returns true if the ring buffer is empty
    pub(crate) fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    /// Returns true if the ring buffer is full
    pub(crate) fn is_full(&self) -> bool {
        self.head ^ self.tail == RING_BUF_LEN
    }

    /// Increments the head pointer of the ring buffer
    pub(crate) fn inc_head(&mut self) {
        self.head = self.head.wrapping_add(1) & RING_BUF_LEN_WRAP_MASK;
    }

    /// Increments the tail pointer of the ring buffer
    pub(crate) fn inc_tail(&mut self) {
        self.tail = self.tail.wrapping_add(1) & RING_BUF_LEN_WRAP_MASK;
    }

    pub(crate) fn set_tail(&mut self, tail: u32) {
        self.tail = tail as usize;
    }

    pub(crate) fn set_head(&mut self, head: u32) {
        self.head = head as usize;
    }
}

#[allow(unsafe_code)]
unsafe impl<T> Send for DmaRingBuf<T> {}
