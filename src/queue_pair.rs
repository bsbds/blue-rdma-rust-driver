use std::{
    iter,
    sync::{
        atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering},
        Arc,
    },
};

use bitvec::vec::BitVec;
use ibverbs_sys::{ibv_qp, ibv_qp_type::IBV_QPT_RC, ibv_send_wr};
use parking_lot::{Mutex, RwLock};
use rand::Rng;

use crate::{
    constants::{MAX_MSN_WINDOW, MAX_PSN_WINDOW, MAX_QP_CNT, MAX_SEND_WR, QPN_KEY_PART_WIDTH},
    device_protocol::{WithQpParams, WrChunkBuilder},
    retransmission::{
        ack_msn_tracker::AckMsnTracker, message_tracker::MessageTracker, psn_tracker::PsnTracker,
    },
    send::SendWrResolver,
    tracker::{AckTracker, Msn, PacketTracker},
};

#[derive(Default, Clone, Copy)]
pub(crate) struct QueuePairAttr {
    pub(crate) qp_type: u8,
    pub(crate) qpn: u32,
    pub(crate) dqpn: u32,
    pub(crate) dqp_ip: u32,
    pub(crate) mac_addr: u64,
    pub(crate) pmtu: u8,
    pub(crate) access_flags: u8,
    pub(crate) send_cq: Option<u32>,
    pub(crate) recv_cq: Option<u32>,
}

pub(crate) struct QueuePairAttrTable {
    inner: Arc<[RwLock<QueuePairAttr>]>,
}

impl QueuePairAttrTable {
    pub(crate) fn new() -> Self {
        Self {
            inner: iter::repeat_with(RwLock::default)
                .take(MAX_QP_CNT)
                .collect(),
        }
    }

    pub(crate) fn clone_arc(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    pub(crate) fn get(&self, qpn: u32) -> Option<QueuePairAttr> {
        let index = index(qpn);
        self.inner.get(index).map(|x| *x.read())
    }

    pub(crate) fn map_qp<F, T>(&self, qpn: u32, mut f: F) -> Option<T>
    where
        F: FnMut(&QueuePairAttr) -> T,
    {
        let index = index(qpn);
        self.inner.get(index).map(|x| f(&x.read()))
    }

    pub(crate) fn map_qp_mut<F, T>(&self, qpn: u32, mut f: F) -> Option<T>
    where
        F: FnMut(&mut QueuePairAttr) -> T,
    {
        let index = index(qpn);
        self.inner.get(index).map(|x| f(&mut x.write()))
    }
}

/// Manages QPs
pub(crate) struct QpManager {
    /// Bitmap tracking allocated QPNs
    bitmap: BitVec,
    /// QP table
    table: QueuePairAttrTable,
}

#[allow(clippy::as_conversions, clippy::indexing_slicing)]
impl QpManager {
    /// Creates a new `QpManager`
    pub(crate) fn new(table: QueuePairAttrTable) -> Self {
        let mut bitmap = BitVec::with_capacity(MAX_QP_CNT);
        bitmap.resize(MAX_QP_CNT, false);
        Self { bitmap, table }
    }

    /// Allocates a new QP and returns its QPN
    #[allow(clippy::cast_possible_truncation)] // no larger than u32
    pub(crate) fn create_qp(&mut self) -> Option<u32> {
        let index = self.bitmap.first_zero()? as u32;
        let key = rand::thread_rng().gen_range(0..1 << QPN_KEY_PART_WIDTH);
        self.bitmap.set(index as usize, true);
        let qpn = index << QPN_KEY_PART_WIDTH | key;
        Some(qpn)
    }

    /// Removes and returns the QP associated with the given QPN
    pub(crate) fn destroy_qp(&mut self, qpn: u32) {
        let index = index(qpn);
        if index >= MAX_QP_CNT {
            return;
        }
        self.bitmap.set(index, false);
    }

    pub(crate) fn get_qp(&self, qpn: u32) -> Option<QueuePairAttr> {
        let index = index(qpn);
        if !self.bitmap.get(index).is_some_and(|x| *x) {
            return None;
        }
        self.table.get(qpn)
    }

