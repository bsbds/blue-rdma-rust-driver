use std::ops::{Deref, DerefMut};

use crate::desc::simple_nic::{SimpleNicRxQueueDesc, SimpleNicTxQueueDesc};

use super::{ToCardQueueTyped, ToHostQueueTyped};

/// A transmit queue for the simple NIC device.
struct SimpleNicTxQueue<Buf, Dev> {
    /// Inner queue
    inner: ToCardQueueTyped<Buf, Dev, SimpleNicTxQueueDesc>,
}

impl<Buf, Dev> Deref for SimpleNicTxQueue<Buf, Dev> {
    type Target = ToCardQueueTyped<Buf, Dev, SimpleNicTxQueueDesc>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Buf, Dev> DerefMut for SimpleNicTxQueue<Buf, Dev> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// A receive queue for the simple NIC device.
struct SimpleNicRxQueue<Buf, Dev> {
    /// Inner queue
    inner: ToHostQueueTyped<Buf, Dev, SimpleNicRxQueueDesc>,
}

impl<Buf, Dev> Deref for SimpleNicRxQueue<Buf, Dev> {
    type Target = ToHostQueueTyped<Buf, Dev, SimpleNicRxQueueDesc>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Buf, Dev> DerefMut for SimpleNicRxQueue<Buf, Dev> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
