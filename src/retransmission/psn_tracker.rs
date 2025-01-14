use bitvec::vec::BitVec;

struct PsnTracker {
    base_psn: u32,
    inner: BitVec,
}

impl PsnTracker {
    /// Acknowledges a range of PSNs starting from base_psn using a bitmap.
    fn ack_range(&mut self, base_psn: u32, bitmap: u128) {
        todo!()
    }

    /// Acknowledges a single PSN.
    fn ack_one(&mut self, psn: u32) {
        todo!()
    }

    /// Returns `true` if all PSNs up to and including the given PSN have been acknowledged.
    fn all_acked(&self, psn_to: u32) -> bool {
        psn_to < self.base_psn
    }
}
