use std::io;

use crate::{
    device_protocol::DeviceCommand,
    net::config::NetworkConfig,
    protocol_impl::device::{
        proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
        CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
    },
    ringbuffer::{Descriptor, RingBuffer},
};

use super::super::desc::{
    CmdQueueReqDescQpManagement, CmdQueueReqDescSetNetworkParam,
    CmdQueueReqDescSetRawPacketReceiveMeta, CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
    CmdQueueRespDescOnlyCommonHeader, RingBufDescUntyped,
};
use super::DescRingBuffer;

/// Command queue for submitting commands to the device
pub(crate) struct CmdQueue {
    /// Inner ring buffer
    inner: DescRingBuffer,
}

/// Command queue descriptor types that can be submitted
#[derive(Debug, Clone, Copy)]
pub(crate) enum CmdQueueDesc {
    /// Update first stage table command
    UpdateMrTable(CmdQueueReqDescUpdateMrTable),
    /// Update second stage table command
    UpdatePGT(CmdQueueReqDescUpdatePGT),
    /// Manage Queue Pair operations
    ManageQP(CmdQueueReqDescQpManagement),
    /// Set network parameters
    SetNetworkParam(CmdQueueReqDescSetNetworkParam),
    /// Set metadata for raw packet receive operations
    SetRawPacketReceiveMeta(CmdQueueReqDescSetRawPacketReceiveMeta),
}

impl CmdQueue {
    /// Creates a new `CmdQueue`
    pub(crate) fn new(ring_buffer: DescRingBuffer) -> Self {
        Self { inner: ring_buffer }
    }

    /// Produces command descriptors to the queue
    pub(crate) fn push(&mut self, desc: CmdQueueDesc) -> bool {
        match desc {
            CmdQueueDesc::UpdateMrTable(d) => self.inner.push(d.into()),
            CmdQueueDesc::UpdatePGT(d) => self.inner.push(d.into()),
            CmdQueueDesc::ManageQP(d) => self.inner.push(d.into()),
            CmdQueueDesc::SetNetworkParam(d) => self.inner.push(d.into()),
            CmdQueueDesc::SetRawPacketReceiveMeta(d) => self.inner.push(d.into()),
        }
    }

    /// Returns the head pointer
    pub(crate) fn head(&self) -> u32 {
        self.inner.head() as u32
    }

    pub(crate) fn set_tail(&mut self, tail: u32) {
        self.inner.set_tail(tail);
    }
}

/// Command queue response descriptor type
#[derive(Debug, Clone, Copy)]
pub(crate) struct CmdRespQueueDesc(CmdQueueRespDescOnlyCommonHeader);

impl std::ops::Deref for CmdRespQueueDesc {
    type Target = CmdQueueRespDescOnlyCommonHeader;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Queue for receiving command responses from the device
pub(crate) struct CmdRespQueue {
    /// Inner ring buffer
    inner: DescRingBuffer,
}

impl CmdRespQueue {
    /// Creates a new `CmdRespQueue`
    pub(crate) fn new(ring_buffer: DescRingBuffer) -> Self {
        Self { inner: ring_buffer }
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<CmdRespQueueDesc> {
        self.inner.pop().map(Into::into).map(CmdRespQueueDesc)
    }

    /// Return tail pointer
    pub(crate) fn tail(&self) -> u32 {
        self.inner.tail() as u32
    }

    pub(crate) fn set_head(&mut self, head: u32) {
        self.inner.set_head(head);
    }
}
