use std::{
    io, iter,
    net::Ipv4Addr,
    sync::{atomic::AtomicBool, Arc},
};

use crossbeam_deque::Worker;
use parking_lot::Mutex;
use qp_attr::{IbvQpAttr, IbvQpInitAttr};

use crate::{
    ack_responder::AckResponder,
    completion::{
        Completion, CompletionQueueTable, CompletionTask, CompletionWorker, CqManager, Event,
        PostRecvEvent,
    },
    config::DeviceConfig,
    device_protocol::{
        DeviceCommand, MttUpdate, PgtUpdate, RecvBufferMeta, SimpleNicTunnel, UpdateQp,
    },
    mem::{get_num_page, page::PageAllocator, virt_to_phy::AddressResolver, PageWithPhysAddr},
    mtt::{Mtt, PgtEntry},
    net::config::NetworkConfig,
    protocol_impl::{
        queue::meta_report_queue::init_and_spawn_meta_worker, spawn_send_workers,
        CommandController, SendQueueScheduler, SimpleNicController,
    },
    qp::{QpManager, QueuePairAttrTable},
    rdma_write_worker::{RdmaWriteTask, RdmaWriteWorker},
    recv::{
        post_recv_channel, PostRecvTx, PostRecvTxTable, RecvWorker, RecvWr, RecvWrQueueTable,
        TcpChannel,
    },
    send::{SendWr, SendWrBase, SendWrRdma},
};

use super::{mode::Mode, DeviceAdaptor};

pub(crate) trait HwDevice {
    type Adaptor;
    type PageAllocator;
    type PhysAddrResolver;

    fn new_adaptor(&self) -> io::Result<Self::Adaptor>;
    fn new_page_allocator(&self) -> Self::PageAllocator;
    fn new_phys_addr_resolver(&self) -> Self::PhysAddrResolver;
}

pub(crate) trait DeviceOps {
    fn reg_mr(&mut self, addr: u64, length: usize, pd_handle: u32, access: u8) -> io::Result<u32>;
    fn dereg_mr(&mut self, mr_key: u32) -> io::Result<()>;
    fn create_qp(&mut self, attr: IbvQpInitAttr) -> io::Result<u32>;
    fn update_qp(&mut self, qpn: u32, attr: IbvQpAttr) -> io::Result<()>;
    fn destroy_qp(&mut self, qpn: u32);
    fn create_cq(&mut self) -> Option<u32>;
    fn destroy_cq(&mut self, handle: u32);
    fn poll_cq(&mut self, handle: u32, max_num_entries: usize) -> Vec<Completion>;
    fn post_send(&mut self, qpn: u32, wr: SendWr) -> io::Result<()>;
    fn post_recv(&mut self, qpn: u32, wr: RecvWr) -> io::Result<()>;
}

pub(crate) struct HwDeviceCtx<H: HwDevice> {
    device: H,
    mtt: Mtt,
    mtt_buffer: PageWithPhysAddr,
    qp_manager: QpManager,
    cq_manager: CqManager,
    cq_table: CompletionQueueTable,
    cmd_controller: CommandController<H::Adaptor>,
    post_recv_tx_table: PostRecvTxTable,
    recv_wr_queue_table: RecvWrQueueTable,
    rdma_write_tx: flume::Sender<RdmaWriteTask>,
    completion_tx: flume::Sender<CompletionTask>,
    config: DeviceConfig,
}

