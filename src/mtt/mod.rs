#![allow(missing_docs, clippy::missing_docs_in_private_items)]

use bitvec::{array::BitArray, bitarr};

/// Maximum number of entries in the first stage table
const MR_TABLE_LEN: u32 = 0x1000;

/// Maximum number of entries in the secodn stage table
const PGT_LEN: usize = 0x20000;

pub(crate) struct MrKey(u32);

pub(crate) struct IbvMr {
    addr: u64,
    length: u32,
    access: u32,
    mr_key: MrKey,
}

/// First stage table allocator
struct MrTableAlloc {
    free_list: Vec<MrKey>,
}

impl MrTableAlloc {
    fn new() -> Self {
        Self {
            free_list: Self::fill_up_free_list(),
        }
    }

    fn fill_up_free_list() -> Vec<MrKey> {
        (0..MR_TABLE_LEN).map(MrKey).collect()
    }

    fn alloc_mr_key(&mut self) -> Option<MrKey> {
        self.free_list.pop()
    }

    fn dealloc_mr_key(&mut self, key: MrKey) {
        self.free_list.push(key);
    }
}

trait PgtAlloc {
    fn alloc(&mut self, len: usize) -> Option<usize>;
    fn dealloc(&mut self, index: usize, len: usize) -> bool;
}

/// Second stage table allocator
struct SimplePgtAlloc {
    free_list: BitArray<[usize; PGT_LEN / 64]>,
}

impl SimplePgtAlloc {
    fn new() -> Self {
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
