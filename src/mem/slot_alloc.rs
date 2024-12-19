use std::{fmt, io, marker::PhantomData, sync::Arc};

use std::ops::{Deref, DerefMut};

/// Trait for types that can specify their size requirements for memory slots.
pub(crate) trait SlotSize {
    /// Returns the size in bytes required for this slot type.
    fn size() -> usize;
}

/// A reference-counted memory slot.
pub(crate) struct RcSlot<Mem, SlotT> {
    /// Raw memory chunk managed by this slot
    inner: &'static mut [u8],
    /// Reference counted memory object
    rc: Arc<Mem>,
    /// Phantom data to carry the Slot type parameter
    _marker: PhantomData<SlotT>,
}

impl<Mem, SlotT> RcSlot<Mem, SlotT> {
    /// Creates a new `RcSlot`
    fn new(inner: &'static mut [u8], rc: Arc<Mem>) -> Self {
        Self {
            inner,
            rc,
            _marker: PhantomData,
        }
    }
}

impl<Mem, SlotT> Deref for RcSlot<Mem, SlotT> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.inner
    }
}

impl<Mem, SlotT> DerefMut for RcSlot<Mem, SlotT> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        self.inner
    }
}

impl<Mem, SlotT> AsRef<[u8]> for RcSlot<Mem, SlotT> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.inner
    }
}

impl<Mem, SlotT> AsMut<[u8]> for RcSlot<Mem, SlotT> {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.inner
    }
}

impl<Mem, SlotT> fmt::Debug for RcSlot<Mem, SlotT> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("RcSlot")
            .field("inner", &self.inner)
            .finish()
    }
}

/// A fixed-size slot allocator that manages memory slots within a consecutive memory region.
pub(crate) struct SlotAlloc<Mem, SlotT> {
    /// Reference counted memory object
    rc: Arc<Mem>,
    /// Free slots to allocate
    slots: Vec<RcSlot<Mem, SlotT>>,
    /// Original length of `Mem`
    len: usize,
}

#[allow(clippy::arithmetic_side_effects)]
impl<Mem, SlotT> SlotAlloc<Mem, SlotT>
where
    Mem: AsMut<[u8]> + 'static,
    SlotT: SlotSize,
{
    /// Creates a new slot allocator with the given consecutive memory region.
    pub(crate) fn new(mut mem: Mem) -> Self {
        // SAFETY: Transmuting to `'static mut [u8]` is safe because:
        // 1. The `Mem` requires `AsMut<[u8]> + 'static` to ensure exclusive access.
        // 2. The memory is kept alive by the reference count stored in `RcSlot`.
        // 3. The fields in RcSlot are private and won't be moved out through provided
        //    APIs.
        #[allow(unsafe_code)]
        unsafe {
            let mem_mut = mem.as_mut();
            let len = mem_mut.len();
            assert!(SlotT::size() <= len, "invalid slot size");
            let chunks: Vec<&'static mut [u8]> = mem
                .as_mut()
                .chunks_exact_mut(Self::slot_size())
                .map(|chunk| std::mem::transmute(chunk))
                .collect();
            let mem_arc = Arc::new(mem);
            let slots = chunks
                .into_iter()
                .zip(std::iter::repeat(Arc::clone(&mem_arc)))
                .map(|(chunk, rc)| RcSlot::new(chunk, rc))
                .collect();

            Self {
                rc: mem_arc,
                slots,
                len,
            }
        }
    }

    /// Allocates a new memory slot if available.
    ///
    /// # Returns
    ///
    /// Returns None if no slots are available.
    pub(crate) fn alloc_one(&mut self) -> Option<RcSlot<Mem, SlotT>> {
        self.slots.pop()
    }

    /// Deallocates a previously allocated memory slot.
    ///
    /// # Returns
    ///
    /// Returns `None` if the slot was successfully deallocated, or `Some(slot)` if the slot belongs to a different allocator.
    pub(crate) fn dealloc(&mut self, slot: RcSlot<Mem, SlotT>) -> Option<RcSlot<Mem, SlotT>> {
        if Arc::ptr_eq(&self.rc, &slot.rc) {
            self.slots.push(slot);
            None
        } else {
            Some(slot)
        }
    }

    /// Returns true if there are no free slots available.
    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    //
    /// Returns the total number of slots that can be allocated.
    pub(crate) fn num_slots_total(&self) -> usize {
        self.len / Self::slot_size()
    }

    /// Returns the maximum slot number that can be allocated.
    pub(crate) fn slot_num_max(&self) -> usize {
        self.num_slots_total().saturating_sub(1)
    }

    /// Returns the size of each slot in bytes.
    pub(crate) fn slot_size() -> usize {
        assert!(SlotT::size() != 0, "invalid slot size");
        SlotT::size()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn slot_alloc_dealloc_ok() {
        struct Slot;
        impl SlotSize for Slot {
            fn size() -> usize {
                16
            }
        }
        let mut mem = [0u8; 1024];
        let mut alloc = SlotAlloc::<_, Slot>::new(mem);
        let slot_size = SlotAlloc::<&mut [u8], Slot>::slot_size();
        let total = alloc.num_slots_total();
        assert_eq!(slot_size, 16);
        assert_eq!(total, 1024 / 16);
        assert!(!alloc.is_empty());
        let slot = alloc.alloc_one().unwrap();
        let slot1 = alloc.alloc_one().unwrap();
        //assert!(alloc.dealloc(slot));
    }
}
