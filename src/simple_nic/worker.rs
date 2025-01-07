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
    mem::page::ContiguousPages,
    queue::{
        simple_nic::{SimpleNicRxQueue, SimpleNicTxQueue},
        ToCardQueue, ToHostQueue,
    },
};

use super::SimpleNicDevice;

/// A buffer slot size for a single frame
const FRAME_SLOT_SIZE: usize = 2048;

/// Worker that handles transmitting frames from the network device to the NIC
struct TxWorker {
    /// The network device to transmit frames from
    dev: Arc<tun::Device>,
    /// Queue for transmitting frames to the NIC
    tx_queue: SimpleNicTxQueue,
    /// Flag to signal worker shutdown
    shutdown: Arc<AtomicBool>,
}

/// Worker that handles receiving frames from the NIC and sending to the network device
struct RxWorker {
    /// The network device to send received frames to
    dev: Arc<tun::Device>,
    /// Queue for receiving frames from the NIC
    rx_queue: SimpleNicRxQueue,
    /// Buffer for storing received frames
    rx_buf: ContiguousPages<1>,
    /// Flag to signal worker shutdown
    shutdown: Arc<AtomicBool>,
}

impl TxWorker {
    /// Creates a new `TxWorker`
    fn new(dev: Arc<tun::Device>, tx_queue: SimpleNicTxQueue, shutdown: Arc<AtomicBool>) -> Self {
        Self {
            dev,
            tx_queue,
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
        let mut desc = Self::build_desc(&buf[..n])
            .unwrap_or_else(|| unreachable!("buffer is smaller than u32::MAX"));
        // retry until success
        while let Err(d) = self.tx_queue.push(desc) {
            desc = d;
            thread::yield_now();
        }
        Ok(())
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

impl RxWorker {
    /// Creates a new `RxWorker`
    fn new(
        dev: Arc<tun::Device>,
        rx_queue: SimpleNicRxQueue,
        rx_buf: ContiguousPages<1>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self {
            dev,
            rx_queue,
            rx_buf,
            shutdown,
        }
    }

    /// Process a single received frame by sending it to the network device
    #[allow(clippy::arithmetic_side_effects)]
    #[allow(clippy::as_conversions)] // converting u32 to usize
    fn process_frame(&self, desc: SimpleNicRxQueueDesc) -> io::Result<()> {
        let pos = (desc.slot_idx() as usize)
            .checked_mul(FRAME_SLOT_SIZE)
            .unwrap_or_else(|| unreachable!("invalid index"));

        let len = desc.len() as usize;
        let frame = self
            .rx_buf
            .get(pos..pos + len)
            .unwrap_or_else(|| unreachable!("invalid len"));
        let _n = self.dev.send(frame)?;

        Ok(())
    }

    /// Spawns the worker thread and returns its handle
    fn spawn(mut self) -> JoinHandle<io::Result<()>> {
        thread::Builder::new()
            .name("simple-nic-rx-worker".into())
            .spawn(move || {
                while !self.shutdown.load(Ordering::Relaxed) {
                    if let Some(desc) = self.rx_queue.pop() {
                        if let Err(err) = self.process_frame(desc) {
                            tracing::error!("Rx processing error: {err}");
                            return Err(err);
                        }
                    } else {
                        thread::yield_now();
                    }
                }
                Ok(())
            })
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"))
    }
}

/// Main worker that manages the TX and RX queues for the simple NIC
pub(crate) struct SimpleNicWorker {
    /// The network device
    dev: SimpleNicDevice,
    /// Queue for transmitting frames to the NIC
    tx_queue: SimpleNicTxQueue,
    /// Queue for receiving frames from the NIC
    rx_queue: SimpleNicRxQueue,
    /// Buffer for storing received frames
    rx_buf: ContiguousPages<1>,
    /// Flag to signal worker shutdown
    shutdown: Arc<AtomicBool>,
}

impl SimpleNicWorker {
    /// Creates a new `SimpleNicWorker`
    pub(crate) fn new(
        dev: SimpleNicDevice,
        tx_queue: SimpleNicTxQueue,
        rx_queue: SimpleNicRxQueue,
        rx_buf: ContiguousPages<1>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self {
            dev,
            tx_queue,
            rx_queue,
            rx_buf,
            shutdown,
        }
    }

    /// Starts the worker threads and returns their handles
    pub(crate) fn run(self) -> SimpleNicQueueHandle {
        let tx_worker = TxWorker::new(
            Arc::clone(&self.dev.tun_dev),
            self.tx_queue,
            Arc::clone(&self.shutdown),
        );
        let rx_worker = RxWorker::new(
            Arc::clone(&self.dev.tun_dev),
            self.rx_queue,
            self.rx_buf,
            self.shutdown,
        );

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

