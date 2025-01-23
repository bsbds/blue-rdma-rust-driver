use super::{MrKeyIndex, MAX_MR_CNT};

/// First stage table allocator
///
/// Manages allocation and deallocation of memory region keys from a free list.
pub(super) struct MrTableAlloc {
    /// List of available memory region keys that can be allocated
    free_list: Vec<MrKeyIndex>,
}

impl MrTableAlloc {
    /// Creates a new `MrTableAlloc` instance with a pre-filled free list
    pub(super) fn new() -> Self {
        Self {
            free_list: Self::fill_up_free_list(),
        }
    }

    /// Allocates a new memory region key from the free list
    ///
    /// # Returns
    ///
    /// Returns None if the table is full
    pub(super) fn alloc_mr_key_idx(&mut self) -> Option<MrKeyIndex> {
        self.free_list.pop()
    }

    /// Returns a memory region key back to the free list
    pub(super) fn dealloc_mr_key(&mut self, key: MrKeyIndex) {
        self.free_list.push(key);
    }

    /// Creates initial free list containing all possible memory region keys
    fn fill_up_free_list() -> Vec<MrKeyIndex> {
        (0..u32::try_from(MAX_MR_CNT).unwrap_or_else(|_| unreachable!("invalid  MAX_MR_CNT")))
            .map(MrKeyIndex)
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mr_table_alloc_dealloc_ok() {
        let mut alloc = MrTableAlloc::new();
        let mr_keys: Vec<_> = std::iter::repeat_with(|| alloc.alloc_mr_key_idx())
            .take(MAX_MR_CNT)
            .flatten()
            .collect();
        assert_eq!(mr_keys.len(), MAX_MR_CNT);
        assert!(alloc.alloc_mr_key_idx().is_none());
        alloc.dealloc_mr_key(mr_keys[0]);
        alloc.alloc_mr_key_idx().unwrap();
    }
}
