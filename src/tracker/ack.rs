use bitvec::vec::BitVec;

use super::msn::Msn;

/// Tracker for tracking message sequence number of ACK packets
#[derive(Default, Debug, Clone)]
pub(crate) struct AckTracker {
    base_msn: Msn,
    inner: BitVec,
}

impl AckTracker {
    /// Acknowledges a single MSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(MSN)` if the left edge of the MSN window is advanced, where the
    /// returned `MSN` is the new base MSN value after the advance.
    #[allow(clippy::as_conversions)] // u16 to usize
    pub(crate) fn ack(&mut self, msn: Msn) -> Option<Msn> {
        if msn < self.base_msn {
            return None;
        }
        let rstart = msn.distance(self.base_msn);
        if rstart >= self.inner.len() {
            self.inner.resize(rstart + 1, false);
        }
        self.inner.set(rstart, true);
        self.try_advance()
    }

    /// Returns the current base MSN
    pub(crate) fn base_msn(&self) -> Msn {
        self.base_msn
    }

    /// Try to advance the base MSN
    ///
    /// # Returns
    ///
    /// Returns `Some(MSN)` if `base_msn` was advanced, where the returned `MSN` is the new
    /// base MSN value after the advance.
    #[allow(clippy::as_conversions)] // pos should never larger than u16::MAX
    fn try_advance(&mut self) -> Option<Msn> {
        self.inner
            .first_zero()
            .and_then(|pos| (pos > 0).then_some(pos))
            .map(|pos| {
                self.inner.shift_left(pos);
                self.base_msn = self.base_msn.advance(pos);
                self.base_msn
            })
    }
}
