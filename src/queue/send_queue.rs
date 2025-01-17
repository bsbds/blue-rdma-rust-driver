use std::io;

use crate::desc::{RingBufDescUntyped, SendQueueReqDescSeg0, SendQueueReqDescSeg1};

use super::{ToCardQueue, ToCardQueueTyped};

/// Send queue descriptor types that can be submitted
#[derive(Debug, Clone, Copy)]
pub(crate) enum SendQueueDesc {
    /// First segment
    Seg0(SendQueueReqDescSeg0),
    /// Second segment
    Sge1(SendQueueReqDescSeg1),
}

impl From<SendQueueDesc> for RingBufDescUntyped {
    fn from(desc: SendQueueDesc) -> Self {
        match desc {
            SendQueueDesc::Seg0(d) => d.into(),
            SendQueueDesc::Sge1(d) => d.into(),
        }
    }
}

/// A transmit queue for the simple NIC device.
pub(crate) struct SendQueue {
    /// Inner queue
    inner: ToCardQueueTyped<SendQueueDesc>,
}

impl ToCardQueue for SendQueue {
    type Desc = SendQueueDesc;

    fn push(&mut self, desc: Self::Desc) -> io::Result<()> {
        self.inner.push(desc)
    }
}
