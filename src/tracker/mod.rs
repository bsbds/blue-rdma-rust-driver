mod ack;
mod msn;
mod packet;

pub(crate) use ack::*;
pub(crate) use msn::*;
pub(crate) use packet::*;

use crate::utils::Psn;

#[derive(Debug, Default)]
pub(crate) struct LocalAckTracker {
    psn_tracker: PsnTracker,
    psn_pre: Psn,
}

impl LocalAckTracker {
    pub(crate) fn ack_one(&mut self, psn: Psn) -> Option<Psn> {
        self.psn_tracker.ack_one(psn)
    }

    pub(crate) fn ack_bitmap(&mut self, base_psn: Psn, bitmap: u128) -> Option<Psn> {
        let x = self.psn_tracker.ack_range(self.psn_pre, base_psn);
        let y = self.psn_tracker.ack_bitmap(base_psn, bitmap);
        if self.psn_pre < base_psn {
            self.psn_pre = base_psn;
        }
        y.or(x)
    }

    pub(crate) fn nak_bitmap(
        &mut self,
        psn_pre: Psn,
        pre_bitmap: u128,
        psn_now: Psn,
        now_bitmap: u128,
    ) -> Option<Psn> {
        let x = self.psn_tracker.ack_range(self.psn_pre, psn_pre);
        let y = self.psn_tracker.ack_bitmap(psn_pre, pre_bitmap);
        let z = self.psn_tracker.ack_bitmap(psn_now, now_bitmap);
        if self.psn_pre < psn_now {
            self.psn_pre = psn_now;
        }
        z.or(y).or(x)
    }

    pub(crate) fn base_psn(&self) -> Psn {
        self.psn_tracker.base_psn()
    }
}

#[derive(Debug, Default)]
pub(crate) struct RemoteAckTracker {
    psn_tracker: PsnTracker,
    msn_pre: u16,
    psn_pre: Psn,
}

impl RemoteAckTracker {
    pub(crate) fn ack_before(&mut self, psn: Psn) -> Option<Psn> {
        self.psn_tracker.ack_before(psn)
    }

    pub(crate) fn nak_bitmap(
        &mut self,
        msn: u16,
        psn_pre: Psn,
        pre_bitmap: u128,
        psn_now: Psn,
        now_bitmap: u128,
    ) -> Option<Psn> {
        let x = (msn == self.msn_pre.wrapping_add(1))
            .then(|| self.psn_tracker.ack_range(self.psn_pre, psn_pre))
            .flatten();
        let y = self.psn_tracker.ack_bitmap(psn_pre, pre_bitmap);
        let z = self.psn_tracker.ack_bitmap(psn_now, now_bitmap);
        if self.psn_pre < psn_now {
            self.psn_pre = psn_now;
            self.msn_pre = msn;
        }
        z.or(y).or(x)
    }
}
