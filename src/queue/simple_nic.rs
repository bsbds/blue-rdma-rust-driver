use std::{
    io,
    ops::{Deref, DerefMut},
};

use crate::desc::simple_nic::{SimpleNicRxQueueDesc, SimpleNicTxQueueDesc};

use super::DescRingBuffer;

/// A transmit queue for the simple NIC device.
pub(crate) struct SimpleNicTxQueue {
    /// Inner queue
    inner: DescRingBuffer,
}

impl SimpleNicTxQueue {
    pub(crate) fn new(inner: DescRingBuffer) -> Self {
        Self { inner }
    }

    pub(crate) fn push(&mut self, desc: SimpleNicTxQueueDesc) -> io::Result<()> {
        self.inner.push(desc.into())
    }

    pub(crate) fn head(&self) -> u32 {
        self.inner.head()
    }
}

/// A receive queue for the simple NIC device.
pub(crate) struct SimpleNicRxQueue {
    /// Inner queue
    inner: DescRingBuffer,
}

impl SimpleNicRxQueue {
    pub(crate) fn new(inner: DescRingBuffer) -> Self {
        Self { inner }
    }

    pub(crate) fn pop(&mut self) -> Option<SimpleNicRxQueueDesc> {
        self.inner.try_pop().copied().map(Into::into)
    }
}
