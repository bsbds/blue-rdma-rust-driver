#![allow(missing_docs)]

use core::ffi::{c_char, c_void, CStr};
use core::ptr::{self, NonNull};
use ibverbs_sys as ffi;

/// RDMA context operations for Blue-RDMA driver.
///
/// # Safety
/// Implementors must ensure all FFI and RDMA verbs specification requirements are met,
pub unsafe trait RdmaCtxOps {
    fn init();

    #[allow(clippy::new_ret_no_self)]
    /// Safety: caller must ensure `sysfs_name` is a valid pointer
    fn new(sysfs_name: *const c_char) -> *mut c_void;

    fn free(driver_data: *const c_void);

    fn alloc_pd(blue_context: *mut ffi::ibv_context) -> *mut ffi::ibv_pd;

    fn dealloc_pd(pd: *mut ffi::ibv_pd) -> ::std::os::raw::c_int;

    fn query_device_ex(
        blue_context: *mut ffi::ibv_context,
        _input: *const ffi::ibv_query_device_ex_input,
        device_attr: *mut ffi::ibv_device_attr,
        _attr_size: usize,
    ) -> ::std::os::raw::c_int;

    fn query_port(
        blue_context: *mut ffi::ibv_context,
        port_num: u8,
        port_attr: *mut ffi::ibv_port_attr,
    ) -> ::std::os::raw::c_int;

    fn create_cq(
        blue_context: *mut ffi::ibv_context,
        cqe: core::ffi::c_int,
        channel: *mut ffi::ibv_comp_channel,
        comp_vector: core::ffi::c_int,
    ) -> *mut ffi::ibv_cq;

    fn destroy_cq(cq: *mut ffi::ibv_cq) -> ::std::os::raw::c_int;

    fn create_qp(pd: *mut ffi::ibv_pd, init_attr: *mut ffi::ibv_qp_init_attr) -> *mut ffi::ibv_qp;

    fn destroy_qp(qp: *mut ffi::ibv_qp) -> ::std::os::raw::c_int;

    fn modify_qp(
        qp: *mut ffi::ibv_qp,
        attr: *mut ffi::ibv_qp_attr,
        attr_mask: core::ffi::c_int,
    ) -> ::std::os::raw::c_int;

    fn query_qp(
        qp: *mut ffi::ibv_qp,
        attr: *mut ffi::ibv_qp_attr,
        attr_mask: core::ffi::c_int,
        init_attr: *mut ffi::ibv_qp_init_attr,
    ) -> ::std::os::raw::c_int;

    fn reg_mr(
        pd: *mut ffi::ibv_pd,
        addr: *mut ::std::os::raw::c_void,
        length: usize,
        _hca_va: u64,
        access: core::ffi::c_int,
    ) -> *mut ffi::ibv_mr;

    fn dereg_mr(mr: *mut ffi::ibv_mr) -> ::std::os::raw::c_int;

    fn post_send(
        qp: *mut ffi::ibv_qp,
        wr: *mut ffi::ibv_send_wr,
        bad_wr: *mut *mut ffi::ibv_send_wr,
    ) -> ::std::os::raw::c_int;

    fn post_recv(
        qp: *mut ffi::ibv_qp,
        wr: *mut ffi::ibv_recv_wr,
        bad_wr: *mut *mut ffi::ibv_recv_wr,
    ) -> ::std::os::raw::c_int;

    fn poll_cq(cq: *mut ffi::ibv_cq, num_entries: i32, wc: *mut ffi::ibv_wc) -> i32;
}
