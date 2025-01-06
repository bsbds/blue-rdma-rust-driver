#![allow(clippy::module_name_repetitions)]

/// Crs client implementation
mod csr;

use std::{io, net::SocketAddr, time::Duration};

use csr::RpcClient;

use crate::{
    desc::{cmd::CmdQueueReqDescUpdateMrTable, RingBufDescUntyped},
    mem::{
        page::ConscMem,
        slot_alloc::SlotAlloc,
        virt_to_phy::{self, PhysAddrResolver, PhysAddrResolverEmulated, VirtToPhys},
    },
    queue::cmd_queue::{CmdQueue, CmdQueueDesc},
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

impl EmulatedDevice {
    #[allow(clippy::as_conversions, unsafe_code, clippy::missing_errors_doc)]
    #[inline]
    pub fn run(rpc_server_addr: SocketAddr) -> io::Result<()> {
        let cli = RpcClient::new(rpc_server_addr)?;
        let dev = Self(cli);
        let proxy_cmd_queue = CmdQueueCsrProxy(dev.clone());
        let proxy_resp_queue = CmdRespQueueCsrProxy(dev);
        let mem0 = vec![RingBufDescUntyped::default(); 128];
        let mem1 = vec![RingBufDescUntyped::default(); 128];
        let resolver =
            PhysAddrResolverEmulated::new(unsafe { bluesimalloc::HEAP_START_ADDR as u64 });
        let ptr0 = mem0.as_ptr() as u64;
        let ptr1 = mem1.as_ptr() as u64;
        let phy_addr0 = resolver.virt_to_phys(ptr0)?.unwrap_or(0);
        let phy_addr1 = resolver.virt_to_phys(ptr1)?.unwrap_or(0);
        proxy_cmd_queue.write_base_addr(phy_addr0)?;
        proxy_resp_queue.write_base_addr(phy_addr1)?;

        //let ring_ctx_cmd_queue = RingCtx::new(proxy_cmd_queue);
        //let ring_ctx_resp_queue = RingCtx::new(proxy_resp_queue);
        //let ring = RingBuffer::<_, _, RingBufDescUntyped>::new(ring_ctx_cmd_queue, mem0)
        //    .unwrap_or_else(|| unreachable!());
        //let mut ring1 = RingBuffer::<_, _, RingBufDescUntyped>::new(ring_ctx_resp_queue, mem1)
        //    .unwrap_or_else(|| unreachable!());
        //
        //let mut cmd_queue = CmdQueue::new(ring);
        //let desc =
        //    CmdQueueDesc::UpdateMrTable(CmdQueueReqDescUpdateMrTable::new(7, 0, 0, 0, 0, 0, 0));
        //
        ////cmd_queue.produce(std::iter::once(desc))?;
        //cmd_queue.flush()?;
        //
        //loop {
        //    println!("check");
        //    if let Some(t) = ring1.try_consume() {
        //        break;
        //    }
        //    std::thread::sleep(Duration::from_secs(1));
        //}

        Ok(())
    }
}
