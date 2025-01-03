use std::ops::{Deref, DerefMut};

use crate::desc::simple_nic::{SimpleNicRxQueueDesc, SimpleNicTxQueueDesc};

use super::{ToCardQueueTyped, ToHostQueueTyped};

/// A transmit queue for the simple NIC device.
pub(crate) struct SimpleNicTxQueue<Dev> {
    /// Inner queue
    inner: ToCardQueueTyped<Dev, SimpleNicTxQueueDesc>,
}

impl<Dev> Deref for SimpleNicTxQueue<Dev> {
    type Target = ToCardQueueTyped<Dev, SimpleNicTxQueueDesc>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Dev> DerefMut for SimpleNicTxQueue<Dev> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// A receive queue for the simple NIC device.
pub(crate) struct SimpleNicRxQueue<Dev> {
    /// Inner queue
    inner: ToHostQueueTyped<Dev, SimpleNicRxQueueDesc>,
}

impl<Dev> Deref for SimpleNicRxQueue<Dev> {
    type Target = ToHostQueueTyped<Dev, SimpleNicRxQueueDesc>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Dev> DerefMut for SimpleNicRxQueue<Dev> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
