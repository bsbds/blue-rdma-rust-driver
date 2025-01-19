use bitvec::vec::BitVec;
use ibverbs_sys::{ibv_qp, ibv_qp_type::IBV_QPT_RC, ibv_send_wr};

use crate::{
    queue::abstr::{WithQpParams, WrChunkBuilder},
    retransmission::{ack_msn_tracker::AckMsnTracker, psn_tracker::PsnTracker},
    send::SendWrResolver,
};

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

    state: State,
}

impl DeviceQp {
    /// Creates a new RC QP
    #[allow(clippy::as_conversions, clippy::cast_possible_truncation)] // qp_type should smaller than u8::MAX
    pub(crate) fn new_rc(qpn: u32, pmtu: u8, dqpn: u32, dqp_ip: u32, mac_addr: u64) -> Self {
        Self {
            qp_type: IBV_QPT_RC as u8,
            qpn,
            dqpn,
            dqp_ip,
            mac_addr,
            pmtu,
            state: State::default(),
        }
    }

    /// Returns the next wr
    pub(crate) fn next_wr(
        &mut self,
        wr: &SendWrResolver,
    ) -> Option<(WrChunkBuilder<WithQpParams>, u32)> {
        let num_psn = self.num_psn(wr.raddr(), wr.length())?;
        let (msn, base_psn) = self.state.next(num_psn)?;

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

    /// Acknowledges a single PSN.
    pub(crate) fn ack_one(&mut self, psn: u32) {
        let _ignore = self.state.psn_tracker.ack_one(psn);
    }

    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    pub(crate) fn ack_range(&mut self, base_psn: u32, bitmap: u128, ack_msn: u16) {
        if self.state.ack_msn_tracker.ack(ack_msn).is_some() {
            let _ignore = self.state.psn_tracker.ack_before(base_psn);
        }
        let _ignore = self.state.psn_tracker.ack_range(base_psn, bitmap);
    }

    /// Returns `true` if all PSNs up to and including the given PSN have been acknowledged.
    pub(crate) fn all_acked(&self, psn: u32) -> bool {
        self.state.psn_tracker.all_acked(psn)
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
}

/// Device Qp state
#[derive(Default, Debug, Clone)]
struct State {
    /// Current MSN
    msn: u16,
    /// Current PSN
    psn: u32,
    /// Last completed PSN
    last_comp_msn: u16,
    /// Last completed PSN
    last_comp_psn: u32,
    /// Tracker for tracking acked PSNs
    psn_tracker: PsnTracker,
    /// Tracker for tracking message sequence number of ACK packets
    ack_msn_tracker: AckMsnTracker,
}

impl State {
    /// FIXME: check `last_comp_psn`
    #[allow(clippy::similar_names)] // name is clear
    fn next(&mut self, num_psn: u32) -> Option<(u16, u32)> {
        let current_psn = self.psn;
        let current_msn = self.msn;
        let next_msn = self.msn.wrapping_add(1);
        (next_msn != self.last_comp_msn).then(|| {
            self.msn = next_msn;
            self.psn = self.psn.wrapping_add(num_psn);
            (current_msn, current_psn)
        })
    }

    /// Sets the last completed MSN
    fn set_last_comp_msn(&mut self, msn: u16) {
        self.last_comp_msn = msn;
    }

    /// Sets the last completed PSN
    fn set_last_comp_psn(&mut self, psn: u32) {
        self.last_comp_psn = psn;
    }
}
