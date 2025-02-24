use std::{
    io,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::protocol_impl_hardware::device::constants::{
    CSR_ADDR_CMD_REQ_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_REQ_QUEUE_ADDR_LOW,
    CSR_ADDR_CMD_RESP_QUEUE_ADDR_HIGH, CSR_ADDR_CMD_RESP_QUEUE_ADDR_LOW,
};

use super::{
    constants::{
        CSR_ADDR_CMD_REQ_QUEUE_HEAD, CSR_ADDR_CMD_REQ_QUEUE_TAIL, CSR_ADDR_CMD_RESP_QUEUE_HEAD,
        CSR_ADDR_CMD_RESP_QUEUE_TAIL,
    },
    CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
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
        let (recv_cnt, _addr) = self.0.recv_from(&mut recv_buf)?;
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

#[non_exhaustive]
#[derive(Clone, Debug)]
pub(crate) struct EmulatedDevice(RpcClient);

impl EmulatedDevice {
    #[allow(clippy::expect_used)]
    pub(crate) fn new_with_addr(addr: &str) -> Self {
        EmulatedDevice(
            RpcClient::new(addr.parse().expect("invalid socket addr"))
                .expect("failed to connect to emulator"),
        )
    }
}

impl DeviceAdaptor for EmulatedDevice {
    fn read_csr(&self, addr: usize) -> io::Result<u32> {
        self.0.read_csr(addr)
    }

    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        self.0.write_csr(addr, data)
    }
}
