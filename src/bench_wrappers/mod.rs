#![allow(missing_docs, clippy::missing_errors_doc)]

use std::io;

use crate::mem::virt_to_phy::virt_to_phy;

#[inline]
pub fn virt_to_phy_bench_wrapper<Vas>(virt_addrs: Vas) -> io::Result<Vec<Option<u64>>>
where
    Vas: IntoIterator<Item = *const u8>,
{
    virt_to_phy(virt_addrs)
}
