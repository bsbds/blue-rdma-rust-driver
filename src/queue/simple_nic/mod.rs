use std::{
    io,
    ops::{Deref, DerefMut},
};

use crate::{
    desc::simple_nic::{SimpleNicRxQueueDesc, SimpleNicTxQueueDesc},
    ring::SyncDevice,
};

use super::{ToCardQueue, ToCardQueueTyped, ToHostQueue, ToHostQueueTyped};

/// A transmit queue for the simple NIC device.
pub(crate) struct SimpleNicTxQueue<Dev> {
    /// Inner queue
    inner: ToCardQueueTyped<Dev, SimpleNicTxQueueDesc>,
}

impl<Dev: SyncDevice> ToCardQueue for SimpleNicTxQueue<Dev> {
    type Desc = SimpleNicTxQueueDesc;

    fn push<Descs: ExactSizeIterator<Item = Self::Desc>>(
        &mut self,
        descs: Descs,
    ) -> io::Result<()> {
        self.inner.push(descs)
    }
}

/// A receive queue for the simple NIC device.
pub(crate) struct SimpleNicRxQueue<Dev> {
    /// Inner queue
    inner: ToHostQueueTyped<Dev, SimpleNicRxQueueDesc>,
}

impl<Dev: SyncDevice> ToHostQueue for SimpleNicRxQueue<Dev> {
    type Desc = SimpleNicRxQueueDesc;

    fn pop(&mut self) -> Option<Self::Desc> {
        self.inner.pop()
    }
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
