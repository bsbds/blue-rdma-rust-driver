use bitvec::{array::BitArray, bitarr};

use super::{PgtAlloc, PGT_LEN};

/// A simple page table allocator that uses a bit array to track free/used entries
pub(crate) struct SimplePgtAlloc {
    /// Bit array tracking which entries are free `false` or used `true`
    free_list: BitArray<[usize; PGT_LEN / 64]>,
}

impl SimplePgtAlloc {
    /// Creates a new empty `SimplePgtAlloc` with all entries marked as free
    pub(crate) fn new() -> Self {
        Self {
            free_list: bitarr![0; PGT_LEN],
        }
    }
}

impl PgtAlloc for SimplePgtAlloc {
    #[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // should never overflow
    fn alloc(&mut self, len: usize) -> Option<usize> {
        let mut count = 0;
        let mut start = 0;

        for i in 0..self.free_list.len() {
            if self.free_list[i] {
                count = 0;
            } else {
                if count == 0 {
                    start = i;
                }
                count += 1;
                if count == len {
                    self.free_list[start..start + len].fill(true);
                    return Some(start);
                }
            }
        }
        None
    }

    fn dealloc(&mut self, index: usize, len: usize) -> bool {
        let Some(end) = index.checked_add(len) else {
            return false;
        };
        if let Some(slice) = self.free_list.get_mut(index..end) {
            slice.fill(false);
            return true;
        }
        false
    }
}
