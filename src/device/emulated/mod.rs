#![allow(clippy::module_name_repetitions)]

use std::{io, net::SocketAddr, time::Duration};

use csr::{CmdQueueCsrProxy, CmdRespQueueCsrProxy, RpcClient};

use crate::{
    desc::{cmd::CmdQueueReqDescUpdateMrTable, RingBufDescUntyped},
    mem::{
        page::ConscMem,
        slot_alloc::SlotAlloc,
        virt_to_phy::{self, PhysAddrResolver, PhysAddrResolverEmulated, VirtToPhys},
    },
    queue::cmd_queue::{CmdQueue, CmdQueueDesc},
    ring::{Ring, RingCtx, SyncDevice},
};

use super::{CsrReaderAdaptor, CsrWriterAdaptor};

/// Crs client implementation
mod csr;

#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct EmulatedDevice;

impl SyncDevice for CmdQueueCsrProxy {
    fn sync_head_ptr(&self, value: u32) -> io::Result<()> {
        self.write_head(value)
    }

    fn sync_tail_ptr(&self, value: u32) -> io::Result<()> {
        unreachable!()
    }
}

impl SyncDevice for CmdRespQueueCsrProxy {
    fn sync_head_ptr(&self, value: u32) -> io::Result<()> {
        unreachable!()
    }

    fn sync_tail_ptr(&self, value: u32) -> io::Result<()> {
        self.write_tail(value)
    }
}

impl EmulatedDevice {
    #[allow(clippy::as_conversions, unsafe_code, clippy::missing_errors_doc)]
    #[inline]
    pub fn run(rpc_server_addr: SocketAddr) -> io::Result<()> {
        let cli = RpcClient::new(rpc_server_addr)?;
        let proxy_cmd_queue = CmdQueueCsrProxy::new(cli.clone());
        let proxy_resp_queue = CmdRespQueueCsrProxy::new(cli.clone());

        let mem0 = vec![RingBufDescUntyped::default(); 128];
        let mem1 = vec![RingBufDescUntyped::default(); 128];
        let resolver =
            PhysAddrResolverEmulated::new(unsafe { bluesimalloc::HEAP_START_ADDR as u64 });
        let ptr0 = mem0.as_ptr() as u64;
        let ptr1 = mem1.as_ptr() as u64;
        let phy_addr0 = resolver.virt_to_phys(ptr0)?.unwrap_or(0);
        let phy_addr1 = resolver.virt_to_phys(ptr1)?.unwrap_or(0);
        proxy_cmd_queue.write_phys_addr(phy_addr0)?;
        proxy_resp_queue.write_phys_addr(phy_addr1)?;

        let ring_ctx_cmd_queue = RingCtx::new(proxy_cmd_queue);
        let ring_ctx_resp_queue = RingCtx::new(proxy_resp_queue);
        let ring = Ring::<_, _, RingBufDescUntyped>::new(ring_ctx_cmd_queue, mem0)
            .unwrap_or_else(|| unreachable!());
        let mut ring1 = Ring::<_, _, RingBufDescUntyped>::new(ring_ctx_resp_queue, mem1)
            .unwrap_or_else(|| unreachable!());

        let mut cmd_queue = CmdQueue::new(ring);
        let desc =
            CmdQueueDesc::UpdateMrTable(CmdQueueReqDescUpdateMrTable::new(7, 0, 0, 0, 0, 0, 0));

        //cmd_queue.produce(std::iter::once(desc))?;
        cmd_queue.flush()?;

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
