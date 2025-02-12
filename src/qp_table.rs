use std::iter;

use crate::{constants::MAX_QP_CNT, qp::qpn_index};

pub(crate) struct QpTable<T> {
    inner: Box<[T]>,
}

impl<T> QpTable<T> {
    pub(crate) fn get_qp_mut(&mut self, qpn: u32) -> Option<&mut T> {
        self.inner.get_mut(qpn_index(qpn))
    }
}

impl<T: Default> QpTable<T> {
    pub(crate) fn new() -> Self {
        Self {
            inner: iter::repeat_with(T::default).take(MAX_QP_CNT).collect(),
        }
    }
}
