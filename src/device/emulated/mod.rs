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

use crate::{
    cmd::CommandController,
    desc::{
        cmd::{
            CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
            CmdQueueRespDescOnlyCommonHeader,
        },
        RingBufDescUntyped,
    },
    device::proxy::{
        MetaReportQueueCsrProxy0, MetaReportQueueCsrProxy1, MetaReportQueueCsrProxy2,
        MetaReportQueueCsrProxy3, SendQueueCsrProxy0, SendQueueCsrProxy1, SendQueueCsrProxy2,
        SendQueueCsrProxy3,
    },
    mem::{
        page::{ContiguousPages, EmulatedPageAllocator},
        slot_alloc::SlotAlloc,
        virt_to_phy::{self, AddressResolver, PhysAddrResolver, PhysAddrResolverEmulated},
    },
    meta_report::MetaReportQueueHandler,
    queue::{
        abstr::DeviceCommand,
        cmd_queue::{CmdQueue, CmdQueueDesc, CmdRespQueue},
        meta_report_queue::MetaReportQueue,
        send_queue::SendQueue,
        DescRingBufferAllocator,
    },
    ringbuffer::{RingBuffer, RingCtx},
    send_scheduler::{SendQueueScheduler, SendWorkerBuilder},
    simple_nic::SimpleNicController,
};

use super::{
    proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
    CsrBaseAddrAdaptor, CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor, InitializeDeviceQueue,
    PageAllocator,
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
    pub(crate) fn new() -> Self {
        Self {
            rpc_server_addr: RPC_SERVER_ADDR,
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
        // TODO: use loop
        let sq_proxy0 = SendQueueCsrProxy0(dev.clone());
        let sq_proxy1 = SendQueueCsrProxy1(dev.clone());
        let sq_proxy2 = SendQueueCsrProxy2(dev.clone());
        let sq_proxy3 = SendQueueCsrProxy3(dev.clone());
        sq_proxy0.write_base_addr(sq_base_pas[0]);
        sq_proxy1.write_base_addr(sq_base_pas[1]);
        sq_proxy2.write_base_addr(sq_base_pas[2]);
        sq_proxy3.write_base_addr(sq_base_pas[3]);
        let proxies: Vec<Box<dyn CsrWriterAdaptor + Send + 'static>> = vec![
            Box::new(sq_proxy0),
            Box::new(sq_proxy1),
            Box::new(sq_proxy2),
            Box::new(sq_proxy3),
        ];
        let send_scheduler = SendQueueScheduler::new();
        let builder = SendWorkerBuilder::new_with_global_injector(send_scheduler.injector());
        let workers = builder.build_workers(sqs, proxies);
        for worker in workers {
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
        let mrq_proxy0 = MetaReportQueueCsrProxy0(dev.clone());
        let mrq_proxy1 = MetaReportQueueCsrProxy1(dev.clone());
        let mrq_proxy2 = MetaReportQueueCsrProxy2(dev.clone());
        let mrq_proxy3 = MetaReportQueueCsrProxy3(dev.clone());
        mrq_proxy0.write_base_addr(mrq_base_pas[0]);
        mrq_proxy1.write_base_addr(mrq_base_pas[1]);
        mrq_proxy2.write_base_addr(mrq_base_pas[1]);
        mrq_proxy3.write_base_addr(mrq_base_pas[1]);
        let meta_report_handler = MetaReportQueueHandler::new(mrqs);

        let simple_nic_tx_queue_buffer = allocator.alloc()?;
        let simple_nic_rx_queue_buffer = allocator.alloc()?;
        let simple_nic_tx_queue_pa = resolver
            .virt_to_phys(simple_nic_tx_queue_buffer.base_addr())?
            .unwrap_or_else(|| unreachable!());
        let simple_nic_rx_queue_pa = resolver
            .virt_to_phys(simple_nic_rx_queue_buffer.base_addr())?
            .unwrap_or_else(|| unreachable!());
        let simple_nic_rx_buffer = allocator.into_inner().alloc()?;

        let simple_nic_controller = SimpleNicController::init(
            &dev,
            simple_nic_tx_queue_buffer,
            simple_nic_rx_queue_pa,
            simple_nic_rx_queue_buffer,
            simple_nic_rx_queue_pa,
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
        clippy::missing_panics_doc
    )]
    #[inline]
    pub fn run(rpc_server_addr: SocketAddr) -> io::Result<()> {
        let cli = RpcClient::new(rpc_server_addr)?;
        let dev = Self(cli);
        let proxy_cmd_queue = CmdQueueCsrProxy(dev.clone());
        let proxy_resp_queue = CmdRespQueueCsrProxy(dev.clone());
        let resolver = PhysAddrResolverEmulated::new(bluesimalloc::shm_start_addr() as u64);
        let ring_ctx_cmd_queue = RingCtx::new();
        let ring_ctx_resp_queue = RingCtx::new();
        let page_allocator = EmulatedPageAllocator::new(
            bluesimalloc::page_start_addr()..bluesimalloc::heap_start_addr(),
        );
        let mut allocator = DescRingBufferAllocator::new(page_allocator);
        let buffer0 = allocator.alloc().unwrap_or_else(|_| unreachable!());
        let mut buffer1 = allocator.alloc().unwrap_or_else(|_| unreachable!());
        let phy_addr0 = resolver.virt_to_phys(buffer0.base_addr())?.unwrap_or(0);
        let phy_addr1 = resolver.virt_to_phys(buffer1.base_addr())?.unwrap_or(0);
        proxy_cmd_queue.write_base_addr(phy_addr0)?;
        proxy_resp_queue.write_base_addr(phy_addr1)?;
        let mut cmd_queue = CmdQueue::new(buffer0);
        let desc0 =
            CmdQueueDesc::UpdateMrTable(CmdQueueReqDescUpdateMrTable::new(7, 1, 1, 1, 1, 1, 1));
        let desc1 = CmdQueueDesc::UpdatePGT(CmdQueueReqDescUpdatePGT::new(8, 1, 1, 1));
        cmd_queue.push(desc0).unwrap_or_else(|_| unreachable!());
        cmd_queue.push(desc1).unwrap_or_else(|_| unreachable!());
        // TODO: flush
        //cmd_queue.flush()?;
        thread::sleep(Duration::from_secs(1));

        let resps: Vec<CmdQueueRespDescOnlyCommonHeader> =
            iter::repeat_with(|| buffer1.try_pop().copied())
                .flatten()
                .take(2)
                .map(Into::into)
                .collect();

        assert_eq!(
            resps[0].headers().cmd_queue_common_header().user_data(),
            7,
            "user data not match"
        );
        assert_eq!(
            resps[1].headers().cmd_queue_common_header().user_data(),
            8,
            "user data not match"
        );

        Ok(())
    }
}
