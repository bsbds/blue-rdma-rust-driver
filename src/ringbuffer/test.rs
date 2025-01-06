use std::iter;

use crate::mem::{page::ConscMem, slot_alloc::SlotAlloc};

use super::*;

pub(crate) fn new_test_ring<Desc: Default + Clone + Descriptor>() -> RingBuffer<Vec<Desc>, Desc> {
    let slot = vec![Desc::default(); 128];
    let ring_ctx = RingCtx::new();
    RingBuffer::<_, Desc>::new(ring_ctx, slot).unwrap()
}

#[derive(Default, Clone, Copy)]
struct TestDesc {
    inner: [u8; 32],
}

impl TestDesc {
    fn new_valid() -> Self {
        Self { inner: [1; 32] }
    }
}

impl Descriptor for TestDesc {
    const SIZE: usize = 32;

    fn take_valid(&mut self) -> bool {
        let valid = self.inner[0] == 1;
        self.inner[0] = 0;
        valid
    }
}

#[test]
fn ring_produce_consume_is_ok() {
    let slot = vec![TestDesc::default(); 128];
    let ring_ctx = RingCtx::new();
    let mut ring = RingBuffer::<_, TestDesc>::new(ring_ctx, slot).unwrap();
    let round = 10;
    for _ in 0..round {
        for i in 0..128 {
            ring.push(iter::once(TestDesc::new_valid())).unwrap();
        }
        assert!(ring.push(iter::once(TestDesc::new_valid())).is_err());
        for i in 0..128 {
            assert!(ring.try_pop().is_some());
        }
        assert!(ring.try_pop().is_none());
    }
}

#[test]
fn build_ring_buffer_should_reject_insufficient_buf_size() {
    let slot = vec![TestDesc::default(); 127];
    let ring_ctx = RingCtx::new();
    assert!(RingBuffer::<_, TestDesc>::new(ring_ctx, slot).is_none());
}
