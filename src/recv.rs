use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::Arc,
    thread,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{queue_pair::qpn_index, utils::QpTable};

#[derive(Debug, Clone, Copy)]
pub(crate) struct RecvWr {
    pub(crate) wr_id: u64,
    pub(crate) addr: u64,
    pub(crate) length: u32,
    pub(crate) lkey: u32,
}

impl RecvWr {
    #[allow(unsafe_code)]
    pub(crate) fn new(wr: ibverbs_sys::ibv_recv_wr) -> Option<Self> {
        let num_sge = usize::try_from(wr.num_sge).ok()?;
        if num_sge != 1 {
            return None;
        }
        // SAFETY: sg_list is valid when num_sge > 0, which we've verified above
        let sge = unsafe { *wr.sg_list };

        Some(Self {
            wr_id: wr.wr_id,
            addr: sge.addr,
            length: sge.length,
            lkey: sge.lkey,
        })
    }

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

pub(crate) trait PostRecvChannel {
    type Tx: PostRecvTx;
    type Rx: PostRecvRx;
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

pub(crate) struct TcpChannel;

impl PostRecvChannel for TcpChannel {
    type Tx = TcpChannelTx;
    type Rx = TcpChannelRx;
}

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

pub(crate) fn post_recv_channel<C: PostRecvChannel>(
    local_addr: Ipv4Addr,
    dest_addr: Ipv4Addr,
    local_qpn: u32,
    dest_qpn: u32,
) -> io::Result<(C::Tx, C::Rx)> {
    let tx = C::Tx::connect(dest_addr, dest_qpn)?;
    let rx = C::Rx::listen(local_addr, local_qpn)?;
    Ok((tx, rx))
}

fn qpn_to_port(qpn: u32) -> u16 {
    let index = qpn_index(qpn);
    BASE_PORT + index as u16
}

pub(crate) struct PostRecvTxTable<Tx = TcpChannelTx> {
    inner: QpTable<Option<Tx>>,
}

impl<Tx> PostRecvTxTable<Tx> {
    pub(crate) fn new() -> Self {
        Self {
            inner: QpTable::new(),
        }
    }

    pub(crate) fn insert(&mut self, qpn: u32, tx: Tx) {
        let _ignore = self.inner.replace(qpn, Some(tx));
    }

    pub(crate) fn get_qp_mut(&mut self, qpn: u32) -> Option<&mut Tx> {
        self.inner.get_qp_mut(qpn).and_then(Option::as_mut)
    }
}

pub(crate) type SharedRecvWrQueue = Arc<Mutex<VecDeque<RecvWr>>>;

pub(crate) struct RecvWrQueueTable {
    inner: QpTable<SharedRecvWrQueue>,
}

impl RecvWrQueueTable {
    pub(crate) fn new() -> Self {
        Self {
            inner: QpTable::new(),
        }
    }

    pub(crate) fn clone_recv_wr_queue(&self, qpn: u32) -> Option<SharedRecvWrQueue> {
        self.inner.get_qp(qpn).cloned()
    }

    pub(crate) fn pop(&self, qpn: u32) -> Option<RecvWr> {
        let queue = self.inner.get_qp(qpn)?;
        queue.lock().pop_front()
    }
}

pub(crate) struct RecvWorker<Rx = TcpChannelRx> {
    rx: Rx,
    wr_queue: SharedRecvWrQueue,
}

impl<Rx: PostRecvRx + Send + 'static> RecvWorker<Rx> {
    pub(crate) fn new(rx: Rx, wr_queue: SharedRecvWrQueue) -> Self {
        Self { rx, wr_queue }
    }

    // TODO: use tokio
    pub(crate) fn spawn(self) {
        let _handle = thread::Builder::new()
            .name("recv-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    #[allow(clippy::needless_pass_by_value)] // consume the flag
    /// Run the handler loop
    fn run(mut self) {
        while let Ok(wr) = self.rx.recv() {
            self.wr_queue.lock().push_back(wr);
        }
    }
}
