use std::sync::{
    atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering},
    Arc,
};

use bitvec::vec::BitVec;
use ibverbs_sys::{ibv_qp, ibv_qp_type::IBV_QPT_RC, ibv_send_wr};

use crate::{
    constants::{MAX_MSN_WINDOW, MAX_PSN_WINDOW},
    queue::abstr::{WithQpParams, WrChunkBuilder},
    retransmission::{
        ack_msn_tracker::AckMsnTracker, message_tracker::MessageTracker, psn_tracker::PsnTracker,
    },
    send::SendWrResolver,
};
/// Initiator state tacking of all QPs
pub(crate) struct QpInitiators {
    /// Vector maps the QPN to the initiator state of each QP
    qps: Vec<InitiatorState>,
}

impl QpInitiators {
    /// Creates a new `QpInitiators`
    pub(crate) fn new() -> Self {
        todo!()
    }

    #[allow(clippy::as_conversions)] // convert u32 to usize
    /// Gets a mutable reference to the QP associated with the given QPN
    pub(crate) fn state_mut(&mut self, qpn: u32) -> Option<&mut InitiatorState> {
        self.qps.get_mut(qpn as usize)
    }
}

/// Message state tacking of all QPs
pub(crate) struct QpTrackers {
    /// Vector maps the QPN to the tracking state of each QP
    qps: Vec<TrackerState>,
}

impl QpTrackers {
    #[allow(clippy::as_conversions)] // convert u32 to usize
    /// Gets a mutable reference to the QP associated with the given QPN
    pub(crate) fn state_mut(&mut self, qpn: u32) -> Option<&mut TrackerState> {
        self.qps.get_mut(qpn as usize)
    }
}

/// Manages QPs
pub(crate) struct QpManager {
    /// Bitmap tracking allocated QPNs
    bitmap: BitVec,
    /// QPN to `DeviceQp` mapping
    qps: Vec<Option<DeviceQp>>,
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
            qps: vec![None; size],
        }
    }

    /// Allocates a new QP and returns its QPN
    #[allow(clippy::cast_possible_truncation)] // no larger than u32
    pub(crate) fn create_qp(&mut self, qp: DeviceQp) -> Option<u32> {
        let qpn = self.bitmap.first_zero()? as u32;
        self.bitmap.set(qpn as usize, true);
        self.qps[qpn as usize] = Some(qp);
        Some(qpn)
    }

    /// Removes and returns the QP associated with the given QPN
    pub(crate) fn destroy_qp(&mut self, qpn: u32) -> Option<DeviceQp> {
        if qpn as usize >= self.max_num_qps() {
            return None;
        }
        self.bitmap.set(qpn as usize, false);
        self.qps[qpn as usize].take()
    }

    /// Gets a reference to the QP associated with the given QPN
    pub(crate) fn get_qp(&self, qpn: u32) -> Option<&DeviceQp> {
        if qpn as usize >= self.max_num_qps() {
            return None;
        }
        self.qps[qpn as usize].as_ref()
    }

    /// Gets a mutable reference to the QP associated with the given QPN
    pub(crate) fn get_qp_mut(&mut self, qpn: u32) -> Option<&mut DeviceQp> {
        if qpn as usize >= self.max_num_qps() {
            return None;
        }
        self.qps[qpn as usize].as_mut()
    }

    /// Returns the maximum number of Queue Pairs (QPs) supported
    fn max_num_qps(&self) -> usize {
        self.qps.len()
    }
}

#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Clone)]
/// A queue pair for building work requests
pub(crate) struct DeviceQp {
    qp_type: u8,
    qpn: u32,
    dqpn: u32,
    dqp_ip: u32,
    mac_addr: u64,
    pmtu: u8,
    send_cq: Option<u32>,
    recv_cq: Option<u32>,
    state: State,
}

impl DeviceQp {
    /// Creates a new RC QP
    #[allow(clippy::as_conversions, clippy::cast_possible_truncation)] // qp_type should smaller than u8::MAX
    pub(crate) fn new_rc(
        qpn: u32,
        pmtu: u8,
        dqpn: u32,
        dqp_ip: u32,
        mac_addr: u64,
        send_cq: Option<u32>,
        recv_cq: Option<u32>,
    ) -> Self {
        Self {
            qp_type: IBV_QPT_RC as u8,
            qpn,
            dqpn,
            dqp_ip,
            mac_addr,
            pmtu,
            send_cq,
            recv_cq,
            state: State::default(),
        }
    }

