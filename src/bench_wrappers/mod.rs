#![allow(
    clippy::all,
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    missing_debug_implementations,
    missing_copy_implementations,
    clippy::pedantic,
    clippy::missing_inline_in_public_items,
    clippy::as_conversions,
    clippy::arithmetic_side_effects
)]

use std::io;

use crate::{
    mem::{
        page::ConscMem,
        slot_alloc::{RcSlot, SlotAlloc, SlotSize},
        virt_to_phy::{virt_to_phy, virt_to_phy_range},
    },
    ring::{Descriptor, Dummy, Ring, RingCtx, RING_BUF_LEN},
};

#[inline]
pub fn virt_to_phy_bench_wrapper<Vas>(virt_addrs: Vas) -> io::Result<Vec<Option<u64>>>
where
    Vas: IntoIterator<Item = *const u8>,
{
    virt_to_phy(virt_addrs)
}

#[inline]
pub fn virt_to_phy_bench_range_wrapper(
    start_addr: *const u8,
    num_pages: usize,
) -> io::Result<Vec<Option<u64>>> {
    virt_to_phy_range(start_addr, num_pages)
}

#[derive(Clone, Copy)]
pub struct BenchDesc {
    _inner: [u8; 32],
}

impl BenchDesc {
    pub fn new(data: [u8; 32]) -> Self {
        Self { _inner: data }
    }
}

impl Descriptor for BenchDesc {
    fn f_valid(&self) -> bool {
        true
    }

    fn size() -> usize {
        24
    }
}

type BenchBuf = RcSlot<ConscMem, BenchSlotSize>;

pub struct RingWrapper {
    inner: Ring<BenchBuf, Dummy, BenchDesc>,
}

impl RingWrapper {
    pub fn produce<Descs: Iterator<Item = BenchDesc>>(&mut self, descs: Descs) {
        self.inner.force_produce(descs.into_iter());
    }

    pub fn consume(&mut self) -> Option<&BenchDesc> {
        self.inner.try_consume()
    }
}

struct BenchSlotSize;

impl SlotSize for BenchSlotSize {
    fn size() -> usize {
        BenchDesc::size() * RING_BUF_LEN as usize
    }
}

#[allow(unsafe_code)]
impl AsMut<[BenchDesc]> for BenchBuf {
    fn as_mut(&mut self) -> &mut [BenchDesc] {
        unsafe { std::mem::transmute(AsMut::<[u8]>::as_mut(self)) }
    }
}

pub fn create_ring_wrapper() -> RingWrapper {
    let mem = ConscMem::new(1).unwrap();
    let mut alloc = SlotAlloc::<_, BenchSlotSize>::new(mem);
    let slot = alloc.alloc_one().unwrap();
    let ring_ctx = RingCtx::new(Dummy);
    let ring = Ring::<_, _, BenchDesc>::new(ring_ctx, slot).unwrap();
    RingWrapper { inner: ring }
}