#[allow(private_bounds)]
impl<H> HwDeviceCtx<H>
where
    H: HwDevice,
    H::Adaptor: DeviceAdaptor + Send + 'static,
    H::PageAllocator: PageAllocator<1>,
    H::PhysAddrResolver: AddressResolver,
{
    pub(crate) fn initialize(device: H, config: DeviceConfig) -> io::Result<Self> {
        let mode = Mode::default();
        let adaptor = device.new_adaptor()?;
        let mut allocator = device.new_page_allocator();
        let addr_resolver = device.new_phys_addr_resolver();
        let mut alloc_page = || PageWithPhysAddr::alloc(&mut allocator, &addr_resolver);
        let cmd_controller = CommandController::init_v2(&adaptor, alloc_page()?, alloc_page()?)?;
        let send_scheduler = SendQueueScheduler::new();
        let send_pages = iter::repeat_with(&mut alloc_page)
            .take(mode.num_channel())
            .collect::<Result<_, _>>()?;
        let meta_pages = iter::repeat_with(&mut alloc_page)
            .take(mode.num_channel())
            .collect::<Result<_, _>>()?;

        let is_shutdown = Arc::new(AtomicBool::new(false));
        let (completion_tx, completion_rx) = flume::unbounded();
        let (ack_tx, ack_rx) = flume::unbounded();
        let (retransmit_tx, retransmit_rx) = flume::unbounded();
        let (packet_retransmit_tx, packet_retransmit_rx) = flume::unbounded();
        let (rdma_write_tx, rdma_write_rx) = flume::unbounded();
        let rx_buffer = alloc_page()?;
        let rx_buffer_pa = rx_buffer.phys_addr;
        let qp_attr_table = QueuePairAttrTable::new();
        let qp_manager = QpManager::new(qp_attr_table.clone_arc());
        let cq_manager = CqManager::new();
        let cq_table = CompletionQueueTable::new();

        let simple_nic_controller = SimpleNicController::init_v2(
            &adaptor,
            alloc_page()?,
            alloc_page()?,
            alloc_page()?,
            rx_buffer,
        )?;
        spawn_send_workers(&adaptor, send_pages, mode, &send_scheduler.injector())?;
        init_and_spawn_meta_worker(
            &adaptor,
            meta_pages,
            mode,
            ack_tx.clone(),
            retransmit_tx.clone(),
            packet_retransmit_tx.clone(),
            completion_tx.clone(),
            rdma_write_tx.clone(),
            Arc::clone(&is_shutdown),
        )?;
        CompletionWorker::new(
            completion_rx,
            cq_table.clone_arc(),
            qp_attr_table.clone_arc(),
            ack_tx,
        )
        .spawn();
        cmd_controller.set_network(config.network())?;
        cmd_controller.set_raw_packet_recv_buffer(RecvBufferMeta::new(rx_buffer_pa))?;

        let (simple_nic_tx, simple_nic_rx) = simple_nic_controller.into_split();
        #[allow(clippy::mem_forget)]
        std::mem::forget(simple_nic_rx); // prevent libc::munmap being called
        AckResponder::new(qp_attr_table.clone_arc(), ack_rx, Box::new(simple_nic_tx)).spawn();
        //TimeoutRetransmitWorker::new(retransmit_rx, send_scheduler.clone_arc(), ack_config).spawn();
        //PacketRetransmitWorker::new(packet_retransmit_rx, send_scheduler.clone_arc()).spawn();
        RdmaWriteWorker::new(
            rdma_write_rx,
            qp_attr_table,
            send_scheduler,
            retransmit_tx,
            packet_retransmit_tx,
            completion_tx.clone(),
        )
        .spawn();

        Ok(Self {
            device,
            cmd_controller,
            qp_manager,
            cq_manager,
            cq_table,
            mtt_buffer: alloc_page()?,
            mtt: Mtt::new(),
            post_recv_tx_table: PostRecvTxTable::new(),
            recv_wr_queue_table: RecvWrQueueTable::new(),
            rdma_write_tx,
            completion_tx,
            config,
        })
    }
}

impl<H: HwDevice> HwDeviceCtx<H> {
    fn send(&self, qpn: u32, mut wr: SendWrBase) -> io::Result<()> {
        match self.recv_wr_queue_table.pop(qpn) {
            Some(x) => {
                if wr.length != x.length {
                    return Err(io::Error::from(io::ErrorKind::InvalidInput));
                }
                let wr = SendWrRdma::new_from_base(wr, x.addr, x.lkey);
                self.rdma_write(qpn, wr)
            }
            None => todo!("return rnr error"),
        }
    }

    fn rdma_read(&self, qpn: u32, wr: SendWrRdma) -> io::Result<()> {
        let (task, result_rx) = RdmaWriteTask::new(qpn, wr);
        self.rdma_write_tx.send(task);
        result_rx
            .recv()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?
    }

    fn rdma_write(&self, qpn: u32, wr: SendWrRdma) -> io::Result<()> {
        let (task, result_rx) = RdmaWriteTask::new(qpn, wr);
        self.rdma_write_tx.send(task);
        result_rx
            .recv()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?
    }

