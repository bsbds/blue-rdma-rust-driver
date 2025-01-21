use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use tracing::error;

use crate::{
    desc::simple_nic::{SimpleNicRxQueueDesc, SimpleNicTxQueueDesc},
    device::{
        proxy::{SimpleNicRxQueueCsrProxy, SimpleNicTxQueueCsrProxy},
        CsrBaseAddrAdaptor, DeviceAdaptor,
    },
    mem::page::ContiguousPages,
    queue::{
        abstr::{FrameRx, FrameTx},
        simple_nic::{SimpleNicRxQueue, SimpleNicTxQueue},
        DescRingBuffer, ToCardQueue, ToHostQueue,
    },
};

use super::{SimpleNicDevice, SimpleNicTunnel};

pub(crate) struct SimpleNicController<Dev> {
    tx: FrameTxQueue<Dev>,
    rx: FrameRxQueue<Dev>,
}

impl<Dev: DeviceAdaptor> SimpleNicController<Dev> {
    pub(crate) fn init(
        dev: &Dev,
        tx_rb: DescRingBuffer,
        tx_rb_base_pa: u64,
        rx_rb: DescRingBuffer,
        rx_rb_base_pa: u64,
        rx_bufffer: ContiguousPages<1>,
    ) -> io::Result<Self> {
        let mut tx_queue = SimpleNicTxQueue::new(tx_rb);
        let mut rx_queue = SimpleNicRxQueue::new(rx_rb);
        let req_csr_proxy = SimpleNicTxQueueCsrProxy(dev.clone());
        let resp_csr_proxy = SimpleNicRxQueueCsrProxy(dev.clone());
        req_csr_proxy.write_base_addr(tx_rb_base_pa)?;
        resp_csr_proxy.write_base_addr(rx_rb_base_pa)?;

        Ok(Self {
            tx: FrameTxQueue::new(tx_queue, req_csr_proxy),
            rx: FrameRxQueue::new(rx_queue, rx_bufffer, resp_csr_proxy),
        })
    }
}

impl<Dev: Send + 'static> SimpleNicTunnel for SimpleNicController<Dev> {
    type Sender = FrameTxQueue<Dev>;

    type Receiver = FrameRxQueue<Dev>;

    fn into_split(self, recv_buffer: super::RecvBuffer) -> (Self::Sender, Self::Receiver) {
        (self.tx, self.rx)
    }
}

/// A buffer slot size for a single frame
const FRAME_SLOT_SIZE: usize = 2048;

/// Send frame through `SimpleNicTxQueue`
pub(crate) struct FrameTxQueue<Dev> {
    /// Inner
    inner: SimpleNicTxQueue,
    /// CSR Proxy
    csr_proxy: SimpleNicTxQueueCsrProxy<Dev>,
}

impl<Dev> FrameTxQueue<Dev> {
    /// Creates a new `FrameTxQueue`
    pub(crate) fn new(inner: SimpleNicTxQueue, csr_proxy: SimpleNicTxQueueCsrProxy<Dev>) -> Self {
        Self { inner, csr_proxy }
    }

    /// Build the descriptor from the given buffer
    #[allow(clippy::as_conversions)] // convert *const u8 to u64 is safe
    fn build_desc(buf: &[u8]) -> Option<SimpleNicTxQueueDesc> {
        let len: u32 = buf.len().try_into().ok()?;
        Some(SimpleNicTxQueueDesc::new(buf.as_ptr() as u64, len))
    }
}

impl<Dev: Send + 'static> FrameTx for FrameTxQueue<Dev> {
    fn send(&mut self, buf: &[u8]) -> io::Result<()> {
        let mut desc = Self::build_desc(buf)
            .unwrap_or_else(|| unreachable!("buffer is smaller than u32::MAX"));
        // retry until success
        while self.inner.push(desc).is_err() {
            thread::yield_now();
        }

        Ok(())
    }
}

