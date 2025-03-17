use std::io;

use crate::mem::PAGE_SIZE;

/// Pins pages in memory to prevent swapping
///
/// # Errors
///
/// Returns an error if the pages could not be locked in memory
pub(crate) fn pin_pages(addr: u64, length: usize) -> io::Result<()> {
    let result = unsafe { libc::mlock(addr as *const std::ffi::c_void, length) };
    if result != 0 {
        return Err(io::Error::new(io::ErrorKind::Other, "failed to lock pages"));
    }
    Ok(())
}

/// Unpins pages
///
/// # Errors
///
/// Returns an error if the pages could not be locked in memory
pub(crate) fn unpin_pages(addr: u64, length: usize) -> io::Result<()> {
    let result = unsafe { libc::munlock(addr as *const std::ffi::c_void, length) };
    if result != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to unlock pages",
        ));
    }
    Ok(())
}

/// Calculates the number of pages spanned by a memory region.
#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn get_num_page(addr: u64, length: usize) -> usize {
    if length == 0 {
        return 0;
    }
    let last = addr.saturating_add(length as u64).saturating_sub(1);
    let start_page = addr / PAGE_SIZE as u64;
    let end_page = last / PAGE_SIZE as u64;
    (end_page.saturating_sub(start_page) + 1) as usize
}