    fn network_config(&self) -> NetworkConfig {
        self.config.network()
    }
}

impl<H> DeviceOps for HwDeviceCtx<H>
where
    H: HwDevice,
    H::Adaptor: DeviceAdaptor + Send + 'static,
    H::PageAllocator: PageAllocator<1>,
    H::PhysAddrResolver: AddressResolver,
{
    fn reg_mr(&mut self, addr: u64, length: usize, pd_handle: u32, access: u8) -> io::Result<u32> {
        fn chunks(entry: PgtEntry) -> Vec<PgtEntry> {
            /// Maximum number of Page Table entries (PGT entries) that can be allocated in a single `PCIe` transaction.
            /// A `PCIe` transaction size is 128 bytes, and each PGT entry is a u64 (8 bytes).
            /// Therefore, 512 bytes / 8 bytes per entry = 16 entries per allocation.
            const MAX_NUM_PGT_ENTRY_PER_ALLOC: usize = 64;

            let base_index = entry.index;
            let end_index = base_index + entry.count;
            (base_index..end_index)
                .step_by(MAX_NUM_PGT_ENTRY_PER_ALLOC)
                .map(|index| PgtEntry {
                    index,
                    count: (MAX_NUM_PGT_ENTRY_PER_ALLOC as u32).min(end_index - index),
                })
                .collect()
        }

        let addr_resolver = self.device.new_phys_addr_resolver();
        let num_pages = get_num_page(addr, length);
        let (mr_key, pgt_entry) = self.mtt.register(num_pages)?;
        let length_u32 =
            u32::try_from(length).map_err(|_err| io::Error::from(io::ErrorKind::InvalidInput))?;
        let mut phys_addrs = addr_resolver
            .virt_to_phys_range(addr, num_pages)?
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(io::Error::new(
                io::ErrorKind::NotFound,
                "physical address not found",
            ))?
            .into_iter();
        let buf = &mut self.mtt_buffer.page;
        let base_index = pgt_entry.index;
        let mtt_update = MttUpdate::new(addr, length_u32, mr_key, pd_handle, access, base_index);
        // TODO: makes updates atomic
        self.cmd_controller.update_mtt(mtt_update)?;
        for PgtEntry { index, count } in chunks(pgt_entry) {
            let bytes: Vec<u8> = phys_addrs
                .by_ref()
                .take(count as usize)
                .flat_map(u64::to_ne_bytes)
                .collect();
            buf.get_mut(..bytes.len())
                .ok_or(io::Error::from(io::ErrorKind::OutOfMemory))?
                .copy_from_slice(&bytes);
            let pgt_update = PgtUpdate::new(self.mtt_buffer.phys_addr, index, count - 1);
            self.cmd_controller.update_pgt(pgt_update)?;
        }

        Ok(mr_key)
    }

    fn dereg_mr(&mut self, mr_key: u32) -> io::Result<()> {
        self.mtt.deregister(mr_key)
    }

    fn create_qp(&mut self, attr: IbvQpInitAttr) -> io::Result<u32> {
        let qpn = self
            .qp_manager
            .create_qp()
            .ok_or(io::Error::from(io::ErrorKind::WouldBlock))?;
        let _ignore = self.qp_manager.update_qp(qpn, |current| {
            current.qpn = qpn;
            current.qp_type = attr.qp_type();
            current.send_cq = attr.send_cq();
            current.recv_cq = attr.recv_cq();
            current.mac_addr = self.network_config().mac.into();
            current.pmtu = ibverbs_sys::IBV_MTU_4096 as u8;
        });
        let entry = UpdateQp {
            ip_addr: self.network_config().ip.ip().to_bits(),
            peer_mac_addr: self.network_config().mac.into(),
            local_udp_port: 0x100,
            qp_type: attr.qp_type(),
            qpn,
            ..Default::default()
        };
        self.cmd_controller.update_qp(entry)?;

        Ok(qpn)
    }

    fn update_qp(&mut self, qpn: u32, attr: IbvQpAttr) -> io::Result<()> {
        let entry = self
            .qp_manager
            .update_qp(qpn, |current| {
                let entry = UpdateQp {
                    qpn,
                    ip_addr: self.network_config().ip.ip().to_bits(),
                    local_udp_port: 0x100,
                    peer_mac_addr: self.network_config().mac.into(),
                    qp_type: current.qp_type,
                    peer_qpn: attr.dest_qp_num().unwrap_or(current.dqpn),
                    rq_access_flags: attr
                        .qp_access_flags()
                        .map_or(current.access_flags, |x| x as u8),
                    pmtu: attr.path_mtu().map_or(current.pmtu, |x| x as u8),
                };
                current.dqpn = entry.peer_qpn;
                current.access_flags = entry.rq_access_flags;
                current.pmtu = entry.pmtu;
                current.dqp_ip = attr.dest_qp_ip().map_or(0, Ipv4Addr::to_bits);
                entry
            })
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;

        self.cmd_controller.update_qp(entry);

        let qp = self
            .qp_manager
            .get_qp(qpn)
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
        if qp.dqpn != 0 && qp.dqp_ip != 0 && self.post_recv_tx_table.get_qp_mut(qpn).is_none() {
            let dqp_ip = Ipv4Addr::from_bits(qp.dqp_ip);
            let (tx, rx) = post_recv_channel::<TcpChannel>(
                self.network_config().ip.ip(),
                dqp_ip,
                qpn,
                qp.dqpn,
            )?;
            self.post_recv_tx_table.insert(qpn, tx);
            let wr_queue = self
                .recv_wr_queue_table
                .clone_recv_wr_queue(qpn)
                .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
            RecvWorker::new(rx, wr_queue).spawn();
        }

        Ok(())
    }

    fn destroy_qp(&mut self, qpn: u32) {
        self.qp_manager.destroy_qp(qpn);
    }

    fn create_cq(&mut self) -> Option<u32> {
        self.cq_manager.create_cq()
    }

    fn destroy_cq(&mut self, handle: u32) {
        self.cq_manager.destroy_cq(handle);
    }

    fn post_send(&mut self, qpn: u32, wr: SendWr) -> io::Result<()> {
        match wr {
            SendWr::Rdma(wr) => self.rdma_write(qpn, wr),
            SendWr::Send(wr) => self.send(qpn, wr),
        }
    }

    fn poll_cq(&mut self, handle: u32, max_num_entries: usize) -> Vec<Completion> {
        let Some(cq) = self.cq_table.get_cq(handle) else {
            return vec![];
        };
        iter::repeat_with(|| cq.pop_front())
            .take_while(Option::is_some)
            .take(max_num_entries)
            .flatten()
            .collect()
    }

    fn post_recv(&mut self, qpn: u32, wr: RecvWr) -> io::Result<()> {
        let qp = self
            .qp_manager
            .get_qp(qpn)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let event = Event::PostRecv(PostRecvEvent::new(wr.wr_id));
        self.completion_tx
            .send(CompletionTask::Register { qpn, event });
        let tx = self
            .post_recv_tx_table
            .get_qp_mut(qpn)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        tx.send(wr)?;

        Ok(())
    }
}

