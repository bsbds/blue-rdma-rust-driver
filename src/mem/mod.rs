/// Tools for converting virtual address to physicall address
mod virt_to_phy;

/// Page implementation
mod page;

/// Number of bits for a 2MB huge page size
pub(crate) const HUGE_PAGE_2MB_BITS: u8 = 21;
/// Size of a 2MB huge page in bytes
pub(crate) const HUGE_PAGE_2MB_SIZE: usize = 1 << 21;

/// Asserts that huge page support is enabled by checking if the system page size
/// matches the expected 2MB huge page size.
///
/// # Panics
///
/// Panics if the system page size does not equal `HUGE_PAGE_2MB_SIZE`.
#[allow(
    unsafe_code, // Safe because sysconf(_SC_PAGESIZE) is guaranteed to return a valid value.
    clippy::as_conversions,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn assert_huge_page_enabled() {
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
    assert_eq!(page_size, HUGE_PAGE_2MB_SIZE, "2MB huge pages not enabled");
}
