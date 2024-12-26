/// Maximum number of entries in the secodn stage table
pub(super) const PGT_LEN: usize = 0x20000;

/// Trait for allocating and deallocating second stage table entries
pub(super) trait PgtAlloc {
    /// Allocates a contiguous range of page table entries
    ///
    /// # Arguments
    ///
    /// * `len` - Number of contiguous entries to allocate
    ///
    /// # Returns
    ///
    /// * `Some(index)` - Starting index of allocated range if successful
    /// * `None` - If allocation failed
    fn alloc(&mut self, len: usize) -> Option<usize>;

    /// Deallocates a previously allocated range of page table entries
    ///
    /// # Arguments
    ///
    /// * `index` - Starting index of range to deallocate
    /// * `len` - Number of entries to deallocate
    ///
    /// # Returns
    ///
    /// * `true` - If deallocation was successful
    /// * `false` - If deallocation failed (e.g. invalid range)
    fn dealloc(&mut self, index: usize, len: usize) -> bool;
}

/// Simple allocator
pub(super) mod simple;
