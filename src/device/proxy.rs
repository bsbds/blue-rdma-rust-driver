use std::io;

use crate::device::{
    constants::{
        CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW,
        CSR_ADDR_CMD_REQ_QUEUE_HEAD, CSR_ADDR_CMD_REQ_QUEUE_TAIL,
        CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW,
        CSR_ADDR_CMD_RESP_QUEUE_HEAD, CSR_ADDR_CMD_RESP_QUEUE_TAIL,
    },
    CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor, RingBufferCsrAddr, ToCard, ToHost,
};

/// Trait for proxying access to an underlying RDMA device.
pub(crate) trait DeviceProxy {
    /// The concrete device type being proxied
    type Device;

    /// Returns a reference to the underlying device
    fn device(&self) -> &Self::Device;
}

#[derive(Clone, Debug)]
pub(crate) struct CmdQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToCard for CmdQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for CmdQueueCsrProxy<Dev> {
    const HEAD: usize = CSR_ADDR_CMD_REQ_QUEUE_HEAD;
    const TAIL: usize = CSR_ADDR_CMD_REQ_QUEUE_TAIL;
    const BASE_ADDR_LOW: usize = CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW;
    const BASE_ADDR_HIGH: usize = CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH;
}

impl<Dev> DeviceProxy for CmdQueueCsrProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CmdRespQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToHost for CmdRespQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for CmdRespQueueCsrProxy<Dev> {
    const HEAD: usize = CSR_ADDR_CMD_RESP_QUEUE_HEAD;
    const TAIL: usize = CSR_ADDR_CMD_RESP_QUEUE_TAIL;
    const BASE_ADDR_LOW: usize = CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW;
    const BASE_ADDR_HIGH: usize = CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH;
}

impl<Dev> DeviceProxy for CmdRespQueueCsrProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.0
    }
}
