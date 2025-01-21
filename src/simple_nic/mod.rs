/// Routing table configurations
mod route;

/// worker handling NIC frames
mod worker;

#[cfg(test)]
mod tests;

pub(crate) use worker::SimpleNicController;

use std::{
    io::{self},
    sync::{atomic::AtomicBool, Arc},
};

use ipnetwork::IpNetwork;
use worker::SimpleNicWorker;

use crate::{
    net::tap::TapDevice,
    queue::abstr::{FrameRx, FrameTx, RecvBuffer, SimpleNicTunnel},
};

#[allow(clippy::module_name_repetitions)]
/// Configuration for the simple NIC device
#[derive(Debug, Clone, Copy)]
pub struct SimpleNicDeviceConfig {
    /// IP network assigned to the NIC
    network: IpNetwork,
}

impl SimpleNicDeviceConfig {
    /// Creates a new `SimpleNicDeviceConfig`
    #[inline]
    #[must_use]
    pub fn new(network: IpNetwork) -> Self {
        Self { network }
    }
}

/// A simple network interface device that uses TUN/TAP for network connectivity
struct SimpleNicDevice {
    /// The underlying TUN device used for network I/O
    tun_dev: Arc<tun::Device>,
    /// Config of the device
    config: SimpleNicDeviceConfig,
}

impl SimpleNicDevice {
    /// Creates a new `SimpleNicDevice`
    fn new(config: SimpleNicDeviceConfig) -> io::Result<Self> {
        let tun_dev = Arc::new(Self::create_tun(config.network)?);
        Ok(Self { tun_dev, config })
    }

    /// Creates a TUN device that operates at L2
    #[allow(unused_results)] // ignore the config construction result
    fn create_tun(network: IpNetwork) -> io::Result<tun::Device> {
        let mut config = tun::Configuration::default();
        config
            .layer(tun::Layer::L2)
            .address(network.ip())
            .netmask(network.mask())
            .up();

        #[cfg(target_os = "linux")]
        config.platform_config(|platform| {
            // requiring root privilege to acquire complete functions
            platform.ensure_root_privileges(true);
        });

        tun::create(&config).map_err(Into::into)
    }
}

/// A launcher for the `SimpleNic` worker thread that handles communication between
/// the NIC device and tunnel.
pub(crate) struct Launch<Tunnel> {
    /// Abstract Tunnel
    inner: Tunnel,
    /// Tap device
    tap_dev: TapDevice,
}

impl<Tunnel: SimpleNicTunnel> Launch<Tunnel> {
    /// Creates a new `Launch`
    pub(crate) fn new(inner: Tunnel, tap_dev: TapDevice) -> Self {
        Self { inner, tap_dev }
    }

    /// Launches the worker thread that handles communication between the NIC device and tunnel
    pub(crate) fn launch(self, is_shutdown: Arc<AtomicBool>) -> worker::SimpleNicQueueHandle {
        let (frame_tx, frame_rx) = self.inner.into_split();
        let worker = SimpleNicWorker::new(self.tap_dev.inner(), frame_tx, frame_rx, is_shutdown);
        worker.run()
    }
}
