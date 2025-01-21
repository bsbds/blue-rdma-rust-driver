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

use super::constants::{
    CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_HIGH,
    CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_LOW,
    CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_HEAD, CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_TAIL,
    CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_HIGH,
    CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_LOW,
    CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_HEAD, CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_TAIL,
    QP_RECV_ADDR_HIGH, QP_RECV_ADDR_LOW, QP_RECV_HEAD, QP_RECV_TAIL, QP_WQE_ADDR_HIGH,
    QP_WQE_ADDR_LOW, QP_WQE_HEAD, QP_WQE_TAIL,
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

#[derive(Clone, Debug)]
pub(crate) struct SimpleNicTxQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToCard for SimpleNicTxQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for SimpleNicTxQueueCsrProxy<Dev> {
    const HEAD: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_HEAD;
    const TAIL: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_TAIL;
    const BASE_ADDR_LOW: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_LOW;
    const BASE_ADDR_HIGH: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_TX_Q_RINGBUF_BASE_ADDR_HIGH;
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
    const HEAD: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_HEAD;
    const TAIL: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_TAIL;
    const BASE_ADDR_LOW: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_LOW;
    const BASE_ADDR_HIGH: usize = CSR_ADDR_OFFSET_SIMPLE_NIC_RX_Q_RINGBUF_BASE_ADDR_HIGH;
}

impl<Dev> DeviceProxy for SimpleNicRxQueueCsrProxy<Dev> {
    type Device = Dev;

    fn device(&self) -> &Self::Device {
        &self.0
    }
}

macro_rules! impl_send_queue {
    ($name:ident, $n:literal) => {
        #[derive(Clone, Debug)]
        pub(crate) struct $name<Dev>(pub(crate) Dev);

        impl<Dev> ToCard for $name<Dev> {}

        impl<Dev> RingBufferCsrAddr for $name<Dev> {
            const HEAD: usize = QP_WQE_HEAD[$n];
            const TAIL: usize = QP_WQE_TAIL[$n];
            const BASE_ADDR_LOW: usize = QP_WQE_ADDR_LOW[$n];
            const BASE_ADDR_HIGH: usize = QP_WQE_ADDR_HIGH[$n];
        }

        impl<Dev> DeviceProxy for $name<Dev> {
            type Device = Dev;
            fn device(&self) -> &Self::Device {
                &self.0
            }
        }
    };
}

macro_rules! impl_meta_report_queue {
    ($name:ident, $n:literal) => {
        #[derive(Clone, Debug)]
        pub(crate) struct $name<Dev>(pub(crate) Dev);

        impl<Dev> ToHost for $name<Dev> {}

        impl<Dev> RingBufferCsrAddr for $name<Dev> {
            const HEAD: usize = QP_RECV_HEAD[$n];
            const TAIL: usize = QP_RECV_TAIL[$n];
            const BASE_ADDR_LOW: usize = QP_RECV_ADDR_LOW[$n];
            const BASE_ADDR_HIGH: usize = QP_RECV_ADDR_HIGH[$n];
        }

        impl<Dev> DeviceProxy for $name<Dev> {
            type Device = Dev;
            fn device(&self) -> &Self::Device {
                &self.0
            }
        }
    };
}

impl_send_queue!(SendQueueCsrProxy0, 0);
impl_send_queue!(SendQueueCsrProxy1, 1);
impl_send_queue!(SendQueueCsrProxy2, 2);
impl_send_queue!(SendQueueCsrProxy3, 3);

impl_meta_report_queue!(MetaReportQueueCsrProxy0, 0);
impl_meta_report_queue!(MetaReportQueueCsrProxy1, 1);
impl_meta_report_queue!(MetaReportQueueCsrProxy2, 2);
impl_meta_report_queue!(MetaReportQueueCsrProxy3, 3);
