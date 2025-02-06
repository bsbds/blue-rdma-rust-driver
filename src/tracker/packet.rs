use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use bitvec::{bits, order::Lsb0, vec::BitVec, view::BitView};

use crate::constants::{MAX_PSN_WINDOW, PSN_MASK};

#[derive(Default, Debug, Clone)]
pub(crate) struct PacketTracker {
    base_psn: Arc<AtomicU32>,
    inner: BitVec,
}

impl PacketTracker {
    pub(crate) fn new_with_default_base() -> Self {
        Self {
            base_psn: Arc::new(0.into()),
            inner: BitVec::default(),
        }
    }

    #[allow(clippy::as_conversions)] // u32 to usize
    /// Acknowledges a range of PSNs starting from `base_psn` using a bitmap.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub(crate) fn ack_range(&mut self, mut now_psn: u32, mut bitmap: u128) -> Option<u32> {
        // won't wrap since we only use 24bits of the u32
        let now_psn_int = now_psn as i32;
        let base_psn_int = self.base_psn() as i32;
        let rstart = now_psn_int - base_psn_int;
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

    #[allow(clippy::as_conversions)] // u32 to usize
    /// Acknowledges a single PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if the left edge of the PSN window is advanced, where the
    /// returned `PSN` is the new base PSN value after the advance.
    pub(crate) fn ack_one(&mut self, psn: u32) -> Option<u32> {
        let rstart = (psn.wrapping_sub(self.base_psn()) & PSN_MASK) as usize;
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
        let rstart = (psn.wrapping_sub(self.base_psn()) & PSN_MASK) as usize;
        let rend = rstart.wrapping_add(1);
        if rend > MAX_PSN_WINDOW {
            return None;
        }
        self.set_base_psn(psn);
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
        let x = self.base_psn().wrapping_sub(psn_to) & PSN_MASK;
        x > 0 && (x as usize) < MAX_PSN_WINDOW
    }

    /// Returns the current base PSN
    fn base_psn(&self) -> u32 {
        self.base_psn.load(Ordering::Acquire)
    }

    /// Returns the current base PSN
    fn set_base_psn(&self, value: u32) {
        self.base_psn.store(value, Ordering::Release);
    }

    /// Try to advance the base PSN to the next unacknowledged PSN.
    ///
    /// # Returns
    ///
    /// Returns `Some(PSN)` if `base_psn` was advanced, where the returned `PSN` is the new
    /// base PSN value after the advance.
    #[allow(clippy::as_conversions)] // pos should never larger than u32::MAX
    fn try_advance(&mut self) -> Option<u32> {
        let pos = self.inner.first_zero().unwrap_or(self.inner.len());
        if pos == 0 {
            return None;
        }
        self.inner.shift_left(pos);
        let mut psn = self.base_psn();
        psn = psn.wrapping_add(pos as u32) & PSN_MASK;
        self.set_base_psn(psn);
        Some(psn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_one() {
        let mut tracker = PacketTracker::default();
        tracker.ack_one(5);
        assert!(!tracker.inner[0..5].iter().any(|b| *b));
        assert!(tracker.inner[5]);
    }

    #[test]
    fn test_ack_range() {
        let mut tracker = PacketTracker::default();
        tracker.ack_range(0, 0b11); // PSN 0 and 1
        assert_eq!(tracker.base_psn(), 2);
        assert!(tracker.inner.not_all());

        let mut tracker = PacketTracker {
            base_psn: Arc::new(5.into()),
            ..Default::default()
        };
        tracker.ack_range(5, 0b11);
        assert_eq!(tracker.base_psn(), 7);
        assert!(tracker.inner.not_all());

        let mut tracker = PacketTracker {
            base_psn: Arc::new(10.into()),
            ..Default::default()
        };
        tracker.ack_range(5, 0b11);
        assert_eq!(tracker.base_psn(), 10);
        assert!(tracker.inner.is_empty());
        tracker.ack_range(20, 0b11);
        assert_eq!(tracker.base_psn(), 10);
        assert!(tracker.inner[10]);
        assert!(tracker.inner[11]);
    }

    #[test]
    fn test_all_acked() {
        let tracker = PacketTracker {
            base_psn: Arc::new(10.into()),
            ..Default::default()
        };
        assert!(tracker.all_acked(9));
        assert!(!tracker.all_acked(10));
        assert!(!tracker.all_acked(11));
    }

    #[test]
    fn test_wrapping_ack() {
        let mut tracker = PacketTracker {
            base_psn: Arc::new((PSN_MASK - 1).into()),
            ..Default::default()
        };
        tracker.ack_range(0, 0b11);
    }
}
