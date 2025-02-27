use std::io;

use crate::{
    mem::virt_to_phy::{AddressResolver, PhysAddrResolverLinuxX86},
    protocol_impl::desc::{RingBufDescUntyped, SendQueueReqDescSeg0, SendQueueReqDescSeg1},
};

use super::DescRingBuffer;

/// Send queue descriptor types that can be submitted
#[derive(Debug, Clone, Copy)]
pub(crate) enum SendQueueDesc {
    /// First segment
    Seg0(SendQueueReqDescSeg0),
    /// Second segment
    Seg1(SendQueueReqDescSeg1),
}

impl From<SendQueueDesc> for RingBufDescUntyped {
    fn from(desc: SendQueueDesc) -> Self {
        match desc {
            SendQueueDesc::Seg0(d) => d.into(),
            SendQueueDesc::Seg1(d) => d.into(),
        }
    }
}

/// A transmit queue for the simple NIC device.
pub(crate) struct SendQueue {
    /// Inner ring buffer
    inner: DescRingBuffer,
}

impl SendQueue {
    pub(crate) fn new(ring_buffer: DescRingBuffer) -> Self {
        Self { inner: ring_buffer }
    }

    pub(crate) fn push(&mut self, desc: SendQueueDesc) -> io::Result<()> {
        let addr = self.inner.base_addr();
        self.inner.push(desc.into())
    }

    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.inner.base_addr()
    }

    /// Returns the head pointer of the buffer
    pub(crate) fn head(&self) -> u32 {
        self.inner.head()
    }

    /// Returns the head pointer of the buffer
    pub(crate) fn set_tail(&mut self, tail: u32) {
        self.inner.set_tail(tail);
    }
}
