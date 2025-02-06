use std::io;

use crate::protocol_impl_hardware::device::{
    constants::{
        CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW,
        CSR_ADDR_CMD_REQ_QUEUE_HEAD, CSR_ADDR_CMD_REQ_QUEUE_TAIL,
        CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW,
        CSR_ADDR_CMD_RESP_QUEUE_HEAD, CSR_ADDR_CMD_RESP_QUEUE_TAIL,
    },
    CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor, RingBufferCsrAddr, ToCard, ToHost,
};

use super::{
    constants::{
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_HIGH,
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_LOW,
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_HEAD, CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_TAIL,
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_HIGH,
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_LOW,
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_HEAD, CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_TAIL,
        NUM_QPS, QP_RECV_ADDR_HIGH, QP_RECV_ADDR_LOW, QP_RECV_HEAD, QP_RECV_TAIL, QP_WQE_ADDR_HIGH,
        QP_WQE_ADDR_LOW, QP_WQE_HEAD, QP_WQE_TAIL,
    },
    mode::Mode,
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
    fn head(&self) -> usize {
        CSR_ADDR_CMD_REQ_QUEUE_HEAD
    }

    fn tail(&self) -> usize {
        CSR_ADDR_CMD_REQ_QUEUE_TAIL
    }

    fn base_addr_low(&self) -> usize {
        CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW
    }

    fn base_addr_high(&self) -> usize {
        CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH
    }
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
    fn head(&self) -> usize {
        CSR_ADDR_CMD_RESP_QUEUE_HEAD
    }

    fn tail(&self) -> usize {
        CSR_ADDR_CMD_RESP_QUEUE_TAIL
    }

    fn base_addr_low(&self) -> usize {
        CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW
    }

    fn base_addr_high(&self) -> usize {
        CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH
    }
}

impl<Dev> DeviceProxy for CmdRespQueueCsrProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SimpleNicTxQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToCard for SimpleNicTxQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for SimpleNicTxQueueCsrProxy<Dev> {
    fn head(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_HEAD
    }

    fn tail(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_TAIL
    }

    fn base_addr_low(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_LOW
    }

    fn base_addr_high(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_HIGH
    }
}

impl<Dev> DeviceProxy for SimpleNicTxQueueCsrProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SimpleNicRxQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToHost for SimpleNicRxQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for SimpleNicRxQueueCsrProxy<Dev> {
    fn head(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_HEAD
    }

    fn tail(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_TAIL
    }

    fn base_addr_low(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_LOW
    }

    fn base_addr_high(&self) -> usize {
        CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_HIGH
    }
}

impl<Dev> DeviceProxy for SimpleNicRxQueueCsrProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.0
    }
}

pub(crate) struct SendQueueProxy<Dev> {
    dev: Dev,
    id: usize,
}

impl<Dev> ToCard for SendQueueProxy<Dev> {}

#[allow(clippy::indexing_slicing)] // static
impl<Dev> RingBufferCsrAddr for SendQueueProxy<Dev> {
    fn head(&self) -> usize {
        QP_WQE_HEAD[self.id]
    }

    fn tail(&self) -> usize {
        QP_WQE_TAIL[self.id]
    }

    fn base_addr_low(&self) -> usize {
        QP_WQE_ADDR_LOW[self.id]
    }

    fn base_addr_high(&self) -> usize {
        QP_WQE_ADDR_HIGH[self.id]
    }
}

impl<Dev> DeviceProxy for SendQueueProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.dev
    }
}

pub(crate) struct MetaReportQueueProxy<Dev> {
    dev: Dev,
    id: usize,
}

impl<Dev> ToHost for MetaReportQueueProxy<Dev> {}

#[allow(clippy::indexing_slicing)] // static
impl<Dev> RingBufferCsrAddr for MetaReportQueueProxy<Dev> {
    fn head(&self) -> usize {
        QP_RECV_HEAD[self.id]
    }

    fn tail(&self) -> usize {
        QP_RECV_TAIL[self.id]
    }

    fn base_addr_low(&self) -> usize {
        QP_RECV_ADDR_LOW[self.id]
    }

    fn base_addr_high(&self) -> usize {
        QP_RECV_ADDR_HIGH[self.id]
    }
}

impl<Dev> DeviceProxy for MetaReportQueueProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.dev
    }
}

pub(crate) fn build_send_queue_proxies<Dev: Clone>(
    dev: Dev,
    mode: Mode,
) -> Vec<SendQueueProxy<Dev>> {
    mode.channel_ids()
        .iter()
        .copied()
        .map(|id| SendQueueProxy {
            dev: dev.clone(),
            id,
        })
        .collect()
}

pub(crate) fn build_meta_report_queue_proxies<Dev: Clone>(
    dev: Dev,
    mode: Mode,
) -> Vec<MetaReportQueueProxy<Dev>> {
    mode.channel_ids()
        .iter()
        .copied()
        .map(|id| MetaReportQueueProxy {
            dev: dev.clone(),
            id,
        })
        .collect()
}
