#![allow(clippy::module_name_repetitions)]

/// Crs client implementation
mod csr;

use std::{
    io, iter,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    thread,
    time::Duration,
};

use csr::RpcClient;
use ipnetwork::{IpNetwork, Ipv4Network};

use crate::{
    device_protocol::DeviceCommand,
    mem::{
        page::{ContiguousPages, EmulatedPageAllocator},
        slot_alloc::SlotAlloc,
        virt_to_phy::{self, AddressResolver, PhysAddrResolver, PhysAddrResolverEmulated},
    },
    net::config::{MacAddress, NetworkConfig},
    protocol_impl_hardware::desc::{
        cmd::{
            CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
            CmdQueueRespDescOnlyCommonHeader,
        },
        RingBufDescUntyped,
    },
    protocol_impl_hardware::device::proxy::{
            },
    protocol_impl_hardware::queue::{
        cmd_queue::{CmdQueue, CmdQueueDesc, CmdRespQueue},
        meta_report_queue::MetaReportQueue,
        send_queue::SendQueue,
        DescRingBufferAllocator,
    },
    protocol_impl_hardware::CommandController,
    protocol_impl_hardware::MetaReportQueueHandler,
    protocol_impl_hardware::SimpleNicController,
    protocol_impl_hardware::{SendQueueScheduler, SendWorkerBuilder},
    ringbuffer::{RingBuffer, RingCtx},
};

use super::{
    proxy::{build_meta_report_queue_proxies, build_send_queue_proxies, CmdQueueCsrProxy, CmdRespQueueCsrProxy},
    CsrBaseAddrAdaptor, CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor, DeviceBuilder,
    InitializeDeviceQueue, PageAllocator,
};

#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct EmulatedDevice(RpcClient);

impl DeviceAdaptor for EmulatedDevice {
    fn read_csr(&self, addr: usize) -> io::Result<u32> {
        self.0.read_csr(addr)
    }

    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()> {
        self.0.write_csr(addr, data)
    }
}

const RPC_SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7700);

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmulatedQueueBuilder {
    rpc_server_addr: SocketAddr,
}

impl EmulatedQueueBuilder {
    #[allow(clippy::as_conversions)]
    pub(crate) fn new(index: usize) -> Self {
        let port = 7700 + index;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port as u16);
        Self {
            rpc_server_addr: addr,
        }
    }
}

impl InitializeDeviceQueue for EmulatedQueueBuilder {
    type Cmd = CommandController<EmulatedDevice>;
    type Send = SendQueueScheduler;
    type MetaReport = MetaReportQueueHandler;
    type SimpleNic = SimpleNicController<EmulatedDevice>;

