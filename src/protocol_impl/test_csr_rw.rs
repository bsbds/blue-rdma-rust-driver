use std::{io, net::Ipv4Addr};

use ipnetwork::Ipv4Network;

use crate::{
    device_protocol::DeviceCommand,
    mem::PageWithPhysAddr,
    net::config::{MacAddress, NetworkConfig},
};

use super::{
    device::{
        hardware::{DmaEngineConfigurator, PciHwDevice},
        ops_impl::HwDevice,
    },
    CommandController,
};

/// Device for testing
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct TestDevice;

#[allow(
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]
impl TestDevice {
    /// Init
    #[inline]
    pub fn init() -> io::Result<Self> {
        let device = PciHwDevice::open_default().unwrap();
        device.reset().unwrap();
        device.init_dma_engine().unwrap();
        let adaptor = device.new_adaptor().unwrap();
        let mut allocator = device.new_page_allocator();
        let addr_resolver = device.new_phys_addr_resolver();
        let mut alloc_page = || PageWithPhysAddr::alloc(&mut allocator, &addr_resolver);
        let cmd_controller =
            CommandController::init_v2(&adaptor, alloc_page()?, alloc_page()?).unwrap();
        let network_config = NetworkConfig {
            ip_network: Ipv4Network::new("10.0.0.2".parse().unwrap(), 24).unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            mac: MacAddress([0; 6]),
            post_recv_ip: Ipv4Addr::new(0, 0, 0, 0),
            post_recv_peer_ip: Ipv4Addr::new(0, 0, 0, 0),
        };
        cmd_controller.set_network(network_config).unwrap();

        Ok(Self)
    }

    /// Test
    #[inline]
    pub fn test_rb(iter: usize) -> io::Result<()> {
        let device = PciHwDevice::open_default().unwrap();
        device.reset().unwrap();
        device.init_dma_engine().unwrap();
        let adaptor = device.new_adaptor().unwrap();
        let mut allocator = device.new_page_allocator();
        let addr_resolver = device.new_phys_addr_resolver();
        let mut alloc_page = || PageWithPhysAddr::alloc(&mut allocator, &addr_resolver);
        let cmd_controller =
            CommandController::init_v2(&adaptor, alloc_page()?, alloc_page()?).unwrap();
        let network_config = NetworkConfig {
            ip_network: Ipv4Network::new("10.0.0.2".parse().unwrap(), 24).unwrap(),
            gateway: "10.0.0.1".parse().unwrap(),
            mac: MacAddress([0; 6]),
            post_recv_ip: Ipv4Addr::new(0, 0, 0, 0),
            post_recv_peer_ip: Ipv4Addr::new(0, 0, 0, 0),
        };
        for _ in 0..iter {
            cmd_controller.set_network(network_config).unwrap();
        }
        Ok(())
    }
}
