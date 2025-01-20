// FIXME: Implement protection againt wrapped PSNs
use bitvec::{bits, order::Lsb0, vec::BitVec, view::BitView};

use crate::constants::{MAX_PSN_WINDOW, PSN_MASK};

#[derive(Default, Debug, Clone)]
pub(crate) struct PsnTracker {
    base_psn: u32,
    inner: BitVec,
}

impl PsnTracker {
    #[allow(clippy::as_conversions)] // u32 to usize
    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_range(&mut self, mut base_psn: u32, mut bitmap: u128) -> Option<u32> {
        let rstart = (base_psn.wrapping_sub(self.base_psn) & PSN_MASK) as usize;
        let rend = rstart.wrapping_add(128);
        if rend > MAX_PSN_WINDOW {
            return None;
        }
        if rend > self.inner.len() {
            self.inner.resize(rend, false);
        }
        for i in 0..128 {
            let pos = rstart.wrapping_add(i);
            if (bitmap >> i) & 1 == 1 {
                self.inner.set(pos, true);
            }
        }

        self.try_advance()
    }

    #[allow(clippy::as_conversions)] // u32 to usize
    /// Acknowledges a single PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_one(&mut self, psn: u32) -> Option<u32> {
        let rstart = (psn.wrapping_sub(self.base_psn) & PSN_MASK) as usize;
        let rend = rstart.wrapping_add(1);
        if rend > MAX_PSN_WINDOW {
            return None;
        }
        if rend > self.inner.len() {
            self.inner.resize(rend, false);
        }
        self.inner.set(rstart, true);

        self.try_advance()
    }

    #[allow(clippy::as_conversions)] // u32 to usize
    /// Acknowledges all PSNs before the given PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_before(&mut self, psn: u32) -> Option<u32> {
        let rstart = (psn.wrapping_sub(self.base_psn) & PSN_MASK) as usize;
        let rend = rstart.wrapping_add(1);
        if rend > MAX_PSN_WINDOW {
            return None;
        }
        self.base_psn = psn;
        if rend > self.inner.len() {
            self.inner.fill(false);
        } else {
            self.inner.shift_left(rstart);
        }
        Some(psn)
    }

    #[allow(clippy::as_conversions)] // u32 to usize
    /// Returns `true` if all PSNs up to and including the given PSN have been acknowledged.
    pub(crate) fn all_acked(&self, psn_to: u32) -> bool {
        let x = self.base_psn.wrapping_sub(psn_to) & PSN_MASK;
        x > 0 && (x as usize) < MAX_PSN_WINDOW
    }

    /// Returns the current base PSN
    pub(crate) fn base_psn(&self) -> u32 {
        self.base_psn
    }

    /// Try to advance the base PSN to the next unacknowledged PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if `base_psn` was advanced, where the returned `PSN` is the new
    /// base PSN value after the advance.
    #[allow(clippy::as_conversions)] // pos should never larger than u32::MAX
    fn try_advance(&mut self) -> Option<u32> {
        self.inner
            .first_zero()
            .and_then(|pos| (pos > 0).then_some(pos))
            .map(|pos| {
                self.inner.shift_left(pos);
                self.base_psn = self.base_psn.wrapping_add(pos as u32) & PSN_MASK;
                self.base_psn
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_one() {
        let mut tracker = PsnTracker::default();
        tracker.ack_one(5);
        assert!(!tracker.inner[0..5].iter().any(|b| *b));
        assert!(tracker.inner[5]);
    }

    #[test]
    fn test_ack_range() {
        let mut tracker = PsnTracker::default();
        tracker.ack_range(0, 0b11); // PSN 0 and 1
        assert_eq!(tracker.base_psn, 2);
        assert!(tracker.inner.not_all());

        let mut tracker = PsnTracker {
            base_psn: 5,
            ..Default::default()
        };
        tracker.ack_range(5, 0b11);
        assert_eq!(tracker.base_psn, 7);
        assert!(tracker.inner.not_all());

        let mut tracker = PsnTracker {
            base_psn: 10,
            ..Default::default()
        };
        tracker.ack_range(5, 0b11);
        assert_eq!(tracker.base_psn, 10);
        assert!(tracker.inner.is_empty());
        tracker.ack_range(20, 0b11);
        assert_eq!(tracker.base_psn, 10);
        assert!(tracker.inner[10]);
        assert!(tracker.inner[11]);
    }

    #[test]
    fn test_all_acked() {
        let tracker = PsnTracker {
            base_psn: 10,
            ..Default::default()
        };
        assert!(tracker.all_acked(9));
        assert!(!tracker.all_acked(10));
        assert!(!tracker.all_acked(11));
    }

    #[test]
    fn test_wrapping_ack() {
        let mut tracker = PsnTracker {
            base_psn: PSN_MASK - 1,
            ..Default::default()
        };
        tracker.ack_range(0, 0b11);
    }
}
