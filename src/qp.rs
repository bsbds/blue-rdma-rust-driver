use std::{
    iter,
    sync::{
        atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering},
        Arc,
    },
};

use bitvec::vec::BitVec;
use ibverbs_sys::{ibv_qp, ibv_qp_type::IBV_QPT_RC, ibv_send_wr};
use parking_lot::Mutex;
use rand::Rng;

use crate::{
    constants::{MAX_MSN_WINDOW, MAX_PSN_WINDOW, QPN_KEY_PART_WIDTH},
    device_protocol::{QpParams, WithQpParams, WrChunkBuilder},
    retransmission::{
        ack_msn_tracker::AckMsnTracker, message_tracker::MessageTracker, psn_tracker::PsnTracker,
    },
    send::SendWrResolver,
};

/// Manages QPs
pub(crate) struct QpManager {
    /// Bitmap tracking allocated QPNs
    bitmap: BitVec,
    /// QPN to `DeviceQp` mapping
    qps: Vec<DeviceQp>,
}

#[allow(clippy::as_conversions, clippy::indexing_slicing)]
impl QpManager {
    /// Creates a new `QpManager`
    pub(crate) fn new(max_num_qps: u32) -> Self {
        let size = max_num_qps as usize;
        let mut bitmap = BitVec::with_capacity(size);
        bitmap.resize(size, false);
        Self {
            bitmap,
            qps: iter::repeat_with(DeviceQp::default).take(size).collect(),
        }
    }

    #[allow(clippy::similar_names)]
    pub(crate) fn new_split(&self) -> (QpInitiatorTable, QpTrackerTable) {
        let (initiators, trackers): (Vec<_>, Vec<_>) = self
            .qps
            .iter()
            .map(|qp| {
                let shared = SharedState::default();
                let initiator = InitiatorState {
                    attrs: Arc::clone(&qp.attrs),
                    msn: 0,
                    psn: 0,
                    shared: shared.clone(),
                };
                let tracker = TrackerState {
                    attrs: Arc::clone(&qp.attrs),
                    psn: PsnTracker::new_with_default_base(),
                    ack_msn: AckMsnTracker::default(),
                    message: Arc::clone(&shared.message),
                };
                (initiator, tracker)
            })
            .unzip();

        (
            QpInitiatorTable { table: initiators },
            QpTrackerTable { table: trackers },
        )
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
        let index = qpn_index(qpn);
        if index >= self.max_num_qps() {
            return;
        }
        self.bitmap.set(index, false);
    }

    pub(crate) fn qp_attr(&self, qpn: u32) -> Option<QpAttr> {
        let index = qpn_index(qpn);
        if !self.bitmap.get(index).is_some_and(|x| *x) {
            return None;
        }
        let qp = self.qps.get(index)?;
        Some(*qp.attrs.inner.lock())
    }

    pub(crate) fn update_qp_attr<F: FnMut(&mut QpAttr)>(&self, qpn: u32, mut f: F) -> bool {
        let index = qpn_index(qpn);
        if !self.bitmap.get(index).is_some_and(|x| *x) {
            return false;
        }
        let Some(qp) = self.qps.get(index) else {
            return false;
        };
        f(&mut qp.attrs.inner.lock());
        true
    }

    /// Returns the maximum number of Queue Pairs (QPs) supported
    fn max_num_qps(&self) -> usize {
        self.qps.len()
    }
}

/// Initiator state tacking of all QPs
#[derive(Debug)]
pub(crate) struct QpInitiatorTable {
    /// Vector maps the QPN to the initiator state of each QP
    table: Vec<InitiatorState>,
}

impl QpInitiatorTable {
    #[allow(clippy::as_conversions)] // convert u32 to usize
    /// Gets a mutable reference to the QP associated with the given QPN
    pub(crate) fn state_mut(&mut self, qpn: u32) -> Option<&mut InitiatorState> {
        self.table.get_mut(qpn_index(qpn))
    }
}

/// Message state tacking of all QPs
pub(crate) struct QpTrackerTable {
    /// Vector maps the QPN to the tracking state of each QP
    table: Vec<TrackerState>,
}

