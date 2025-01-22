#![allow(missing_docs, clippy::missing_docs_in_private_items)]
#![allow(clippy::todo)] // FIXME: implement
#![allow(clippy::missing_errors_doc)] // FIXME: add error docs

/// Hardware device adaptor
pub(crate) mod hardware;

/// Emulated device adaptor
pub(crate) mod emulated;

/// Dummy device adaptor for testing
pub(crate) mod dummy;

/// CSR proxy types
pub(crate) mod proxy;

/// Memory-mapped I/O addresses of device registers
mod constants;

use std::{
    io,
    marker::PhantomData,
    net::Ipv4Addr,
    ptr,
    sync::{atomic::AtomicBool, Arc, OnceLock},
};

use emulated::EmulatedQueueBuilder;
use ipnetwork::{IpNetwork, Ipv4Network};
use parking_lot::Mutex;
use proxy::DeviceProxy;

use crate::{
    completion::{CompletionEvent, CqManager, EventRegistry, MetaCqTable},
    ctx_ops::RdmaCtxOps,
    mem::{
        page::{ContiguousPages, EmulatedPageAllocator, PageAllocator},
        virt_to_phy::{PhysAddrResolverEmulated, VirtToPhys},
    },
    meta_worker,
    mtt::v2::Mttv2,
    net::{
        config::{MacAddress, NetworkConfig},
        tap::TapDevice,
    },
    qp::{DeviceQp, QpInitiatorTable, QpManager, QpTrackerTable},
    queue::abstr::{
        DeviceCommand, MetaReport, MttEntry, QpEntry, RecvBuffer, RecvBufferMeta, SimpleNicTunnel,
        WorkReqSend, WrChunk,
    },
    send::{SendWrResolver, WrFragmenter},
    simple_nic,
};

/// A trait for interacting with device hardware through CSR operations.
pub(crate) trait DeviceAdaptor: Clone {
    /// Reads from a CSR at the specified address.
    ///
    /// # Arguments
    ///
    /// * `addr` - The 64-bit memory address of the CSR to read from
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful read, or an error if the read operation fails
    fn read_csr(&self, addr: usize) -> io::Result<u32>;

    /// Writes data to a Control and Status Register at the specified address.
    ///
    /// # Arguments
    ///
    /// * `addr` - The 64-bit memory address of the CSR to write to
    /// * `data` - The 32-bit data value to write to the register
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful write, or an error if the write operation fails
    fn write_csr(&self, addr: usize, data: u32) -> io::Result<()>;
}

/// Trait for types that have ring buffer CSR addresses
pub(crate) trait RingBufferCsrAddr {
    /// Memory address of the head pointer register
    const HEAD: usize;
    /// Memory address of the tail pointer register
    const TAIL: usize;
    /// Memory address of the low 32 bits of the base address register
    const BASE_ADDR_LOW: usize;
    /// Memory address of the high 32 bits of the base address register
    const BASE_ADDR_HIGH: usize;
}

/// Marker trait for ring buffers that transfer data from host to card
pub(crate) trait ToCard {}

/// Marker trait for ring buffers that transfer data from card to host
pub(crate) trait ToHost {}

/// An adaptor to read the tail pointer and write the head pointer, using by writer.
pub(crate) trait CsrWriterAdaptor {
    /// Write the head pointer value
    fn write_head(&self, data: u32) -> io::Result<()>;
    /// Read the tail pointer value
    fn read_tail(&self) -> io::Result<u32>;
}

/// An adaptor to read the head pointer and write the tail pointer, using by reader.
pub(crate) trait CsrReaderAdaptor {
    /// Write the tail pointer value
    fn write_tail(&self, data: u32) -> io::Result<()>;
    /// Read the head pointer value
    fn read_head(&self) -> io::Result<u32>;
}

/// An adaptor to setup the base address of the ring buffer
pub(crate) trait CsrBaseAddrAdaptor {
    /// Read the base physical address of the ring buffer
    fn read_base_addr(&self) -> io::Result<u64>;
    /// Write the base physical address of the ring buffer
    fn write_base_addr(&self, phys_addr: u64) -> io::Result<()>;
}

