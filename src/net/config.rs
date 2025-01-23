#![allow(clippy::module_name_repetitions)] // exported

use std::{io, net::IpAddr};

use ipnetwork::IpNetwork;

/// Trait for network devices that can provide MAC address and DHCP resolution
pub trait NetworkResolver: Send + Sync + 'static {
    /// Resolve network configuration using DHCP
    ///
    /// # Errors
    /// Returns an error if dynamic discovery fails
    fn resolve_dynamic(&self) -> io::Result<NetworkConfig>;
}

/// MAC address represented as 6 bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MacAddress(pub [u8; 6]);

impl From<MacAddress> for u64 {
    #[inline]
    fn from(mac: MacAddress) -> u64 {
        let mut bytes = [0u8; 8];
        bytes[..6].copy_from_slice(&mac.0);
        u64::from_le_bytes(bytes)
    }
}

impl From<u64> for MacAddress {
    #[inline]
    fn from(mac: u64) -> MacAddress {
        let bytes = mac.to_le_bytes();
        let mut mac_bytes = [0u8; 6];
        mac_bytes.copy_from_slice(&bytes[..6]);
        MacAddress(mac_bytes)
    }
}

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
    /// Dynamic network configuration
    Dynamic {
        /// Network device to use for dynamic resolution
        device: Box<dyn NetworkResolver>,
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
            NetworkMode::Dynamic { ref device } => device.resolve_dynamic(),
        }
    }
}

impl std::fmt::Debug for NetworkMode {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            NetworkMode::Static(ref config) => f.debug_tuple("Static").field(config).finish(),
            NetworkMode::Dynamic { .. } => f.debug_struct("DHCP").finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDevice;

    impl NetworkResolver for MockDevice {
        fn resolve_dynamic(&self) -> io::Result<NetworkConfig> {
            Ok(NetworkConfig {
                ip_network: IpNetwork::new("10.0.0.2".parse().unwrap(), 24).unwrap(),
                gateway: "10.0.0.1".parse().unwrap(),
                mac: MacAddress([0; 6]),
            })
        }
    }

    fn dhcp_resolution_ok() {
        let device = MockDevice;
        let mode = NetworkMode::Dynamic {
            device: Box::new(device),
        };
        let result = mode.resolve();
        assert!(result.is_ok());
    }
}
