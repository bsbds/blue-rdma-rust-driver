use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
};

use serde::{Deserialize, Serialize};

use crate::qp::qpn_index;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct RecvWr {
    pub(crate) wr_id: u64,
    pub(crate) addr: u64,
    pub(crate) length: u32,
    pub(crate) lkey: u32,
}

impl RecvWr {
    fn to_bytes(self) -> [u8; size_of::<RecvWr>()] {
        let mut bytes = [0u8; 24];
        bytes[0..8].copy_from_slice(&self.wr_id.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.addr.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.length.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.lkey.to_be_bytes());
        bytes
    }

    #[allow(clippy::unwrap_used)]
    fn from_bytes(bytes: &[u8; size_of::<RecvWr>()]) -> Self {
        Self {
            wr_id: u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
            addr: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
            length: u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            lkey: u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
        }
    }
}

/// A channel for the responder to pass `ibv_recv_wr` to the initiator
pub(crate) trait PostRecvTx: Sized {
    fn connect(addr: Ipv4Addr, dqpn: u32) -> io::Result<Self>;
    fn send(&mut self, wr: RecvWr) -> io::Result<()>;
}

pub(crate) trait PostRecvRx: Sized {
    fn listen(addr: Ipv4Addr, qpn: u32) -> io::Result<Self>;
    fn recv(&mut self) -> io::Result<RecvWr>;
}

const BASE_PORT: u16 = 60000;

pub(crate) struct TcpChannelTx {
    addr: Ipv4Addr,
    dqpn: u32,
    inner: Option<TcpStream>,
}

impl PostRecvTx for TcpChannelTx {
    fn connect(addr: Ipv4Addr, dqpn: u32) -> io::Result<Self> {
        Ok(Self {
            inner: None,
            addr,
            dqpn,
        })
    }

    fn send(&mut self, wr: RecvWr) -> io::Result<()> {
        if self.inner.is_none() {
            self.inner = Some(TcpStream::connect((self.addr, qpn_to_port(self.dqpn)))?);
        }
        let stream = self.inner.as_mut().unwrap_or_else(|| unreachable!());
        stream.write_all(&wr.to_bytes())?;

        Ok(())
    }
}

pub(crate) struct TcpChannelRx {
    inner: TcpListener,
    stream: Option<TcpStream>,
    buf: [u8; size_of::<RecvWr>()],
}

impl PostRecvRx for TcpChannelRx {
    fn listen(addr: Ipv4Addr, qpn: u32) -> io::Result<Self> {
        let inner = TcpListener::bind((addr, qpn_to_port(qpn)))?;
        Ok(Self {
            inner,
            stream: None,
            buf: [0; size_of::<RecvWr>()],
        })
    }

    fn recv(&mut self) -> io::Result<RecvWr> {
        if self.stream.is_none() {
            let (stream, _socket_addr) = self.inner.accept()?;
            self.stream = Some(stream);
        }
        let stream = self.stream.as_mut().unwrap_or_else(|| unreachable!());
        stream.read_exact(self.buf.as_mut())?;
        Ok(RecvWr::from_bytes(&self.buf))
    }
}

pub(crate) fn post_recv_channel<Tx: PostRecvTx, Rx: PostRecvRx>(
    local_addr: Ipv4Addr,
    dest_addr: Ipv4Addr,
    local_qpn: u32,
    dest_qpn: u32,
) -> io::Result<(Tx, Rx)> {
    let tx = Tx::connect(dest_addr, dest_qpn)?;
    let rx = Rx::listen(local_addr, local_qpn)?;

    Ok((tx, rx))
}

fn qpn_to_port(qpn: u32) -> u16 {
    let index = qpn_index(qpn);
    BASE_PORT + index as u16
}