impl QpTrackerTable {
    #[allow(clippy::as_conversions)] // convert u32 to usize
    /// Gets a mutable reference to the QP associated with the given QPN
    pub(crate) fn state_mut(&mut self, qpn: u32) -> Option<&mut TrackerState> {
        self.table.get_mut(qpn_index(qpn))
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct QpAttr {
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

#[derive(Default, Debug)]
struct QpAttrShared {
    inner: Mutex<QpAttr>,
}

impl QpAttrShared {
    pub(crate) fn qp_type(&self) -> u8 {
        self.inner.lock().qp_type
    }

    pub(crate) fn set_qp_type(&self, value: u8) {
        self.inner.lock().qp_type = value;
    }

    pub(crate) fn qpn(&self) -> u32 {
        self.inner.lock().qpn
    }

    pub(crate) fn set_qpn(&self, value: u32) {
        self.inner.lock().qpn = value;
    }

    pub(crate) fn dqpn(&self) -> u32 {
        self.inner.lock().dqpn
    }

    pub(crate) fn set_dqpn(&self, value: u32) {
        self.inner.lock().dqpn = value;
    }

    pub(crate) fn dqp_ip(&self) -> u32 {
        self.inner.lock().dqp_ip
    }

    pub(crate) fn set_dqp_ip(&self, value: u32) {
        self.inner.lock().dqp_ip = value;
    }

    pub(crate) fn mac_addr(&self) -> u64 {
        self.inner.lock().mac_addr
    }

    pub(crate) fn set_mac_addr(&self, value: u64) {
        self.inner.lock().mac_addr = value;
    }

    pub(crate) fn pmtu(&self) -> u8 {
        self.inner.lock().pmtu
    }

    pub(crate) fn set_pmtu(&self, value: u8) {
        self.inner.lock().pmtu = value;
    }

    pub(crate) fn access_flags(&self) -> u8 {
        self.inner.lock().access_flags
    }

    pub(crate) fn set_access_flags(&self, value: u8) {
        self.inner.lock().access_flags = value;
    }

    pub(crate) fn send_cq(&self) -> Option<u32> {
        self.inner.lock().send_cq
    }

    pub(crate) fn set_send_cq(&self, value: Option<u32>) {
        self.inner.lock().send_cq = value;
    }

    pub(crate) fn recv_cq(&self) -> Option<u32> {
        self.inner.lock().recv_cq
    }

    pub(crate) fn set_recv_cq(&self, value: Option<u32>) {
        self.inner.lock().recv_cq = value;
    }
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Default, Debug, Clone)]
/// A queue pair for building work requests
pub(crate) struct DeviceQp {
    attrs: Arc<QpAttrShared>,
}

impl DeviceQp {
    /// Returns the QPN of this QP
    pub(crate) fn qpn(&self) -> u32 {
        self.attrs.qpn()
    }
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug)]
pub(crate) struct InitiatorState {
    attrs: Arc<QpAttrShared>,
    /// Current MSN
    msn: u16,
    /// Current PSN
    psn: u32,
    /// Shared state
    shared: SharedState,
}

/// Shared state between Queue Pairs (QPs) containing atomic counters
/// for packet sequence numbers and message sequence numbers.
#[derive(Default, Debug, Clone)]
struct SharedState {
    /// Current base PSN
    base_psn: Arc<AtomicU32>,
    /// Current base MSN
    base_msn: Arc<AtomicU16>,
    /// Message ack tracker info
    message: Arc<Mutex<MessageTracker>>,
}

impl SharedState {
    /// Returns the base PSN
    fn base_psn(&self) -> u32 {
        self.base_psn.load(Ordering::Acquire)
    }

    /// Returns the base MSN
    fn base_msn(&self) -> u16 {
        self.base_msn.load(Ordering::Acquire)
    }
}

impl InitiatorState {
    /// Returns the next wr
    pub(crate) fn next_wr(
        &mut self,
        wr: &SendWrResolver,
    ) -> Option<(WrChunkBuilder<WithQpParams>, u16, u32, u32)> {
        let num_psn = num_psn(self.attrs.pmtu(), wr.raddr(), wr.length())?;
        let (msn, base_psn) = self.next(num_psn)?;
        let end_psn = base_psn.wrapping_add(num_psn).wrapping_sub(1);

        Some((
            WrChunkBuilder::new().set_qp_params(QpParams::new(
                msn,
                self.attrs.qp_type(),
                self.attrs.qpn(),
                self.attrs.mac_addr(),
                self.attrs.dqpn(),
                self.attrs.dqp_ip(),
                self.attrs.pmtu(),
            )),
            msn,
            base_psn,
            end_psn,
        ))
    }