    pub(crate) fn update_qp<F: FnMut(&mut QueuePairAttr)>(&self, qpn: u32, mut f: F) -> bool {
        let index = index(qpn);
        if !self.bitmap.get(index).is_some_and(|x| *x) {
            return false;
        }
        if self.table.map_qp_mut(qpn, f).is_none() {
            return false;
        }
        true
    }
}

pub(crate) struct SenderTable {
    inner: Box<[Mutex<Sender>]>,
}

pub(crate) struct Sender {
    msn: u16,
    psn: u32,
    base_psn_acked: u32,
    base_msn_acked: u16,
}

impl Sender {
    #[allow(clippy::similar_names)]
    fn next_wr(&mut self, num_psn: u32) -> Option<(u16, u32)> {
        let outstanding_num_psn = self.psn.wrapping_sub(self.base_psn_acked);
        let outstanding_num_msn = self.msn.wrapping_sub(self.base_msn_acked);
        if outstanding_num_psn.saturating_add(num_psn) as usize > MAX_PSN_WINDOW
            || outstanding_num_msn as usize >= MAX_SEND_WR
        {
            return None;
        }
        let current_psn = self.psn;
        let current_msn = self.msn;
        let next_msn = self.msn.wrapping_add(1);
        let next_psn = self.psn.wrapping_add(num_psn);
        self.msn = next_msn;
        self.psn = next_psn;

        Some((current_msn, current_psn))
    }

    fn update_psn_acked(&mut self, psn: u32) {
        self.base_psn_acked = psn;
    }

    fn update_msn_acked(&mut self, msn: u16) {
        self.base_msn_acked = msn;
    }
}

pub(crate) struct TrackerTable {
    inner: Box<[Tracker]>,
}

pub(crate) struct Tracker {
    psn: PacketTracker,
    ack: AckTracker,
}

impl Tracker {
    /// Acknowledges a single PSN.
    pub(crate) fn ack_one(&mut self, psn: u32) {
        let _ignore = self.psn.ack_one(psn);
    }

    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    pub(crate) fn ack_range(&mut self, base_psn: u32, bitmap: u128, ack_msn: u16) -> Option<u32> {
        let mut acked_psn = None;
        if self.ack.ack(Msn(ack_msn)).is_some() {
            acked_psn = self.psn.ack_before(base_psn);
        }
        if let Some(psn) = self.psn.ack_range(base_psn, bitmap) {
            acked_psn = Some(psn);
        }
        acked_psn
    }

    pub(crate) fn base_psn(&self) -> u32 {
        self.psn.base_psn()
    }
}

#[allow(clippy::as_conversions)] // u32 to usize
fn index(qpn: u32) -> usize {
    (qpn >> QPN_KEY_PART_WIDTH) as usize
}

/// Calculate the number of psn required for this WR
fn num_psn(pmtu: u8, addr: u64, length: u32) -> Option<u32> {
    let pmtu = convert_ibv_mtu_to_u16(pmtu)?;
    let pmtu_mask = pmtu
        .checked_sub(1)
        .unwrap_or_else(|| unreachable!("pmtu should be greater than 1"));
    let next_align_addr = addr.saturating_add(u64::from(pmtu_mask)) & !u64::from(pmtu_mask);
    let gap = next_align_addr.saturating_sub(addr);
    let length_u64 = u64::from(length);
    length_u64
        .checked_sub(gap)
        .unwrap_or(length_u64)
        .div_ceil(u64::from(pmtu))
        .try_into()
        .ok()
}

pub(crate) fn convert_ibv_mtu_to_u16(ibv_mtu: u8) -> Option<u16> {
    let pmtu = match u32::from(ibv_mtu) {
        ibverbs_sys::IBV_MTU_256 => 256,
        ibverbs_sys::IBV_MTU_512 => 512,
        ibverbs_sys::IBV_MTU_1024 => 1024,
        ibverbs_sys::IBV_MTU_2048 => 2048,
        ibverbs_sys::IBV_MTU_4096 => 4096,
        _ => return None,
    };
    Some(pmtu)
}
