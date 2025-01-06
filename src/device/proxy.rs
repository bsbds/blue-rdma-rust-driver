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

#[derive(Debug)]
pub(crate) struct CmdQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToCard for CmdQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for CmdQueueCsrProxy<Dev> {
    const HEAD: usize = CSR_ADDR_CMD_REQ_QUEUE_HEAD;
    const TAIL: usize = CSR_ADDR_CMD_REQ_QUEUE_TAIL;
    const BASE_ADDR_LOW: usize = CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW;
    const BASE_ADDR_HIGH: usize = CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH;
}

#[derive(Debug)]
pub(crate) struct CmdRespQueueCsrProxy<Dev>(pub(crate) Dev);

impl<Dev> ToHost for CmdRespQueueCsrProxy<Dev> {}

impl<Dev> RingBufferCsrAddr for CmdRespQueueCsrProxy<Dev> {
    const HEAD: usize = CSR_ADDR_CMD_RESP_QUEUE_HEAD;
    const TAIL: usize = CSR_ADDR_CMD_RESP_QUEUE_TAIL;
    const BASE_ADDR_LOW: usize = CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW;
    const BASE_ADDR_HIGH: usize = CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH;
}

macro_rules! impl_device_adaptor_proxy {
    ($($proxy:ty),*) => {
        $(
            impl<Dev> DeviceAdaptor for $proxy where Dev: DeviceAdaptor {
                fn read_csr(&self, addr: usize) -> io::Result<u32> {
                    self.0.read_csr(addr)
                }

                fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
                    self.0.write_csr(addr, data)
                }
            }
        )*
    };
}

impl_device_adaptor_proxy!(CmdQueueCsrProxy<Dev>, CmdRespQueueCsrProxy<Dev>);
