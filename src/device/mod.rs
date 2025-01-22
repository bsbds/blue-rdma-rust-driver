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
    sync::{atomic::AtomicBool, Arc, OnceLock},
};

use parking_lot::Mutex;
use proxy::DeviceProxy;

use crate::{
    completion::{CompletionEvent, CqManager, EventRegistry, MetaCqTable},
    ctx_ops::RdmaCtxOps,
    mem::{
        page::{ContiguousPages, PageAllocator},
        virt_to_phy::VirtToPhys,
    },
    meta_worker,
    mtt::v2::Mttv2,
    net::{config::NetworkConfig, tap::TapDevice},
    qp::{DeviceQp, QpInitiatorTable, QpManager, QpTrackerTable},
    queue::abstr::{
        DeviceCommand, MetaReport, MttEntry, QPEntry, RecvBuffer, RecvBufferMeta, SimpleNicTunnel,
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
    ) -> io::Result<BlueRdmaDevice>
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

        Ok(BlueRdmaDevice {
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
pub struct BlueRdmaDevice {
    pub(crate) cmd_queue: Box<dyn DeviceCommand + Send + 'static>,
    pub(crate) send_queue: Box<dyn WorkReqSend + Send + 'static>,
    pub(crate) qp_manager: QpManager,
    pub(crate) cq_manager: CqManager,
    pub(crate) qp_table: QpInitiatorTable,
    pub(crate) mtt: Mttv2,
    buffer: ContiguousPages<1>,
    buffer_phys_addr: u64,
}

impl BlueRdmaDevice {
    /// Updates Memory Translation Table entry
    #[inline]
    pub fn reg_mr_inner(&mut self, addr: u64, length: usize) -> io::Result<()> {
        let entry =
            self.mtt
                .register(&mut self.buffer, self.buffer_phys_addr, addr, length, 0, 0)?;
        self.cmd_queue.update_mtt(entry)
    }

    /// Updates Queue Pair entry
    #[inline]
    pub fn update_qp_inner(&self, qpn: u32) -> io::Result<()> {
        let entry = QPEntry {
            qpn,
            ..Default::default()
        };
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

static INSTANCE: OnceLock<Mutex<BlueRdmaDevice>> = OnceLock::new();

#[allow(unsafe_code)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
unsafe impl RdmaCtxOps for BlueRdmaDevice {
    #[inline]
    fn init() {
        todo!()
    }

    #[inline]
    fn new(sysfs_name: *const std::ffi::c_char) -> *mut std::ffi::c_void {
        todo!()
    }

    #[inline]
    fn free(driver_data: *const std::ffi::c_void) {
        todo!()
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
        todo!()
    }

    #[inline]
    fn destroy_qp(qp: *mut ibverbs_sys::ibv_qp) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn modify_qp(
        qp: *mut ibverbs_sys::ibv_qp,
        attr: *mut ibverbs_sys::ibv_qp_attr,
        attr_mask: core::ffi::c_int,
    ) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn query_qp(
        qp: *mut ibverbs_sys::ibv_qp,
        attr: *mut ibverbs_sys::ibv_qp_attr,
        attr_mask: core::ffi::c_int,
        init_attr: *mut ibverbs_sys::ibv_qp_init_attr,
    ) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn reg_mr(
        pd: *mut ibverbs_sys::ibv_pd,
        addr: *mut ::std::os::raw::c_void,
        length: usize,
        _hca_va: u64,
        access: core::ffi::c_int,
    ) -> *mut ibverbs_sys::ibv_mr {
        todo!()
    }

    #[inline]
    fn dereg_mr(mr: *mut ibverbs_sys::ibv_mr) -> ::std::os::raw::c_int {
        todo!()
    }

    #[inline]
    fn post_send(
        qp: *mut ibverbs_sys::ibv_qp,
        wr: *mut ibverbs_sys::ibv_send_wr,
        bad_wr: *mut *mut ibverbs_sys::ibv_send_wr,
    ) -> ::std::os::raw::c_int {
        unsafe {
            let qp_num = (*qp).qp_num;
            let wr = SendWrResolver::new(*wr).unwrap_or_else(|_| todo!("handle invalid input"));
        }
        todo!()
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
