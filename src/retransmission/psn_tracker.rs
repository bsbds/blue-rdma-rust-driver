use bitvec::vec::BitVec;

#[derive(Default, Debug, Clone)]
pub(crate) struct PsnTracker {
    base_psn: u32,
    inner: BitVec,
}

impl PsnTracker {
    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    pub(crate) fn ack_range(&mut self, base_psn: u32, bitmap: u128) {
        todo!()
    }

    /// Acknowledges a single PSN.
    pub(crate) fn ack_one(&mut self, psn: u32) {
        todo!()
    }

    /// Returns `true` if all PSNs up to and including the given PSN have been acknowledged.
    pub(crate) fn all_acked(&self, psn_to: u32) -> bool {
        psn_to < self.base_psn
    }
}
