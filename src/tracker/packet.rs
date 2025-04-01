use bitvec::{bits, order::Lsb0, vec::BitVec, view::BitView};

use crate::{
    constants::{MAX_PSN_WINDOW, PSN_MASK},
    utils::Psn,
};

#[derive(Default, Debug, Clone)]
pub(crate) struct PsnTracker {
    base_psn: Psn,
    inner: BitVec,
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // won't wrap since we only use 24bits of the Psn
impl PsnTracker {
    #[allow(clippy::as_conversions)] // Psn to usize
    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_bitmap(&mut self, mut now_psn: Psn, mut bitmap: u128) -> Option<Psn> {
        let rstart = self.rstart(now_psn);
        let rend = rstart + 128;
        if let Ok(x) = usize::try_from(rend) {
            if x > self.inner.len() {
                self.inner.resize(x, false);
            }
        }
        for i in rstart.max(0)..rend {
            let x = (i - rstart) as usize;
            if bitmap.wrapping_shr(x as u32) & 1 == 1 {
                self.inner.set(i as usize, true);
            }
        }

        self.try_advance()
    }

    /// Acknowledges a range of PSNs from `psn_low` to `psn_high` (exclusive).
    ///
    /// # Returns
    /// * `Some(PSN)` - If the acknowledgment causes the base PSN to advance, returns the new base PSN
    /// * `None` - If the base PSN doesn't change
    pub(crate) fn ack_range(&mut self, psn_low: Psn, psn_high: Psn) -> Option<Psn> {
        if psn_low <= self.base_psn {
            return self.ack_before(psn_high);
        }
        let rstart: usize = usize::try_from(self.rstart(psn_low)).ok()?;
        let rend: usize = usize::try_from(self.rstart(psn_high)).ok()?;
        if rend >= self.inner.len() {
            self.inner.resize(rend + 1, false);
        }
        for i in rstart..rend {
            self.inner.set(i, true);
        }
        None
    }

    /// Acknowledges a single PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_one(&mut self, psn: Psn) -> Option<Psn> {
        let rstart: usize = usize::try_from(self.rstart(psn)).ok()?;
        if rstart >= self.inner.len() {
            self.inner.resize(rstart + 1, false);
        }
        self.inner.set(rstart, true);
        self.try_advance()
    }

    /// Acknowledges all PSNs before the given PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_before(&mut self, psn: Psn) -> Option<Psn> {
        let rstart: usize = usize::try_from(self.rstart(psn)).ok()?;
        self.base_psn = psn;
        if rstart >= self.inner.len() {
            self.inner.fill(false);
        } else {
            self.inner.shift_left(rstart);
        }
        Some(psn)
    }

    pub(crate) fn base_psn(&self) -> Psn {
        self.base_psn
    }

    fn rstart(&self, psn: Psn) -> i32 {
        let x = (psn - self.base_psn).into_inner();
        if ((x >> 23) & 1) != 0 {
            (x | 0xFF00_0000) as i32
        } else {
            x as i32
        }
    }

    /// Try to advance the base PSN to the next unacknowledged PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if `base_psn` was advanced, where the returned `PSN` is the new
    /// base PSN value after the advance.
    fn try_advance(&mut self) -> Option<Psn> {
        let pos = self.inner.first_zero().unwrap_or(self.inner.len());
        if pos == 0 {
            return None;
        }
        self.inner.shift_left(pos);
        let mut psn = self.base_psn;
        self.base_psn += pos as u32;
        Some(self.base_psn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_one() {
        let mut tracker = PsnTracker::default();
        tracker.ack_one(5.into());
        assert!(!tracker.inner[0..5].iter().any(|b| *b));
        assert!(tracker.inner[5]);
    }

    #[test]
    fn test_ack_range() {
        let mut tracker = PsnTracker::default();
        tracker.ack_bitmap(0.into(), 0b11); // PSN 0 and 1
        assert_eq!(tracker.base_psn, 2.into());
        assert!(tracker.inner.not_all());

        let mut tracker = PsnTracker {
            base_psn: 5.into(),
            ..Default::default()
        };
        tracker.ack_bitmap(5.into(), 0b11);
        assert_eq!(tracker.base_psn, 7.into());
        assert!(tracker.inner.not_all());

        let mut tracker = PsnTracker {
            base_psn: 10.into(),
            ..Default::default()
        };
        tracker.ack_bitmap(5.into(), 0b11);
        assert_eq!(tracker.base_psn, 10.into());
        assert!(tracker.inner.not_all());
        tracker.ack_bitmap(20.into(), 0b11);
        assert_eq!(tracker.base_psn, 10.into());
        assert!(tracker.inner[10]);
        assert!(tracker.inner[11]);
    }

    #[test]
    fn test_wrapping_ack() {
        let mut tracker = PsnTracker {
            base_psn: (PSN_MASK - 1).into(),
            ..Default::default()
        };
        tracker.ack_bitmap(0.into(), 0b11);
    }
}