impl<T> CsrWriterAdaptor for T
where
    T: DeviceProxy + ToCard + RingBufferCsrAddr,
    <T as DeviceProxy>::Device: DeviceAdaptor,
{
    fn write_head(&self, data: u32) -> io::Result<()> {
        self.device().write_csr(T::HEAD, data)
    }

    fn read_tail(&self) -> io::Result<u32> {
        self.device().read_csr(T::TAIL)
    }
}

impl<T> CsrReaderAdaptor for T
where
    T: DeviceProxy + ToHost + RingBufferCsrAddr,
    <T as DeviceProxy>::Device: DeviceAdaptor,
{
    fn write_tail(&self, data: u32) -> io::Result<()> {
        self.device().write_csr(Self::TAIL, data)
    }

    fn read_head(&self) -> io::Result<u32> {
        self.device().read_csr(Self::HEAD)
    }
}

impl<T> CsrBaseAddrAdaptor for T
where
    T: DeviceProxy + RingBufferCsrAddr,
    <T as DeviceProxy>::Device: DeviceAdaptor,
{
    #[allow(clippy::arithmetic_side_effects)]
    fn read_base_addr(&self) -> io::Result<u64> {
        let lo = self.device().read_csr(Self::BASE_ADDR_LOW)?;
        let hi = self.device().read_csr(Self::BASE_ADDR_HIGH)?;
        Ok(u64::from(lo) + (u64::from(hi) << 32))
    }

    #[allow(clippy::as_conversions)]
    fn write_base_addr(&self, phys_addr: u64) -> io::Result<()> {
        self.device()
            .write_csr(Self::BASE_ADDR_LOW, (phys_addr & 0xFFFF_FFFF) as u32)?;
        self.device()
            .write_csr(Self::BASE_ADDR_HIGH, (phys_addr >> 32) as u32)
    }
}

pub(crate) trait InitializeDeviceQueue {
    type Cmd;
    type Send;
    type MetaReport;
    type SimpleNic;

    #[allow(clippy::type_complexity)]
    fn initialize<A: PageAllocator<1>>(
        &self,
        allocator: A,
    ) -> io::Result<(Self::Cmd, Self::Send, Self::MetaReport, Self::SimpleNic)>;
}

#[derive(Debug)]
pub(crate) struct DeviceBuilder<B> {
    queue_builder: B,
}

impl<B> DeviceBuilder<B>
where
    B: InitializeDeviceQueue,
    B::Send: WorkReqSend + Send + 'static,
    B::Cmd: DeviceCommand + Send + 'static,
    B::MetaReport: MetaReport + Send + 'static,
    B::SimpleNic: SimpleNicTunnel + Send + 'static,
{
    pub(crate) fn new(queue_builder: B) -> Self {
        Self { queue_builder }
    }

    pub(crate) fn initialize<A, R>(
        &self,
        network: NetworkConfig,
        mut allocator: A,
        phys_addr_resolver: &R,
        max_num_qps: u32,
        max_num_cqs: u32,
    ) -> io::Result<BlueRdma>
    where
        A: PageAllocator<1>,
        R: VirtToPhys,
    {
        let buffer = allocator.alloc()?;
        let buffer_phys_addr = phys_addr_resolver
            .virt_to_phys(buffer.addr())?
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
        let (cmd_queue, send_queue, meta_report_queue, simple_nic) =
            self.queue_builder.initialize(allocator)?;
        let tap_dev = TapDevice::create(Some(network.mac), Some(network.ip_network))?;
        let recv_buffer_virt_addr = simple_nic.recv_buffer_virt_addr();
        let phys_addr = phys_addr_resolver
            .virt_to_phys(recv_buffer_virt_addr)?
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
        let meta = RecvBufferMeta::new(phys_addr);
        cmd_queue.set_network(network)?;
        cmd_queue.set_raw_packet_recv_buffer(meta)?;
        let qp_manager = QpManager::new(max_num_qps);
        let cq_manager = CqManager::new(max_num_cqs);
        let meta_cq_table = cq_manager.new_meta_table();
        let (initiator_table, tracker_table) = qp_manager.new_split();
        Self::launch_backgroud(
            meta_report_queue,
            simple_nic,
            tap_dev,
            tracker_table,
            meta_cq_table,
        );

        Ok(BlueRdma {
            cmd_queue: Box::new(cmd_queue),
            send_queue: Box::new(send_queue),
            qp_manager,
            qp_table: initiator_table,
            cq_manager,
            mtt: Mttv2::new_simple(),
            buffer,
            buffer_phys_addr,
        })
    }

    fn launch_backgroud(
        meta_report: B::MetaReport,
        simple_nic: B::SimpleNic,
        tap_dev: TapDevice,
        tracker_table: QpTrackerTable,
        meta_cq_table: MetaCqTable,
    ) {
        let is_shutdown = Arc::new(AtomicBool::new(false));
        let launch = simple_nic::Launch::new(simple_nic, tap_dev);
        let _handle = launch.launch(Arc::clone(&is_shutdown));
        let launch_meta = meta_worker::Launch::new(meta_report, tracker_table, meta_cq_table);
        launch_meta.launch(Arc::clone(&is_shutdown));
    }
}

