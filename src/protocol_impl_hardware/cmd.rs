use std::{io, net::IpAddr, time::Duration};

use ipnetwork::IpNetwork;
use parking_lot::Mutex;

use crate::{
    device_protocol::{DeviceCommand, MttEntry, RecvBufferMeta, UpdateQp},
    mem::{page::ContiguousPages, PageWithPhysAddr},
    mtt::Mtt,
    net::config::NetworkConfig,
    protocol_impl_hardware::device::{
        proxy::{CmdQueueCsrProxy, CmdRespQueueCsrProxy},
        CsrBaseAddrAdaptor, CsrReaderAdaptor, CsrWriterAdaptor, DeviceAdaptor,
    },
};

use super::{
    desc::{
        cmd::{CmdQueueReqDescUpdateMrTable, CmdQueueReqDescUpdatePGT},
        CmdQueueReqDescQpManagement, CmdQueueReqDescSetNetworkParam,
        CmdQueueReqDescSetRawPacketReceiveMeta,
    },
    queue::{
        cmd_queue::{CmdQueue, CmdQueueDesc, CmdRespQueue},
        DescRingBuffer,
    },
};

/// Controller of the command queue
pub(crate) struct CommandController<Dev> {
    /// Command queue pair
    cmd_qp: Mutex<CmdQp>,
    /// Proxy for accessing command queue CSRs
    req_csr_proxy: CmdQueueCsrProxy<Dev>,
    /// Proxy for accessing command response queue CSRs
    resp_csr_proxy: CmdRespQueueCsrProxy<Dev>,
}

impl<Dev: DeviceAdaptor> CommandController<Dev> {
    /// Creates a new command controller instance
    ///
    /// # Returns
    /// A new `CommandController` with an initialized command queue
    pub(crate) fn init(
        dev: &Dev,
        req_rb: DescRingBuffer,
        req_rb_base_pa: u64,
        resp_rb: DescRingBuffer,
        resp_rb_base_pa: u64,
    ) -> io::Result<Self> {
        let mut req_queue = CmdQueue::new(req_rb);
        let mut resp_queue = CmdRespQueue::new(resp_rb);
        let req_csr_proxy = CmdQueueCsrProxy(dev.clone());
        let resp_csr_proxy = CmdRespQueueCsrProxy(dev.clone());
        req_csr_proxy.write_base_addr(req_rb_base_pa)?;
        resp_csr_proxy.write_base_addr(resp_rb_base_pa)?;

        Ok(Self {
            cmd_qp: Mutex::new(CmdQp::new(req_queue, resp_queue)),
            req_csr_proxy,
            resp_csr_proxy,
        })
    }

    /// Creates a new command controller instance
    ///
    /// # Returns
    /// A new `CommandController` with an initialized command queue
    pub(crate) fn init_v2(
        dev: &Dev,
        req_page: PageWithPhysAddr,
        resp_page: PageWithPhysAddr,
    ) -> io::Result<Self> {
        let mut req_queue = CmdQueue::new(DescRingBuffer::new(req_page.page));
        let mut resp_queue = CmdRespQueue::new(DescRingBuffer::new(resp_page.page));
        let req_csr_proxy = CmdQueueCsrProxy(dev.clone());
        let resp_csr_proxy = CmdRespQueueCsrProxy(dev.clone());
        req_csr_proxy.write_base_addr(req_page.phys_addr)?;
        resp_csr_proxy.write_base_addr(resp_page.phys_addr)?;

        Ok(Self {
            cmd_qp: Mutex::new(CmdQp::new(req_queue, resp_queue)),
            req_csr_proxy,
            resp_csr_proxy,
        })
    }

    /// Flush cmd request queue pointer to device
    pub(crate) fn flush_req_queue(&self, req_queue: &CmdQueue) -> io::Result<()> {
        self.req_csr_proxy.write_head(req_queue.head())
    }

    /// Flush cmd response queue pointer to device
    pub(crate) fn flush_resp_queue(&self, resp_queue: &CmdRespQueue) -> io::Result<()> {
        self.resp_csr_proxy.write_tail(resp_queue.tail())
    }
}

// FIXME: Splits `MttEntry` to multiple `CmdQueueReqDescUpdatePGT` if the entry count exceed `UPDATE_PGT_ENTRY_LIMIT`
/// The maximum number of page table entries that can be updated in a single PCI Express burst transaction.
const UPDATE_PGT_ENTRY_LIMIT: usize = 64;