    #[allow(clippy::indexing_slicing)] // bound is checked
    #[allow(clippy::as_conversions)] // usize to u64
    #[allow(clippy::similar_names)] // it's clear
    fn initialize<A: PageAllocator<1>>(
        &self,
        page_allocator: A,
    ) -> io::Result<(Self::Cmd, Self::Send, Self::MetaReport, Self::SimpleNic)> {
        let mut allocator = DescRingBufferAllocator::new(page_allocator);
        let resolver = PhysAddrResolverEmulated::new(bluesimalloc::shm_start_addr() as u64);
        let cli = RpcClient::new(self.rpc_server_addr)?;
        let dev = EmulatedDevice(cli);
        let proxy_cmd_queue = CmdQueueCsrProxy(dev.clone());
        let proxy_resp_queue = CmdRespQueueCsrProxy(dev.clone());
        let cmd_queue_buffer = allocator.alloc()?;
        let cmd_resp_queue_buffer = allocator.alloc()?;
        let cmdq_base_pa = resolver
            .virt_to_phys(cmd_queue_buffer.base_addr())?
            .unwrap_or_else(|| unreachable!());
        let cmdrespq_base_pa = resolver
            .virt_to_phys(cmd_resp_queue_buffer.base_addr())?
            .unwrap_or_else(|| unreachable!());
        let cmd_controller = CommandController::init(
            &dev,
            cmd_queue_buffer,
            cmdq_base_pa,
            cmd_resp_queue_buffer,
            cmdrespq_base_pa,
        )?;

        let sqs = iter::repeat_with(|| allocator.alloc().map(SendQueue::new))
            .take(4)
            .collect::<Result<Vec<_>, _>>()?;
        let sq_base_pas: Vec<_> = sqs
            .iter()
            .map(SendQueue::base_addr)
            .map(|addr| resolver.virt_to_phys(addr))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let mut sq_proxies = build_send_queue_proxies(dev.clone());
        for (proxy, pa) in sq_proxies.iter_mut().zip(sq_base_pas) {
            proxy.write_base_addr(pa);
        }
        let send_scheduler = SendQueueScheduler::new();
        let builder = SendWorkerBuilder::new_with_global_injector(send_scheduler.injector());
        let mut workers = builder.build_workers(sqs, sq_proxies);
        for worker in workers.drain(3..) {
            let _handle = thread::spawn(|| worker.run());
        }

        let mrqs = iter::repeat_with(|| allocator.alloc().map(MetaReportQueue::new))
            .take(4)
            .collect::<Result<Vec<_>, _>>()?;
        let mrq_base_pas: Vec<_> = mrqs
            .iter()
            .map(MetaReportQueue::base_addr)
            .map(|addr| resolver.virt_to_phys(addr))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let mut mrq_proxies = build_meta_report_queue_proxies(dev.clone());
        for (proxy, pa) in mrq_proxies.iter_mut().zip(mrq_base_pas) {
            proxy.write_base_addr(pa);
        }
        let meta_report_handler = MetaReportQueueHandler::new(mrqs);

        let simple_nic_tx_queue_buffer = allocator.alloc()?;
        let simple_nic_rx_queue_buffer = allocator.alloc()?;
        let simple_nic_tx_queue_pa = resolver
            .virt_to_phys(simple_nic_tx_queue_buffer.base_addr())?
            .unwrap_or_else(|| unreachable!());
        let simple_nic_rx_queue_pa = resolver
            .virt_to_phys(simple_nic_rx_queue_buffer.base_addr())?
            .unwrap_or_else(|| unreachable!());
        let mut allocator = allocator.into_inner();
        let simple_nic_tx_buffer = allocator.alloc()?;
        let simple_nic_tx_buffer_base_pa = resolver
            .virt_to_phys(simple_nic_tx_buffer.addr())?
            .unwrap_or_else(|| unreachable!());
        let simple_nic_rx_buffer = allocator.alloc()?;

        let simple_nic_controller = SimpleNicController::init(
            &dev,
            simple_nic_tx_queue_buffer,
            simple_nic_tx_queue_pa,
            simple_nic_rx_queue_buffer,
            simple_nic_rx_queue_pa,
            simple_nic_tx_buffer,
            simple_nic_tx_buffer_base_pa,
            simple_nic_rx_buffer,
        )?;

        Ok((
            cmd_controller,
            send_scheduler,
            meta_report_handler,
            simple_nic_controller,
        ))
    }
}

impl EmulatedDevice {
    #[allow(
        clippy::as_conversions,
        unsafe_code,
        clippy::missing_errors_doc,
        clippy::indexing_slicing,
        clippy::missing_panics_doc,
        clippy::unwrap_used,
        clippy::unwrap_in_result
    )]
    #[inline]
    pub fn run(rpc_server_addr: SocketAddr) -> io::Result<()> {
        let queue_builder = EmulatedQueueBuilder::new(0);
        let device_builder = DeviceBuilder::new(queue_builder);
        let page_allocator = EmulatedPageAllocator::new(
            bluesimalloc::page_start_addr()..bluesimalloc::heap_start_addr(),
        );
        let resolver = PhysAddrResolverEmulated::new(bluesimalloc::shm_start_addr() as u64);
        let network_config = NetworkConfig {
            ip_network: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(127, 0, 0, 1), 24).unwrap()),
            gateway: Ipv4Addr::new(127, 0, 0, 1).into(),
            mac: MacAddress([0x02, 0x42, 0xAC, 0x11, 0x00, 0x02]),
        };
        let mut bluerdma = device_builder
            .initialize(network_config, page_allocator, resolver, 128, 128)
            .unwrap();

        Ok(())
    }
}