#[allow(unsafe_code, clippy::wildcard_imports)]
pub(crate) mod qp_attr {
    use std::net::Ipv4Addr;

    use ibverbs_sys::*;

    pub(crate) struct IbvQpInitAttr {
        inner: ibv_qp_init_attr,
    }

    impl IbvQpInitAttr {
        pub(crate) fn new(inner: ibv_qp_init_attr) -> Self {
            Self { inner }
        }

        pub(crate) fn qp_type(&self) -> u8 {
            self.inner.qp_type as u8
        }

        pub(crate) fn send_cq(&self) -> Option<u32> {
            unsafe { self.inner.send_cq.as_ref() }.map(|cq| cq.handle)
        }

        pub(crate) fn recv_cq(&self) -> Option<u32> {
            unsafe { self.inner.recv_cq.as_ref() }.map(|cq| cq.handle)
        }
    }

    pub(crate) struct IbvQpAttr {
        inner: ibv_qp_attr,
        attr_mask: u32,
    }

    macro_rules! impl_getter {
        ($name:ident, $type:ty, $mask:expr) => {
            pub(crate) fn $name(&self) -> Option<$type> {
                (self.attr_mask & $mask.0 != 0).then_some(self.inner.$name)
            }
        };
    }

    impl IbvQpAttr {
        pub(crate) fn new(inner: ibv_qp_attr, attr_mask: u32) -> Self {
            Self { inner, attr_mask }
        }

