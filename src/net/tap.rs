#![allow(clippy::module_name_repetitions)] // exported

use std::{io, sync::Arc};

use ipnetwork::IpNetwork;

use super::config::NetworkDevice;

/// A TAP device that provides a virtual network interface.
#[derive(Clone)]
pub struct TapDevice {
    /// Inner
    inner: Arc<tun::Device>,
}

impl std::fmt::Debug for TapDevice {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TapDevice").finish()
    }
}

impl TapDevice {
    /// Creates a TUN device that operates at L2
    #[allow(unused_results)] // ignore the config construction result
    fn create(network: IpNetwork) -> io::Result<Self> {
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

        let inner = Arc::new(tun::create(&config)?);

        Ok(Self { inner })
    }
}

impl NetworkDevice for TapDevice {
    #[inline]
    fn mac_addr(&self) -> io::Result<super::config::MacAddress> {
        todo!()
    }

    #[inline]
    fn resolve_dhcp(
        &self,
        static_mac: Option<super::config::MacAddress>,
    ) -> io::Result<super::config::NetworkConfig> {
        todo!()
    }
}
