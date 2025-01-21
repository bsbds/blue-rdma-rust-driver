/// Maximum number of bits used to represent a PSN.
pub(crate) const MAX_PSN_SIZE_BITS: usize = 24;
/// Maximum size of the PSN window. This represents the maximum number outstanding PSNs.
pub(crate) const MAX_PSN_WINDOW: usize = 1 << (MAX_PSN_SIZE_BITS - 1);
/// Bit mask used to extract the PSN value from a 32-bit number.
pub(crate) const PSN_MASK: u32 = (1 << MAX_PSN_SIZE_BITS) - 1;

/// Maximum number of bits used to represent a MSN.
pub(crate) const MAX_MSN_SIZE_BITS: usize = 16;
/// Maximum size of the PSN window. This represents the maximum number outstanding PSNs.
pub(crate) const MAX_MSN_WINDOW: usize = 1 << (MAX_MSN_SIZE_BITS - 1);
