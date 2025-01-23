use std::{
    io,
    ops::{Deref, DerefMut},
};

use crate::desc::simple_nic::{SimpleNicRxQueueDesc, SimpleNicTxQueueDesc};

use super::{DescRingBuffer, ToCardQueue, ToCardQueueTyped, ToHostQueue, ToHostQueueTyped};

/// A transmit queue for the simple NIC device.
pub(crate) struct SimpleNicTxQueue {
    /// Inner queue
    inner: ToCardQueueTyped<SimpleNicTxQueueDesc>,
}

impl SimpleNicTxQueue {
    pub(crate) fn new(inner: DescRingBuffer) -> Self {
        Self {
            inner: ToCardQueueTyped::new(inner),
        }
    }

    pub(crate) fn head(&self) -> u32 {
        self.inner.inner.head()
    }
}

impl ToCardQueue for SimpleNicTxQueue {
    type Desc = SimpleNicTxQueueDesc;

    fn push(&mut self, desc: Self::Desc) -> io::Result<()> {
        self.inner.push(desc)
    }
}

/// A receive queue for the simple NIC device.
pub(crate) struct SimpleNicRxQueue {
    /// Inner queue
    inner: ToHostQueueTyped<SimpleNicRxQueueDesc>,
}

impl SimpleNicRxQueue {
    pub(crate) fn new(inner: DescRingBuffer) -> Self {
        Self {
            inner: ToHostQueueTyped::new(inner),
        }
    }
}

impl ToHostQueue for SimpleNicRxQueue {
    type Desc = SimpleNicRxQueueDesc;

    fn pop(&mut self) -> Option<Self::Desc> {
        self.inner.pop()
    }
}

impl Deref for SimpleNicRxQueue {
    type Target = ToHostQueueTyped<SimpleNicRxQueueDesc>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SimpleNicRxQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
