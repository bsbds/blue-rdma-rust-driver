use bitvec::vec::BitVec;

/// Tracker for tracking message sequence number of ACK packets
pub(crate) struct AckMsnTracker {
    base_msn: u16,
    inner: BitVec,
}

const MAX_MSN_WINDOW: usize = 1 << 15;

impl AckMsnTracker {
    /// Acknowledges a single MSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(MSN)` if the left edge of the MSN window is advanced, where the
    /// returned `MSN` is the new base MSN value after the advance.
    #[allow(clippy::as_conversions)] // u16 to usize
    pub(crate) fn ack(&mut self, msn: u16) -> Option<u16> {
        let rstart = msn.wrapping_sub(self.base_msn) as usize;
        let rend = rstart.wrapping_add(1);
        if rend > MAX_MSN_WINDOW {
            return None;
        }
        if rend > self.inner.len() {
            self.inner.resize(rend, false);
        }
        self.inner.set(rstart, true);

        self.try_advance()
    }

    /// Try to advance the base MSN
    ///
    /// # Returns
    ///
    /// Returns `Some(MSN)` if `base_msn` was advanced, where the returned `MSN` is the new
    /// base MSN value after the advance.
    #[allow(clippy::as_conversions)] // pos should never larger than u16::MAX
    fn try_advance(&mut self) -> Option<u16> {
        self.inner
            .first_zero()
            .and_then(|pos| (pos > 0).then_some(pos))
            .map(|pos| {
                self.inner.shift_left(pos);
                self.base_msn = self.base_msn.wrapping_add(pos as u16);
                self.base_msn
            })
    }
}
