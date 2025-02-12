use std::{cmp::Ordering, collections::VecDeque};

use crate::{constants::MAX_PSN_WINDOW, send::SendWrResolver};

struct IbvSendQueueTable {
    inner: Box<[IbvSendQueue]>,
}

struct IbvSendQueue {
    inner: VecDeque<SendQueueElem>,
}

impl IbvSendQueue {
    fn push(&mut self, elem: SendQueueElem) {
        self.inner.push_back(elem);
    }

    fn pop_until(&mut self, psn: u32) {
        let mut a = self.inner.partition_point(|x| x.psn < Psn(psn));
        let _drop = self.inner.drain(..a);
    }

    fn find(&self, psn_low: u32, psn_high: u32) -> Vec<SendQueueElem> {
        let mut a = self.inner.partition_point(|x| x.psn <= Psn(psn_low));
        let mut b = self.inner.partition_point(|x| x.psn < Psn(psn_high));
        a = a.saturating_sub(1);
        self.inner.range(a..b).copied().collect()
    }
}

#[derive(Clone, Copy)]
struct SendQueueElem {
    psn: Psn,
    pub(crate) wr: SendWrResolver,
}

impl SendQueueElem {
    fn new(psn: u32, wr: SendWrResolver) -> Self {
        Self { psn: Psn(psn), wr }
    }

    fn psn(&self) -> u32 {
        self.psn.0
    }

    fn wr(&self) -> SendWrResolver {
        self.wr
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Psn(pub(crate) u32);

impl PartialOrd for Psn {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let x = self.0.wrapping_sub(other.0);
        Some(match x {
            0 => Ordering::Equal,
            x if x as usize > MAX_PSN_WINDOW => Ordering::Less,
            _ => Ordering::Greater,
        })
    }
}
