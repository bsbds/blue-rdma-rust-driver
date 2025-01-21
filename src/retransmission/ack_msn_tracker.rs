use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc,
};

use bitvec::vec::BitVec;

use crate::constants::MAX_MSN_WINDOW;

/// Tracker for tracking message sequence number of ACK packets
#[derive(Default, Debug, Clone)]
pub(crate) struct AckMsnTracker {
    base_msn: Arc<AtomicU16>,
    inner: BitVec,
}

impl AckMsnTracker {
    /// Acknowledges a single MSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(MSN)` if the left edge of the MSN window is advanced, where the
    /// returned `MSN` is the new base MSN value after the advance.
    #[allow(clippy::as_conversions)] // u16 to usize
    pub(crate) fn ack(&mut self, msn: u16) -> Option<u16> {
        let rstart = msn.wrapping_sub(self.base_msn()) as usize;
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

    /// Returns the current base MSN
    pub(crate) fn base_msn(&self) -> u16 {
        self.base_msn.load(Ordering::Acquire)
    }

    /// Sets the current base MSN
    pub(crate) fn set_base_msn(&self, value: u16) {
        self.base_msn.store(value, Ordering::Release)
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
                let mut msn = self.base_msn();
                msn = msn.wrapping_add(pos as u16);
                self.set_base_msn(msn);
                msn
            })
    }
}
