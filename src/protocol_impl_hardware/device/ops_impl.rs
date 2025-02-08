use std::{
    io, iter,
    marker::PhantomData,
    net::Ipv4Addr,
    ptr,
    sync::{atomic::AtomicBool, Arc, OnceLock},
};

use crossbeam_deque::Worker;
use ipnetwork::{IpNetwork, Ipv4Network};
use parking_lot::Mutex;

use crate::{
    completion::{CompletionEvent, CompletionQueueTable, CqManager, EventRegistry},
    ctx_ops::RdmaCtxOps,
    device_protocol::{
        DeviceCommand, MetaReport, MttEntry, RecvBuffer, RecvBufferMeta, SimpleNicTunnel, UpdateQp,
        WorkReqSend, WrChunk,
    },
    mem::{
        page::{ContiguousPages, EmulatedPageAllocator, PageAllocator},
        virt_to_phy::{AddressResolver, PhysAddrResolverEmulated},
        PageWithPhysAddr,
    },
    meta_worker,
    mtt::Mtt,
    net::{
        config::{MacAddress, NetworkConfig},
        tap::TapDevice,
    },
    protocol_impl_hardware::{
        init_and_spawn_send_workers,
        queue::{
            meta_report_queue::init_and_spawn_meta_worker, DescRingBuffer, DescRingBufferAllocator,
        },
        CommandController, SendQueueScheduler, SendWorker, SendWorkerBuilder, SimpleNicController,
    },
    qp::{DeviceQp, QpInitiatorTable, QpTrackerTable},
    queue_pair::{QpManager, QueuePairAttrTable},
    send::{SendWrResolver, WrFragmenter},
};

use super::{mode::Mode, DeviceAdaptor};

trait HwDevice {
    type Adaptor;
    type PageAllocator;
    type PhysAddrResolver;

    fn new_adaptor(&self) -> Self::Adaptor;
    fn new_page_allocator(&self) -> Self::PageAllocator;
    fn new_phys_addr_resolver(&self) -> Self::PhysAddrResolver;
}

trait Ops {
    /// Updates Memory Translation Table entry
    #[inline]
    fn reg_mr(&mut self, addr: u64, length: usize, access: u8) -> io::Result<u32>;

    /// Updates Queue Pair entry
    #[inline]
    fn update_qp(&self, entry: UpdateQp) -> io::Result<()>;

    /// Sends an RDMA operation
    fn post_send_inner(&mut self, qpn: u32, wr: SendWrResolver) -> io::Result<()>;
}

struct HwDeviceCtx<H: HwDevice> {
    device: H,
    mtt: Mtt,
    mtt_buffer: PageWithPhysAddr,
    qp_manager: QpManager,
    cq_manager: CqManager,
    cmd_controller: CommandController<H::Adaptor>,
    send_scheduler: SendQueueScheduler,
}

impl<H> HwDeviceCtx<H>
where
    H: HwDevice,
    H::Adaptor: DeviceAdaptor + Send + 'static,
    H::PageAllocator: PageAllocator<1>,
    H::PhysAddrResolver: AddressResolver,
{
    fn init(device: H, network_config: NetworkConfig) -> io::Result<Self> {
        let mode = Mode::default();
        let adaptor = device.new_adaptor();
        let mut allocator = device.new_page_allocator();
        let addr_resolver = device.new_phys_addr_resolver();
        let mut alloc_page = || PageWithPhysAddr::alloc(&mut allocator, &addr_resolver);
        let buffer = PageWithPhysAddr::alloc(&mut allocator, &addr_resolver)?;
        let cmd_controller = CommandController::init_v2(&adaptor, alloc_page()?, alloc_page()?)?;
        let send_scheduler = SendQueueScheduler::new();
        let send_pages =
            iter::repeat_with(|| PageWithPhysAddr::alloc(&mut allocator, &addr_resolver))
                .take(mode.num_channel())
                .collect::<Result<_, _>>()?;
        let meta_pages =
            iter::repeat_with(|| PageWithPhysAddr::alloc(&mut allocator, &addr_resolver))
                .take(mode.num_channel())
                .collect::<Result<_, _>>()?;

        let is_shutdown = Arc::new(AtomicBool::new(false));
        init_and_spawn_send_workers(&adaptor, send_pages, mode, send_scheduler.injector())?;
        let (sender_task_tx, sender_task_rx) = flume::unbounded();
        let (receiver_task_tx, receiver_task_rx) = flume::unbounded();
        init_and_spawn_meta_worker(
            &adaptor,
            meta_pages,
            mode,
            sender_task_tx,
            receiver_task_tx,
            Arc::clone(&is_shutdown),
        )?;
        let rx_buffer = alloc_page()?;
        let rx_buffer_pa = rx_buffer.phys_addr;
        let simple_nic_controller = SimpleNicController::init_v2(
            &adaptor,
            alloc_page()?,
            alloc_page()?,
            alloc_page()?,
            rx_buffer,
        )?;
        let meta = RecvBufferMeta::new(rx_buffer_pa);
        cmd_controller.set_network(network_config)?;
        cmd_controller.set_raw_packet_recv_buffer(meta)?;

        let qp_attr_table = QueuePairAttrTable::new();
        let qp_manager = QpManager::new(qp_attr_table.clone_arc());
        let cq_manager = CqManager::new();
        let cq_table = cq_manager.table().clone_arc();
        let mtt_buffer = alloc_page()?;

        Ok(Self {
            device,
            cmd_controller,
            send_scheduler,
            qp_manager,
            cq_manager,
            mtt_buffer,
            mtt: Mtt::new(),
        })
    }
}

impl<H: HwDevice> Ops for HwDeviceCtx<H> {
    fn reg_mr(&mut self, addr: u64, length: usize, access: u8) -> io::Result<u32> {
        todo!()
    }

    fn update_qp(&self, entry: UpdateQp) -> io::Result<()> {
        todo!()
    }

    fn post_send_inner(&mut self, qpn: u32, wr: SendWrResolver) -> io::Result<()> {
        todo!()
    }
}