    /// Returns the QPN of this QP
    pub(crate) fn qpn(&self) -> u32 {
        self.qpn
    }
}

/// Device Qp state
#[derive(Default, Debug, Clone)]
struct State {
    /// Current MSN
    msn: u16,
    /// Current PSN
    psn: u32,
    /// Tracker for tracking acked PSNs
    psn_tracker: PsnTracker,
    /// Tracker for tracking message sequence number of ACK packets
    ack_msn_tracker: AckMsnTracker,
    /// Message ack tracker info
    message_tracker: MessageTracker,
}

pub(crate) struct InitiatorState {
    qp_type: u8,
    qpn: u32,
    dqpn: u32,
    dqp_ip: u32,
    mac_addr: u64,
    pmtu: u8,
    send_cq: Option<u32>,
    recv_cq: Option<u32>,

    /// Current MSN
    msn: u16,
    /// Current PSN
    psn: u32,
    /// Shared state
    shared: SharedState,
}

/// Shared state between Queue Pairs (QPs) containing atomic counters
/// for packet sequence numbers and message sequence numbers.
#[derive(Debug)]
struct SharedState {
    /// Current base PSN
    base_psn: Arc<AtomicU32>,
    /// Current base MSN
    base_msn: Arc<AtomicU16>,
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
    ) -> Option<(WrChunkBuilder<WithQpParams>, u32)> {
        let num_psn = num_psn(self.pmtu, wr.raddr(), wr.length())?;
        let (msn, base_psn) = self.next(num_psn)?;

        Some((
            WrChunkBuilder::new().set_qp_params(
                msn,
                self.qp_type,
                self.qpn,
                self.mac_addr,
                self.dqpn,
                self.dqp_ip,
                self.pmtu,
            ),
            base_psn,
        ))
    }

    /// Returns the send cq handle.
    pub(crate) fn send_cq_handle(&self) -> Option<u32> {
        self.send_cq
    }

    /// Returns the recv cq handle.
    pub(crate) fn recv_cq_handle(&self) -> Option<u32> {
        self.recv_cq
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

pub(crate) struct TrackerState {
    /// Tracker for tracking acked PSNs
    psn: PsnTracker,
    /// Tracker for tracking message sequence number of ACK packets
    ack_msn: AckMsnTracker,
    /// Message ack tracker info
    message: MessageTracker,

    // Original qp states
    /// QPN
    qpn: u32,
    /// Current PMTU
    pmtu: u8,
    /// Send CQ handle
    send_cq: Option<u32>,
    /// Recv CQ handle
    recv_cq: Option<u32>,
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

    /// Returns a mutable reference to the message tracker associated with this QP.
    pub(crate) fn message_tracker(&mut self) -> &mut MessageTracker {
        &mut self.message
    }

    /// Calculate the number of psn required for this WR
    pub(crate) fn num_psn(&self, addr: u64, length: u32) -> Option<u32> {
        let pmtu_mask = self
            .pmtu
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("pmtu should be greater than 1"));
        let next_align_addr = addr.saturating_add(u64::from(pmtu_mask)) & !u64::from(pmtu_mask);
        let gap = next_align_addr.saturating_sub(addr);
        let length_u64 = u64::from(length);
        length_u64
            .checked_sub(gap)
            .unwrap_or(length_u64)
            .div_ceil(u64::from(self.pmtu))
            .try_into()
            .ok()
    }

    /// Returns the send cq handle.
    pub(crate) fn send_cq_handle(&self) -> Option<u32> {
        self.send_cq
    }

    /// Returns the recv cq handle.
    pub(crate) fn recv_cq_handle(&self) -> Option<u32> {
        self.recv_cq
    }

    /// Returns the QPN of this QP
    pub(crate) fn qpn(&self) -> u32 {
        self.qpn
    }
}

/// Calculate the number of psn required for this WR
fn num_psn(pmtu: u8, addr: u64, length: u32) -> Option<u32> {
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
