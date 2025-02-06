use std::cmp::Ordering;

use crate::constants::MAX_SEND_WR;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Msn(pub(crate) u16);

impl Msn {
    pub(crate) fn distance(self, rhs: Self) -> usize {
        self.0.wrapping_sub(rhs.0) as usize
    }

    #[allow(clippy::expect_used)]
    /// Advances the MSN by the given delta.
    ///
    /// # Panics
    ///
    /// Panics if the delta cannot be converted to a u16.
    pub(crate) fn advance(self, dlt: usize) -> Self {
        let x = self
            .0
            .wrapping_add(u16::try_from(dlt).expect("invalid delta"));
        Self(x)
    }
}

impl PartialOrd for Msn {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let x = self.0.wrapping_sub(other.0);
        Some(match x {
            0 => Ordering::Equal,
            x if x as usize > MAX_SEND_WR => Ordering::Less,
            _ => Ordering::Greater,
        })
    }
}

impl std::ops::Sub for Msn {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.sub(rhs.0))
    }
}
