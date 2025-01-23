use std::io;

use crate::desc::{RingBufDescUntyped, SendQueueReqDescSeg0, SendQueueReqDescSeg1};

use super::{DescRingBuffer, ToCardQueue, ToCardQueueTyped};

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
    /// Inner queue
    inner: ToCardQueueTyped<SendQueueDesc>,
}

impl SendQueue {
    pub(crate) fn new(ring_buffer: DescRingBuffer) -> Self {
        Self {
            inner: ToCardQueueTyped::new(ring_buffer),
        }
    }

    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.inner.inner.base_addr()
    }

    /// Returns the head pointer of the buffer
    pub(crate) fn head(&self) -> u32 {
        self.inner.inner.head()
    }
}

impl ToCardQueue for SendQueue {
    type Desc = SendQueueDesc;

    fn push(&mut self, desc: Self::Desc) -> io::Result<()> {
        let addr = self.inner.inner.base_addr();
        let pa = addr - bluesimalloc::shm_start_addr() as u64;
        println!("push to addr: {pa:x}, head: {}", self.head());
        self.inner.push(desc)
    }
}
