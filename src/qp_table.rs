use std::{iter, mem};

use crate::{constants::MAX_QP_CNT, queue_pair::qpn_index};

#[derive(Debug)]
pub(crate) struct QpTable<T> {
    inner: Box<[T]>,
}

impl<T> QpTable<T> {
    pub(crate) fn get_qp(&self, qpn: u32) -> Option<&T> {
        self.inner.get(qpn_index(qpn))
    }

    pub(crate) fn get_qp_mut(&mut self, qpn: u32) -> Option<&mut T> {
        self.inner.get_mut(qpn_index(qpn))
    }

    pub(crate) fn replace(&mut self, qpn: u32, mut t: T) -> Option<T> {
        if let Some(x) = self.inner.get_mut(qpn_index(qpn)) {
            mem::swap(x, &mut t);
            Some(t)
        } else {
            None
        }
    }
}

impl<T: Default> QpTable<T> {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl<T: Default> Default for QpTable<T> {
    fn default() -> Self {
        Self {
            inner: iter::repeat_with(T::default).take(MAX_QP_CNT).collect(),
        }
    }
}
