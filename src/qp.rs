use bitvec::vec::BitVec;
use ibverbs_sys::{ibv_qp, ibv_qp_type::IBV_QPT_RC, ibv_send_wr};

use crate::{
    queue::abstr::{WithQpParams, WrChunkBuilder},
    send::SendWrResolver,
};

#[allow(clippy::missing_docs_in_private_items)]
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
        let num_psn = Self::num_psn(self.pmtu, wr.laddr(), wr.length())?;
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
}

/// Device Qp state
#[derive(Default)]
struct State {
    /// Current MSN
    msn: u16,
    /// Current PSN
    psn: u32,
    /// Last completed PSN
    last_comp_msn: u16,
    /// Last completed PSN
    last_comp_psn: u32,
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

/// A pool for managing queue pair numbers (QPN).
struct QpPool {
    /// Bitmap tracking allocated QPNs
    bitmap: BitVec,
}

#[allow(clippy::as_conversions, clippy::cast_possible_truncation)] // u32 to usize
impl QpPool {
    /// Creates a new `QpPool`
    pub(crate) fn new(max_qps: u32) -> Self {
        let mut bitmap = BitVec::with_capacity(max_qps as usize);
        bitmap.resize(max_qps as usize, false);
        QpPool { bitmap }
    }

    /// Allocates a new queue pair number.
    pub(crate) fn allocate_qpn(&mut self) -> Option<u32> {
        let pos = self.bitmap.first_zero()?;
        self.bitmap.set(pos, true);
        Some(pos as u32)
    }

    /// Frees a previously allocated queue pair number.
    pub(crate) fn free_qpn(&mut self, qpn: u32) {
        self.bitmap.set(qpn as usize, false);
    }
}
