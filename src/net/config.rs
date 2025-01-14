#![allow(clippy::module_name_repetitions)] // exported

use std::{io, net::IpAddr};

use ipnetwork::IpNetwork;

/// Trait for network devices that can provide MAC address and DHCP resolution
pub trait NetworkDevice: Send + Sync + 'static {
    /// Get the MAC address of this network device
    ///
    /// # Errors
    /// Returns an error if unable to retrieve the MAC address from the device
    fn mac_addr(&self) -> io::Result<MacAddress>;

    /// Resolve network configuration using DHCP
    ///
    /// # Arguments
    /// * `static_mac` - Optional MAC address to use instead of device's MAC
    ///
    /// # Errors
    /// Returns an error if DHCP discovery fails
    fn resolve_dhcp(&self, static_mac: Option<MacAddress>) -> io::Result<NetworkConfig>;
}

/// MAC address represented as 6 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress([u8; 6]);

/// Static network configuration containing IP network, gateway and MAC address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct NetworkConfig {
    /// IP network (address and subnet)
    pub ip_network: IpNetwork,
    /// Gateway IP address
    pub gateway: IpAddr,
    /// MAC address
    pub mac: MacAddress,
}

/// Network mode configuration - either static or DHCP
#[non_exhaustive]
pub enum NetworkMode {
    /// Static network configuration
    Static(NetworkConfig),
    /// DHCP configuration with optional MAC override
    Dhcp {
        /// Network device to use for DHCP
        device: Box<dyn NetworkDevice>,
        /// Optional MAC address override
        mac: Option<MacAddress>,
    },
}

impl NetworkMode {
    /// Resolve the network configuration based on the mode
    ///
    /// For static mode, returns the static config directly.
    /// For DHCP mode, resolves configuration using the device.
    pub(crate) fn resolve(&self) -> io::Result<NetworkConfig> {
        match *self {
            NetworkMode::Static(config) => Ok(NetworkConfig {
                ip_network: config.ip_network,
                gateway: config.gateway,
                mac: config.mac,
            }),
            NetworkMode::Dhcp { ref device, mac } => device.resolve_dhcp(mac),
        }
    }
}

impl std::fmt::Debug for NetworkMode {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            NetworkMode::Static(ref config) => f.debug_tuple("Static").field(config).finish(),
            NetworkMode::Dhcp { ref mac, .. } => f.debug_struct("DHCP").field("mac", mac).finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDevice;

    impl NetworkDevice for MockDevice {
        fn resolve_dhcp(&self, static_mac: Option<MacAddress>) -> io::Result<NetworkConfig> {
            Ok(NetworkConfig {
                ip_network: IpNetwork::new("10.0.0.2".parse().unwrap(), 24).unwrap(),
                gateway: "10.0.0.1".parse().unwrap(),
                mac: static_mac.unwrap_or(MacAddress([0; 6])),
            })
        }

        fn mac_addr(&self) -> io::Result<MacAddress> {
            Ok(MacAddress([0; 6]))
        }
    }

    fn dhcp_resolution_ok() {
        let device = MockDevice;
        let mode = NetworkMode::Dhcp {
            device: Box::new(device),
            mac: None,
        };
        let result = mode.resolve();
        assert!(result.is_ok());
    }
}
