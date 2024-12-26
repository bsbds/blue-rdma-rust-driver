use super::MrKey;

/// Maximum number of entries in the first stage table
const MR_TABLE_LEN: u32 = 0x1000;

/// First stage table allocator
///
/// Manages allocation and deallocation of memory region keys from a free list.
pub(super) struct MrTableAlloc {
    /// List of available memory region keys that can be allocated
    free_list: Vec<MrKey>,
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
    pub(super) fn alloc_mr_key(&mut self) -> Option<MrKey> {
        self.free_list.pop()
    }

    /// Returns a memory region key back to the free list
    pub(super) fn dealloc_mr_key(&mut self, key: MrKey) {
        self.free_list.push(key);
    }

    /// Creates initial free list containing all possible memory region keys
    fn fill_up_free_list() -> Vec<MrKey> {
        (0..MR_TABLE_LEN).map(MrKey).collect()
    }
}
