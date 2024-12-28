use std::{
    io,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::device::constants::{
    CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW,
    CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW,
};

use super::super::{
    constants::{
        CSR_ADDR_CMD_REQ_QUEUE_HEAD, CSR_ADDR_CMD_REQ_QUEUE_TAIL, CSR_ADDR_CMD_RESP_QUEUE_HEAD,
        CSR_ADDR_CMD_RESP_QUEUE_TAIL,
    },
    CsrReaderAdaptor, CsrWriterAdaptor,
};

#[derive(Debug, Clone)]
pub(super) struct RpcClient(Arc<UdpSocket>);

#[derive(Serialize, Deserialize)]
struct CsrAccessRpcMessage {
    is_write: bool,
    addr: usize,
    value: u32,
}

impl RpcClient {
    pub(super) fn new(server_addr: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(server_addr)?;
        Ok(Self(socket.into()))
    }

    pub(super) fn read_csr(&self, addr: usize) -> io::Result<u32> {
        let msg = CsrAccessRpcMessage {
            is_write: false,
            addr,
            value: 0,
        };

        let send_buf = serde_json::to_vec(&msg)?;
        let _: usize = self.0.send(&send_buf)?;

        let mut recv_buf = [0; 128];
        let recv_cnt = self.0.recv(&mut recv_buf)?;
        // the length of CsrAccessRpcMessage is fixed,
        #[allow(clippy::indexing_slicing)]
        let response = serde_json::from_slice::<CsrAccessRpcMessage>(&recv_buf[..recv_cnt])?;

        Ok(response.value)
    }

    pub(super) fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        let msg = CsrAccessRpcMessage {
            is_write: true,
            addr,
            value: data,
        };

        let send_buf = serde_json::to_vec(&msg)?;
        let _: usize = self.0.send(&send_buf)?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct CmdQueueCsrProxy(RpcClient);

impl CmdQueueCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_CMD_REQ_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_CMD_REQ_QUEUE_TAIL;

    pub(crate) fn new(client: RpcClient) -> Self {
        Self(client)
    }

    #[allow(clippy::as_conversions)] // never truncate
    pub(crate) fn write_phys_addr(&self, phy_addr: u64) -> io::Result<()> {
        self.0.write_csr(
            CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW,
            (phy_addr & 0xFFFF_FFFF) as u32,
        )?;
        self.0
            .write_csr(CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH, (phy_addr >> 32) as u32)
    }

    #[allow(clippy::as_conversions, clippy::arithmetic_side_effects)] // never truncate
    pub(crate) fn read_phys_addr(&self) -> io::Result<u64> {
        let x = self.0.read_csr(CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW)?;
        let y = self.0.read_csr(CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH)?;
        Ok(u64::from(x) + (u64::from(y) << 32))
    }
}
impl CsrWriterAdaptor for CmdQueueCsrProxy {
    fn write_head(&self, data: u32) -> io::Result<()> {
        self.0.write_csr(Self::HEAD_CSR, data)
    }

    fn read_tail(&self) -> io::Result<u32> {
        self.0.read_csr(Self::TAIL_CSR)
    }
}

#[derive(Debug)]
pub(crate) struct CmdRespQueueCsrProxy(RpcClient);

impl CmdRespQueueCsrProxy {
    const HEAD_CSR: usize = CSR_ADDR_CMD_RESP_QUEUE_HEAD;
    const TAIL_CSR: usize = CSR_ADDR_CMD_RESP_QUEUE_TAIL;

    pub(crate) fn new(client: RpcClient) -> Self {
        Self(client)
    }

    #[allow(clippy::as_conversions)] // never truncate
    pub(crate) fn write_phys_addr(&self, phy_addr: u64) -> io::Result<()> {
        self.0.write_csr(
            CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW,
            (phy_addr & 0xFFFF_FFFF) as u32,
        )?;
        self.0
            .write_csr(CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH, (phy_addr >> 32) as u32)
    }
}

impl CsrReaderAdaptor for CmdRespQueueCsrProxy {
    fn write_tail(&self, data: u32) -> io::Result<()> {
        self.0.write_csr(Self::TAIL_CSR, data)
    }

    fn read_head(&self) -> io::Result<u32> {
        self.0.read_csr(Self::HEAD_CSR)
    }
}

//#[derive(Debug)]
//pub(crate) struct WorkQueueCsrProxy(RpcClient);
//
//impl WorkQueueCsrProxy {
//    const HEAD_CSR: usize = CSR_ADDR_SEND_QUEUE_HEAD;
//    const TAIL_CSR: usize = CSR_ADDR_SEND_QUEUE_TAIL;
//
//    pub(crate) fn new(client: RpcClient) -> Self {
//        Self(client)
//    }
//}
//
//impl CsrWriterAdaptor for WorkQueueCsrProxy {
//    fn write_head(&self, data: u32) -> io::Result<()> {
//        self.0.write_csr(Self::HEAD_CSR, data)
//    }
//
//    fn read_tail(&self) -> io::Result<u32> {
//        self.0.read_csr(Self::TAIL_CSR)
//    }
//}
//
//#[derive(Debug)]
//pub(crate) struct RecvQueueCsrProxy(RpcClient);
//
//impl RecvQueueCsrProxy {
//    const HEAD_CSR: usize = CSR_ADDR_META_REPORT_QUEUE_HEAD;
//    const TAIL_CSR: usize = CSR_ADDR_META_REPORT_QUEUE_TAIL;
//
//    pub(crate) fn new(client: RpcClient) -> Self {
//        Self(client)
//    }
//}
//
//impl CsrReaderAdaptor for RecvQueueCsrProxy {
//    fn write_tail(&self, data: u32) -> io::Result<()> {
//        self.0.write_csr(Self::TAIL_CSR, data)
//    }
//
//    fn read_head(&self) -> io::Result<u32> {
//        self.0.read_csr(Self::HEAD_CSR)
//    }
//}