/// Receive frame from `SimpleNicRxQueue`
pub(crate) struct FrameRxQueue<Dev> {
    /// Queue for receiving frames from the NIC
    rx_queue: SimpleNicRxQueue,
    /// Buffer for storing received frames
    rx_buf: ContiguousPages<1>,
    /// CSR Proxy
    csr_proxy: SimpleNicRxQueueCsrProxy<Dev>,
}

impl<Dev> FrameRxQueue<Dev> {
    /// Creates a new `FrameRxQueue`
    pub(crate) fn new(
        rx_queue: SimpleNicRxQueue,
        rx_buf: ContiguousPages<1>,
        csr_proxy: SimpleNicRxQueueCsrProxy<Dev>,
    ) -> Self {
        Self {
            rx_queue,
            rx_buf,
            csr_proxy,
        }
    }
}

impl<Dev: Send + 'static> FrameRx for FrameRxQueue<Dev> {
    #[allow(clippy::arithmetic_side_effects)]
    #[allow(clippy::as_conversions)] // converting u32 to usize
    fn recv_nonblocking(&mut self) -> io::Result<&[u8]> {
        let Some(desc) = self.rx_queue.pop() else {
            return Err(io::ErrorKind::WouldBlock.into());
        };
        let pos = (desc.slot_idx() as usize)
            .checked_mul(FRAME_SLOT_SIZE)
            .unwrap_or_else(|| unreachable!("invalid index"));

        let len = desc.len() as usize;
        let frame = self
            .rx_buf
            .get(pos..pos + len)
            .unwrap_or_else(|| unreachable!("invalid len"));

        Ok(frame)
    }
}

/// Worker that handles transmitting frames from the network device to the NIC
struct TxWorker<Tx> {
    /// The network device to transmit frames from
    dev: Arc<tun::Device>,
    /// Tx for transmitting frames to remote
    frame_tx: Tx,
    /// Flag to signal worker shutdown
    shutdown: Arc<AtomicBool>,
}

impl<Tx: FrameTx> TxWorker<Tx> {
    /// Creates a new `TxWorker`
    fn new(dev: Arc<tun::Device>, frame_tx: Tx, shutdown: Arc<AtomicBool>) -> Self {
        Self {
            dev,
            frame_tx,
            shutdown,
        }
    }

    /// Build the descriptor from the given buffer
    #[allow(clippy::as_conversions)] // convert *const u8 to u64 is safe
    fn build_desc(buf: &[u8]) -> Option<SimpleNicTxQueueDesc> {
        let len: u32 = buf.len().try_into().ok()?;
        Some(SimpleNicTxQueueDesc::new(buf.as_ptr() as u64, len))
    }

    /// Process a single frame by receiving from device and pushing to tx queue
    #[allow(clippy::indexing_slicing)] // safe for indexing the buffer
    fn process_frame(&mut self, buf: &mut [u8]) -> io::Result<()> {
        let n = self.dev.recv(buf)?;
        self.frame_tx.send(&buf[..n])
    }

    /// Spawns the worker thread and returns its handle
    fn spawn(mut self) -> JoinHandle<io::Result<()>> {
        thread::Builder::new()
            .name("simple-nic-tx-worker".into())
            .spawn(move || {
                let mut buf = vec![0; FRAME_SLOT_SIZE];
                while !self.shutdown.load(Ordering::Relaxed) {
                    if let Err(err) = self.process_frame(&mut buf) {
                        error!("Tx processing error: {err}");
                        return Err(err);
                    }
                }
                Ok(())
            })
            .unwrap_or_else(|err| unreachable!("Failed to spawn tx thread: {err}"))
    }
}

/// Worker that handles receiving frames from the NIC and sending to the network device
struct RxWorker<Rx> {
    /// The network device to send received frames to
    dev: Arc<tun::Device>,
    /// Rx for receiving frames from remote
    frame_rx: Rx,
    /// Flag to signal worker shutdown
    shutdown: Arc<AtomicBool>,
}

