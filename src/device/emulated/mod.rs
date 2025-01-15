#![allow(clippy::module_name_repetitions)]

/// Crs client implementation
mod csr;

use std::{io, net::SocketAddr, time::Duration};

use csr::RpcClient;

use crate::{
    desc::{
        cmd::{
            CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT,
            CmdQueueRespDescOnlyCommonHeader,
        },
        RingBufDescUntyped,
    },
    mem::{
        page::{ContiguousPages, EmulatedPageAllocator},
        slot_alloc::SlotAlloc,
        virt_to_phy::{self, PhysAddrResolver, PhysAddrResolverEmulated, VirtToPhys},
    },
    queue::{
        abstr::DeviceCommand,
        cmd_queue::{CmdQueue, CmdQueueDesc, CommandController},
        DescRingBufferAllocator,
    },
    ringbuffer::{RingBuffer, RingCtx},
};

use super::{
    proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
    CsrBaseAddrAdaptor, CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
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

//impl InitializeCsr for EmulatedDevice {
//    type Cmd = CommandController<Self>;
//
//    type Send = ();
//
//    type MetaReport = ();
//
//    type SimpleNic = ();
//
//    fn initialize(&mut self) -> (Self::Cmd, Self::Send, Self::MetaReport, Self::SimpleNic) {
//        (Self::Cmd::new(), (), (), ())
//    }
//}

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
            bluesimalloc::shm_start_addr()..bluesimalloc::heap_start_addr(),
        );
        let mut allocator = DescRingBufferAllocator::new(page_allocator);
        let buffer0 = allocator.alloc().unwrap_or_else(|_| unreachable!());
        let mut buffer1 = allocator.alloc().unwrap_or_else(|_| unreachable!());
        let phy_addr0 = resolver.virt_to_phys(buffer0.base_addr())?.unwrap_or(0);
        let phy_addr1 = resolver.virt_to_phys(buffer1.base_addr())?.unwrap_or(0);
        proxy_cmd_queue.write_base_addr(phy_addr0)?;
        proxy_resp_queue.write_base_addr(phy_addr1)?;
        let mut cmd_queue = CmdQueue::new(dev, buffer0);
        let desc0 =
            CmdQueueDesc::UpdateMrTable(CmdQueueReqDescUpdateMrTable::new(7, 1, 1, 1, 1, 1, 1));
        let desc1 = CmdQueueDesc::UpdatePGT(CmdQueueReqDescUpdatePGT::new(8, 1, 1, 1));
        cmd_queue.push(desc0).unwrap_or_else(|_| unreachable!());
        cmd_queue.push(desc1).unwrap_or_else(|_| unreachable!());
        cmd_queue.flush()?;
        std::thread::sleep(Duration::from_secs(1));

        let resps: Vec<CmdQueueRespDescOnlyCommonHeader> =
            std::iter::repeat_with(|| buffer1.try_pop().copied())
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