impl<Dev: DeviceAdaptor> DeviceCommand for CommandController<Dev> {
    fn update_mtt(&self, entry: MttEntry<'_>) -> io::Result<()> {
        let id0 = 0;
        let id1 = 1;
        let update_mr_table = CmdQueueReqDescUpdateMrTable::new(
            id0,
            entry.mr_base_va,
            entry.mr_length,
            entry.mr_key,
            entry.pd_handler,
            entry.acc_flags,
            entry.pgt_offset,
        );
        let update_pgt = CmdQueueReqDescUpdatePGT::new(
            id1,
            entry.dma_addr,
            entry.pgt_offset,
            entry.zero_based_entry_count,
        );

        let mut qp = self.cmd_qp.lock();
        let mut update = qp.update();
        update.push(CmdQueueDesc::UpdateMrTable(update_mr_table));
        update.push(CmdQueueDesc::UpdatePGT(update_pgt));
        update.flush(&self.req_csr_proxy);
        update.wait();

        Ok(())
    }

    fn update_qp(&self, entry: UpdateQp) -> io::Result<()> {
        let desc = CmdQueueReqDescQpManagement::new(
            0,
            entry.ip_addr,
            entry.qpn,
            false,
            true,
            entry.peer_qpn,
            entry.rq_access_flags,
            entry.qp_type,
            entry.pmtu,
            entry.local_udp_port,
            entry.peer_mac_addr,
        );

        let mut qp = self.cmd_qp.lock();
        let mut update = qp.update();
        update.push(CmdQueueDesc::ManageQP(desc));
        update.flush(&self.req_csr_proxy);
        update.wait();

        Ok(())
    }

    fn set_network(&self, param: NetworkConfig) -> io::Result<()> {
        let IpAddr::V4(gateway) = param.gateway else {
            unreachable!("IPv6 unsupported")
        };
        let IpNetwork::V4(network) = param.ip_network else {
            unreachable!("IPv6 unsupported");
        };
        let desc = CmdQueueReqDescSetNetworkParam::new(
            0,
            gateway.to_bits(),
            network.mask().to_bits(),
            network.ip().to_bits(),
            param.mac.into(),
        );
        let mut qp = self.cmd_qp.lock();
        let mut update = qp.update();
        update.push(CmdQueueDesc::SetNetworkParam(desc));
        update.flush(&self.req_csr_proxy);
        update.wait();

        Ok(())
    }

    fn set_raw_packet_recv_buffer(&self, meta: RecvBufferMeta) -> io::Result<()> {
        let desc = CmdQueueReqDescSetRawPacketReceiveMeta::new(0, meta.phys_addr);
        let mut qp = self.cmd_qp.lock();
        let mut update = qp.update();
        update.push(CmdQueueDesc::SetRawPacketReceiveMeta(desc));
        update.flush(&self.req_csr_proxy);
        update.wait();

        Ok(())
    }
}

/// Command queue pair
struct CmdQp {
    /// The command request queue
    req_queue: CmdQueue,
    /// The command response queue
    resp_queue: CmdRespQueue,
}

impl CmdQp {
    /// Creates a new command queue pair
    fn new(req_queue: CmdQueue, resp_queue: CmdRespQueue) -> Self {
        Self {
            req_queue,
            resp_queue,
        }
    }

    /// Creates a queue pair update handle to process commands
    fn update(&mut self) -> QpUpdate<'_> {
        QpUpdate {
            num: 0,
            req_queue: &mut self.req_queue,
            resp_queue: &mut self.resp_queue,
        }
    }
}

/// An updates handle
struct QpUpdate<'a> {
    /// Number of updates
    num: usize,
    /// The command request queue
    req_queue: &'a mut CmdQueue,
    /// The command response queue
    resp_queue: &'a mut CmdRespQueue,
}

impl QpUpdate<'_> {
    /// Pushes a new command queue descriptor to the queue.
    fn push(&mut self, desc: CmdQueueDesc) {
        self.num = self.num.wrapping_add(1);
        self.req_queue.push(desc);
    }

    /// Flushes the command queue by writing the head pointer to the CSR proxy.
    fn flush<Dev: DeviceAdaptor>(&self, req_csr_proxy: &CmdQueueCsrProxy<Dev>) {
        req_csr_proxy.write_head(self.req_queue.head());
    }

    /// Waits for responses to all pushed commands.
    fn wait(mut self) {
        while self.num != 0 {
            std::thread::sleep(Duration::from_millis(10));
            //std::hint::spin_loop();
            if let Some(resp) = self.resp_queue.try_pop() {
                self.num = self.num.wrapping_sub(1);
            }
        }
    }
}