#[allow(missing_debug_implementations)]
pub struct BlueRdma {
    pub(crate) cmd_queue: Box<dyn DeviceCommand + Send + 'static>,
    pub(crate) send_queue: Box<dyn WorkReqSend + Send + 'static>,
    pub(crate) qp_manager: QpManager,
    pub(crate) cq_manager: CqManager,
    pub(crate) qp_table: QpInitiatorTable,
    pub(crate) mtt: Mttv2,
    buffer: ContiguousPages<1>,
    buffer_phys_addr: u64,
}

impl BlueRdma {
    /// Updates Memory Translation Table entry
    #[inline]
    fn reg_mr_inner(&mut self, addr: u64, length: usize) -> io::Result<u32> {
        let entry =
            self.mtt
                .register(&mut self.buffer, self.buffer_phys_addr, addr, length, 0, 0)?;
        let mr_key = entry.mr_key;
        self.cmd_queue.update_mtt(entry)?;

        Ok(mr_key)
    }

    /// Updates Queue Pair entry
    #[inline]
    fn update_qp_inner(&self, entry: QpEntry) -> io::Result<()> {
        self.cmd_queue.update_qp(entry)
    }

    /// Sends an RDMA operation
    fn post_send_inner(&mut self, qpn: u32, wr: SendWrResolver) -> io::Result<()> {
        let qp = self
            .qp_table
            .state_mut(qpn)
            .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
        let (builder, msn, base_psn) = qp
            .next_wr(&wr)
            .ok_or(io::Error::from(io::ErrorKind::WouldBlock))?;
        let flags = wr.send_flags();
        if flags & ibverbs_sys::ibv_send_flags::IBV_SEND_SIGNALED.0 != 0 {
            let wr_id = wr.wr_id();
            let send_cq_handle = qp
                .send_cq_handle()
                .ok_or(io::Error::from(io::ErrorKind::InvalidInput))?;
            self.cq_manager.register_event(
                send_cq_handle,
                qpn,
                CompletionEvent::new(qpn, msn, wr_id),
            );
        }

        let fragmenter = WrFragmenter::new(wr, builder, base_psn);
        for chunk in fragmenter {
            // TODO: Should this never fail
            self.send_queue.send(chunk)?;
        }

        Ok(())
    }
}

#[allow(unsafe_code)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
unsafe impl RdmaCtxOps for BlueRdma {
    #[inline]
    fn init() {
        bluesimalloc::setup_allocator!();
    }