impl<Rx: FrameRx> RxWorker<Rx> {
    /// Creates a new `RxWorker`
    fn new(dev: Arc<tun::Device>, frame_rx: Rx, shutdown: Arc<AtomicBool>) -> Self {
        Self {
            dev,
            frame_rx,
            shutdown,
        }
    }

    /// Spawns the worker thread and returns its handle
    fn spawn(mut self) -> JoinHandle<io::Result<()>> {
        thread::Builder::new()
            .name("simple-nic-rx-worker".into())
            .spawn(move || {
                while !self.shutdown.load(Ordering::Relaxed) {
                    let frame = match self.frame_rx.recv_nonblocking() {
                        Ok(frame) => frame,
                        Err(err) if matches!(err.kind(), io::ErrorKind::WouldBlock) => {
                            thread::yield_now();
                            continue;
                        }
                        Err(err) => {
                            tracing::error!("Rx processing error: {err}");
                            return Err(err);
                        }
                    };

                    if let Err(err) = self.dev.send(frame) {
                        tracing::error!("Rx processing error: {err}");
                        return Err(err);
                    }
                }
                Ok(())
            })
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"))
    }
}

/// Main worker that manages the TX and RX queues for the simple NIC
pub(crate) struct SimpleNicWorker<Tx, Rx> {
    /// The network device
    dev: Arc<tun::Device>,
    /// Tx for transmitting frames to remote
    frame_tx: Tx,
    /// Rx for receiving frames from remote
    frame_rx: Rx,
    ///// Queue for transmitting frames to the NIC
    //tx_queue: SimpleNicTxQueue,
    ///// Queue for receiving frames from the NIC
    //rx_queue: SimpleNicRxQueue,
    ///// Buffer for storing received frames
    //rx_buf: ContiguousPages<1>,
    /// Flag to signal worker shutdown
    shutdown: Arc<AtomicBool>,
}

impl<Tx: FrameTx, Rx: FrameRx> SimpleNicWorker<Tx, Rx> {
    /// Creates a new `SimpleNicWorker`
    pub(crate) fn new(
        dev: Arc<tun::Device>,
        frame_tx: Tx,
        frame_rx: Rx,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self {
            dev,
            frame_tx,
            frame_rx,
            shutdown,
        }
    }

    /// Starts the worker threads and returns their handles
    pub(crate) fn run(self) -> SimpleNicQueueHandle {
        let tx_worker = TxWorker::new(
            Arc::clone(&self.dev),
            self.frame_tx,
            Arc::clone(&self.shutdown),
        );
        let rx_worker = RxWorker::new(Arc::clone(&self.dev), self.frame_rx, self.shutdown);

        SimpleNicQueueHandle {
            tx: tx_worker.spawn(),
            rx: rx_worker.spawn(),
        }
    }
}

/// Handle for managing the TX and RX worker threads
#[derive(Debug)]
pub(crate) struct SimpleNicQueueHandle {
    /// Handle to the TX worker thread
    tx: JoinHandle<io::Result<()>>,
    /// Handle to the RX worker thread
    rx: JoinHandle<io::Result<()>>,
}

impl SimpleNicQueueHandle {
    /// Waits for both worker threads to complete
    pub(crate) fn join(self) -> io::Result<()> {
        self.tx.join().map_err(|err| {
            io::Error::new(io::ErrorKind::Other, "tx thread join failed: {err}")
        })??;
        self.rx.join().map_err(|err| {
            io::Error::new(io::ErrorKind::Other, "rx thread join failed: {err}")
        })??;
        Ok(())
    }

    /// Checks if transmit thread has completed
    pub(crate) fn is_tx_finished(&self) -> bool {
        self.tx.is_finished()
    }

    /// Checks if receive thread has completed
    pub(crate) fn is_rx_finished(&self) -> bool {
        self.rx.is_finished()
    }
}