        pub(crate) fn dest_qp_ip(&self) -> Option<Ipv4Addr> {
            if self.attr_mask & ibv_qp_attr_mask::IBV_QP_AV.0 == 0 {
                return None;
            }

            let gid = unsafe { self.inner.ah_attr.grh.dgid.raw };

            // Format: ::ffff:a.b.c.d
            let is_ipv4_mapped =
                gid[..10].iter().all(|&x| x == 0) && gid[10] == 0xFF && gid[11] == 0xFF;

            is_ipv4_mapped.then(|| Ipv4Addr::new(gid[12], gid[13], gid[14], gid[15]))
        }

        impl_getter!(qp_state, ibv_qp_state::Type, ibv_qp_attr_mask::IBV_QP_STATE);
        impl_getter!(
            cur_qp_state,
            ibv_qp_state::Type,
            ibv_qp_attr_mask::IBV_QP_CUR_STATE
        );
        impl_getter!(path_mtu, ibv_mtu, ibv_qp_attr_mask::IBV_QP_PATH_MTU);
        impl_getter!(
            path_mig_state,
            ibv_mig_state,
            ibv_qp_attr_mask::IBV_QP_PATH_MIG_STATE
        );
        impl_getter!(qkey, u32, ibv_qp_attr_mask::IBV_QP_QKEY);
        impl_getter!(rq_psn, u32, ibv_qp_attr_mask::IBV_QP_RQ_PSN);
        impl_getter!(sq_psn, u32, ibv_qp_attr_mask::IBV_QP_SQ_PSN);
        impl_getter!(dest_qp_num, u32, ibv_qp_attr_mask::IBV_QP_DEST_QPN);
        impl_getter!(
            qp_access_flags,
            ::std::os::raw::c_uint,
            ibv_qp_attr_mask::IBV_QP_ACCESS_FLAGS
        );
        impl_getter!(cap, ibv_qp_cap, ibv_qp_attr_mask::IBV_QP_CAP);
        impl_getter!(ah_attr, ibv_ah_attr, ibv_qp_attr_mask::IBV_QP_AV);
        impl_getter!(alt_ah_attr, ibv_ah_attr, ibv_qp_attr_mask::IBV_QP_ALT_PATH);
        impl_getter!(pkey_index, u16, ibv_qp_attr_mask::IBV_QP_PKEY_INDEX);
        impl_getter!(alt_pkey_index, u16, ibv_qp_attr_mask::IBV_QP_ALT_PATH);
        impl_getter!(
            en_sqd_async_notify,
            u8,
            ibv_qp_attr_mask::IBV_QP_EN_SQD_ASYNC_NOTIFY
        );
        impl_getter!(max_rd_atomic, u8, ibv_qp_attr_mask::IBV_QP_MAX_QP_RD_ATOMIC);
        impl_getter!(
            max_dest_rd_atomic,
            u8,
            ibv_qp_attr_mask::IBV_QP_MAX_DEST_RD_ATOMIC
        );
        impl_getter!(min_rnr_timer, u8, ibv_qp_attr_mask::IBV_QP_MIN_RNR_TIMER);
        impl_getter!(port_num, u8, ibv_qp_attr_mask::IBV_QP_PORT);
        impl_getter!(timeout, u8, ibv_qp_attr_mask::IBV_QP_TIMEOUT);
        impl_getter!(retry_cnt, u8, ibv_qp_attr_mask::IBV_QP_RETRY_CNT);
        impl_getter!(rnr_retry, u8, ibv_qp_attr_mask::IBV_QP_RNR_RETRY);
        impl_getter!(alt_port_num, u8, ibv_qp_attr_mask::IBV_QP_ALT_PATH);
        impl_getter!(alt_timeout, u8, ibv_qp_attr_mask::IBV_QP_ALT_PATH);
        impl_getter!(rate_limit, u32, ibv_qp_attr_mask::IBV_QP_RATE_LIMIT);
    }
}