    #[inline]
    #[allow(clippy::unwrap_used)]
    #[allow(clippy::as_conversions)] // usize to u64
    fn new(sysfs_name: *const std::ffi::c_char) -> *mut std::ffi::c_void {
        let queue_builder = EmulatedQueueBuilder::new();
        let device_builder = DeviceBuilder::new(queue_builder);
        let page_allocator = EmulatedPageAllocator::new(
            bluesimalloc::shm_start_addr()..bluesimalloc::heap_start_addr(),
        );
        let resolver = PhysAddrResolverEmulated::new(bluesimalloc::shm_start_addr() as u64);
        let network_config = NetworkConfig {
            ip_network: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(127, 0, 0, 1), 24).unwrap()),
            gateway: Ipv4Addr::new(127, 0, 0, 1).into(),
            mac: MacAddress([0x02, 0x42, 0xAC, 0x11, 0x00, 0x02]),
        };
        let mut bluerdma = device_builder
            .initialize(network_config, page_allocator, &resolver, 128, 128)
            .unwrap();
        Box::into_raw(Box::new(bluerdma)).cast()
    }

    #[inline]
    #[allow(clippy::as_conversions)] // provider implementation guarantees pointer validity
    fn free(driver_data: *const std::ffi::c_void) {
        if !driver_data.is_null() {
            unsafe {
                drop(Box::from_raw(driver_data as *mut BlueRdma));
            }
        }
    }

    #[inline]
    fn alloc_pd(blue_context: *mut ibverbs_sys::ibv_context) -> *mut ibverbs_sys::ibv_pd {
        Box::into_raw(Box::new(ibverbs_sys::ibv_pd {
            context: blue_context,
            handle: 0,
        }))
    }

    #[inline]
    fn dealloc_pd(pd: *mut ibverbs_sys::ibv_pd) -> ::std::os::raw::c_int {
        0
    }

    #[inline]
    fn query_device_ex(
        blue_context: *mut ibverbs_sys::ibv_context,
        _input: *const ibverbs_sys::ibv_query_device_ex_input,
        device_attr: *mut ibverbs_sys::ibv_device_attr,
        _attr_size: usize,
    ) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn query_port(
        blue_context: *mut ibverbs_sys::ibv_context,
        port_num: u8,
        port_attr: *mut ibverbs_sys::ibv_port_attr,
    ) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn create_cq(
        blue_context: *mut ibverbs_sys::ibv_context,
        cqe: core::ffi::c_int,
        channel: *mut ibverbs_sys::ibv_comp_channel,
        comp_vector: core::ffi::c_int,
    ) -> *mut ibverbs_sys::ibv_cq {
        todo!()
    }

    #[inline]
    fn destroy_cq(cq: *mut ibverbs_sys::ibv_cq) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn create_qp(
        pd: *mut ibverbs_sys::ibv_pd,
        init_attr: *mut ibverbs_sys::ibv_qp_init_attr,
    ) -> *mut ibverbs_sys::ibv_qp {
        let context = unsafe { *pd }.context;
        let bluerdma = unsafe { get_device_mut(context) };
        let init_attr = unsafe { *init_attr };
        let Some(qpn) = bluerdma.qp_manager.create_qp() else {
            return ptr::null_mut();
        };
        let entry = QpEntry {
            qp_type: init_attr.qp_type as u8,
            qpn,
            ..Default::default()
        };
        bluerdma.update_qp_inner(entry);

        Box::into_raw(Box::new(ibverbs_sys::ibv_qp {
            context,
            qp_context: ptr::null_mut(),
            pd,
            send_cq: ptr::null_mut(),
            recv_cq: ptr::null_mut(),
            srq: ptr::null_mut(),
            handle: 0,
            qp_num: qpn,
            state: ibverbs_sys::ibv_qp_state::IBV_QPS_INIT,
            qp_type: init_attr.qp_type,
            mutex: ibverbs_sys::pthread_mutex_t::default(),
            cond: ibverbs_sys::pthread_cond_t::default(),
            events_completed: 0,
        }))
    }

    #[inline]
    fn destroy_qp(qp: *mut ibverbs_sys::ibv_qp) -> ::std::os::raw::c_int {
        let qp = unsafe { *qp };
        let context = qp.context;
        let bluerdma = unsafe { get_device_mut(context) };
        let qpn = qp.qp_num;
        bluerdma.qp_manager.destroy_qp(qpn);

        0
    }

    #[inline]
    fn modify_qp(
        qp: *mut ibverbs_sys::ibv_qp,
        attr: *mut ibverbs_sys::ibv_qp_attr,
        attr_mask: core::ffi::c_int,
    ) -> ::std::os::raw::c_int {
        let qp = unsafe { *qp };
        let attr = unsafe { *attr };
        let context = qp.context;
        let bluerdma = unsafe { get_device_mut(context) };
        let dgid = unsafe { attr.ah_attr.grh.dgid.raw };
        let ip_addr = u32::from_le_bytes([dgid[12], dgid[13], dgid[14], dgid[15]]);
        let entry = QpEntry {
            ip_addr,
            qpn: qp.qp_num,
            peer_qpn: attr.dest_qp_num,
            rq_access_flags: attr.qp_access_flags as u8,
            qp_type: qp.qp_type as u8,
            pmtu: attr.path_mtu as u8,
            local_udp_port: u16::from(attr.port_num),
            peer_mac_addr: 0,
        };
        bluerdma.update_qp_inner(entry);
        0
    }

    #[inline]
    fn query_qp(
        qp: *mut ibverbs_sys::ibv_qp,
        attr: *mut ibverbs_sys::ibv_qp_attr,
        attr_mask: core::ffi::c_int,
        init_attr: *mut ibverbs_sys::ibv_qp_init_attr,
    ) -> ::std::os::raw::c_int {
        let qp = unsafe { *qp };
        let context = qp.context;
        let bluerdma = unsafe { get_device_mut(context) };
        let Some(_current_attr) = bluerdma.qp_manager.qp_attr(qp.qp_num) else {
            return -1;
        };
        0
    }

    #[inline]
    fn reg_mr(
        pd: *mut ibverbs_sys::ibv_pd,
        addr: *mut ::std::os::raw::c_void,
        length: usize,
        _hca_va: u64,
        access: core::ffi::c_int,
    ) -> *mut ibverbs_sys::ibv_mr {
        let context = unsafe { (*pd) }.context;
        let bluerdma = unsafe { get_device_mut(context) };
        let Ok(mr_key) = bluerdma.reg_mr_inner(addr as u64, length) else {
            return ptr::null_mut();
        };
        let ibv_mr = Box::new(ibverbs_sys::ibv_mr {
            context,
            pd,
            addr,
            length,
            handle: 0, // TODO: implement mr handle
            lkey: mr_key,
            rkey: mr_key,
        });
        Box::into_raw(ibv_mr)
    }

    #[inline]
    fn dereg_mr(mr: *mut ibverbs_sys::ibv_mr) -> ::std::os::raw::c_int {
        if !mr.is_null() {
            let mr = unsafe { Box::from_raw(mr) };
            // TODO: implement dereg mr
            drop(mr);
        }
        0
    }

    #[inline]
    fn post_send(
        qp: *mut ibverbs_sys::ibv_qp,
        wr: *mut ibverbs_sys::ibv_send_wr,
        bad_wr: *mut *mut ibverbs_sys::ibv_send_wr,
    ) -> ::std::os::raw::c_int {
        let qp = unsafe { *qp };
        let wr = unsafe { *wr };
        let context = qp.context;
        let bluerdma = unsafe { get_device_mut(context) };
        let qp_num = qp.qp_num;
        let wr = SendWrResolver::new(wr).unwrap_or_else(|_| todo!("handle invalid input"));
        bluerdma.post_send_inner(qp_num, wr);

        0
    }

    #[inline]
    fn post_recv(
        qp: *mut ibverbs_sys::ibv_qp,
        wr: *mut ibverbs_sys::ibv_recv_wr,
        bad_wr: *mut *mut ibverbs_sys::ibv_recv_wr,
    ) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn poll_cq(
        cq: *mut ibverbs_sys::ibv_cq,
        num_entries: i32,
        wc: *mut ibverbs_sys::ibv_wc,
    ) -> i32 {
        todo!()
    }
}

#[repr(C)]
struct BlueRdmaDevice {
    pad: [u8; 712],
    driver: *mut core::ffi::c_void,
    abi_version: core::ffi::c_int,
}

#[allow(unsafe_code)]
unsafe fn get_device_mut(context: *mut ibverbs_sys::ibv_context) -> &'static mut BlueRdma {
    let dev_ptr = unsafe { *context }.device.cast::<BlueRdmaDevice>();
    unsafe { (*dev_ptr).driver.cast::<BlueRdma>().as_mut() }
        .unwrap_or_else(|| unreachable!("null device pointer"))
}
