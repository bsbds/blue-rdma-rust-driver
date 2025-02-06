use std::{cmp::Ordering, collections::VecDeque};

use crate::constants::MAX_SEND_WR;

use super::msn::Msn;

pub(crate) struct MessageTracker {
    inner: VecDeque<MessageMeta>,
}

impl MessageTracker {
    pub(crate) fn append(&mut self, meta: MessageMeta) {
        if self.inner.back().is_some_and(|last| last.msn > meta.msn) {
            let insert_pos = self
                .inner
                .iter()
                .position(|m| m.msn > meta.msn)
                .unwrap_or(self.inner.len());
            self.inner.insert(insert_pos, meta);
        } else {
            self.inner.push_back(meta);
        }
    }

    pub(crate) fn ack(&mut self, base_psn: u32) -> Vec<MessageMeta> {
        let mut elements = Vec::new();
        while let Some(elem) = self.inner.front() {
            if elem.psn < base_psn {
                elements.push(self.inner.pop_front().unwrap_or_else(|| unreachable!()));
            }
        }
        elements
    }
}

pub(crate) struct MessageMeta {
    msn: Msn,
    psn: u32,
    ack_req: bool,
}

impl MessageMeta {
    pub(crate) fn new(msn: Msn, psn: u32, ack_req: bool) -> Self {
        Self { msn, psn, ack_req }
    }

    pub(crate) fn msn(&self) -> Msn {
        self.msn
    }

    pub(crate) fn psn(&self) -> u32 {
        self.psn
    }

    pub(crate) fn ack_req(&self) -> bool {
        self.ack_req
    }
}