    pub(crate) fn insert_messsage(&self, msn: u16, end_psn: u32) {
        self.shared.message.lock().insert(msn, false, end_psn);
    }

    /// Returns the send cq handle.
    pub(crate) fn send_cq_handle(&self) -> Option<u32> {
        self.attrs.send_cq()
    }

    /// Returns the recv cq handle.
    pub(crate) fn recv_cq_handle(&self) -> Option<u32> {
        self.attrs.recv_cq()
    }

    /// Get the next MSN and PSN pair for a new request.
    ///
    /// # Arguments
    ///
    /// * `num_psn` - Number of PSNs needed for this request
    ///
    /// # Returns
    ///
    /// * `Some((msn, psn))` - The MSN and PSN pair for the new request
    /// * `None` - If there is not enough PSN window available or MSN has wrapped around
    #[allow(clippy::similar_names)] // name is clear
    #[allow(clippy::as_conversions)] // convert u32 to usize
    fn next(&mut self, num_psn: u32) -> Option<(u16, u32)> {
        let base_psn = self.shared.base_psn();
        let base_msn = self.shared.base_msn();
        let outstanding_num_psn = self.psn.saturating_sub(base_psn);
        let outstanding_num_msn = self.msn.saturating_sub(base_msn);
        if outstanding_num_psn.saturating_add(num_psn) as usize > MAX_PSN_WINDOW
            || outstanding_num_msn.saturating_add(1) as usize > MAX_MSN_WINDOW
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
}

/// Qp state for maintaining message trackers
pub(crate) struct TrackerState {
    attrs: Arc<QpAttrShared>,
    /// Tracker for tracking acked PSNs
    psn: PsnTracker,
    /// Tracker for tracking message sequence number of ACK packets
    ack_msn: AckMsnTracker,
    /// Message ack tracker info
    message: Arc<Mutex<MessageTracker>>,
}

impl TrackerState {
    /// Acknowledges a single PSN.
    pub(crate) fn ack_one(&mut self, psn: u32) {
        let _ignore = self.psn.ack_one(psn);
    }

    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    pub(crate) fn ack_range(&mut self, base_psn: u32, bitmap: u128, ack_msn: u16) -> Option<u32> {
        let mut acked_psn = None;
        if self.ack_msn.ack(ack_msn).is_some() {
            acked_psn = self.psn.ack_before(base_psn);
        }
        if let Some(psn) = self.psn.ack_range(base_psn, bitmap) {
            acked_psn = Some(psn);
        }
        acked_psn
    }

    /// Returns `true` if all PSNs up to and including the given PSN have been acknowledged.
    pub(crate) fn all_acked(&self, psn: u32) -> bool {
        self.psn.all_acked(psn)
    }

    pub(crate) fn insert_messsage(&self, msn: u16, ack_req: bool, end_psn: u32) {
        self.message.lock().insert(msn, ack_req, end_psn);
    }

    /// Returns a mutable reference to the message tracker associated with this QP.
    pub(crate) fn ack_message(&self, base_psn: u32) -> Vec<(u16, bool)> {
        self.message.lock().ack(base_psn)
    }

    /// Calculate the number of psn required for this WR
    pub(crate) fn num_psn(&self, addr: u64, length: u32) -> Option<u32> {
        let pmtu = convert_ibv_mtu_to_u16(self.attrs.pmtu())?;
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

    /// Returns the send cq handle.
    pub(crate) fn send_cq_handle(&self) -> Option<u32> {
        self.attrs.send_cq()
    }

    /// Returns the recv cq handle.
    pub(crate) fn recv_cq_handle(&self) -> Option<u32> {
        self.attrs.recv_cq()
    }

    /// Returns the QPN of this QP
    pub(crate) fn qpn(&self) -> u32 {
        self.attrs.qpn()
    }

    /// Returns the Destination QPN of this QP
    pub(crate) fn dqpn(&self) -> u32 {
        self.attrs.dqpn()
    }
}

#[allow(clippy::as_conversions)] // u32 to usize
pub(crate) fn qpn_index(qpn: u32) -> usize {
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
