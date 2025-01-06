use std::{
    io::{self, Read},
    iter,
    net::IpAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

use crate::{
    desc::simple_nic::SimpleNicTxQueueDesc,
    mem::page::ConscMem,
    queue::{
        simple_nic::{SimpleNicRxQueue, SimpleNicTxQueue},
        ToCardQueue, ToHostQueue,
    },
    ring::SyncDevice,
};

/// Configuration for the simple NIC device
#[derive(Debug)]
struct SimpleNicDeviceConfig {
    /// IP address assigned to the NIC
    address: IpAddr,
    /// Network mask for the NIC's subnet
    netmask: IpAddr,
}

/// A simple network interface device that uses TUN/TAP for network connectivity
struct SimpleNicDevice<Dev> {
    /// The underlying TUN device used for network I/O
    tun_dev: tun::Device,
    /// Tx queue to submit descriptors
    queue: SimpleNicTxQueue<Dev>,
}

/// Handle for managing transmit and receive queue threads of a `SimpleNic`
struct SimpleNicQueueHandle {
    /// Join handle for the transmit queue processing thread
    tx: JoinHandle<io::Result<()>>,
    /// Join handle for the receive queue processing thread
    rx: JoinHandle<io::Result<()>>,
}

impl SimpleNicQueueHandle {
    /// Waits for both transmit and receive queue threads to complete
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

impl<Dev> SimpleNicDevice<Dev> {
    /// Creates a TUN device that operates at L2
    #[allow(unused_results)] // ignore the config construction result
    fn create_tun(address: IpAddr, netmask: IpAddr) -> io::Result<tun::Device> {
        let mut config = tun::Configuration::default();
        config
            .layer(tun::Layer::L2)
            .address(address)
            .netmask(netmask)
            .up();

        #[cfg(target_os = "linux")]
        config.platform_config(|platform| {
            // requiring root privilege to acquire complete functions
            platform.ensure_root_privileges(true);
        });

        tun::create(&config).map_err(Into::into)
    }

    /// Build the descriptor from the given buffer
    #[allow(clippy::as_conversions)] // convert *const u8 to u64 is safe
    fn build_desc(buf: &[u8]) -> Option<SimpleNicTxQueueDesc> {
        let len: u32 = buf.len().try_into().ok()?;
        Some(SimpleNicTxQueueDesc::new(buf.as_ptr() as u64, len))
    }

    /// Runs the send/recv
    fn run<RDMADev: SyncDevice + Send + 'static>(
        mut dev: tun::Device,
        mut tx_queue: SimpleNicTxQueue<RDMADev>,
        mut rx_queue: SimpleNicRxQueue<RDMADev>,
        rx_buf: ConscMem,
        shutdown: Arc<AtomicBool>,
    ) -> SimpleNicQueueHandle {
        let mut buf = [0; 2048];
        let dev = Arc::new(dev);
        let dev_c = Arc::clone(&dev);
        let shutdown_c = Arc::clone(&shutdown);
        #[allow(clippy::indexing_slicing)] // safe for indexing the buffer
        let handle_tx = std::thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                let n = dev.recv(&mut buf)?;
                // if queue is full, retry
                loop {
                    let desc = Self::build_desc(&buf[0..n])
                        .unwrap_or_else(|| unreachable!("buffer is smaller than u32::MAX"));
                    // FIXME: return the desc if an error occurred
                    if tx_queue.push(iter::once(desc)).is_ok() {
                        break;
                    }
                }
            }
            Ok::<(), io::Error>(())
        });

        #[allow(clippy::as_conversions, clippy::arithmetic_side_effects)] // u32 to usize
        let handle_rx = std::thread::spawn(move || {
            while !shutdown_c.load(Ordering::Relaxed) {
                if let Some(desc) = rx_queue.pop() {
                    let pos = (desc.slot_idx() as usize)
                        .checked_mul(2048)
                        .unwrap_or_else(|| unreachable!("invalid index"));
                    let len = desc.len() as usize;
                    let packet = rx_buf
                        .get(pos..pos + len)
                        .unwrap_or_else(|| unreachable!("invalid len"));
                    let _n = dev_c.send(packet)?;
                }
            }
            Ok::<(), io::Error>(())
        });

        SimpleNicQueueHandle {
            tx: handle_tx,
            rx: handle_rx,
        }
    }
}
