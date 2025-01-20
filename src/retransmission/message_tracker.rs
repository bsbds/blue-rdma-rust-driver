use std::collections::BTreeMap;

/// Tracks MSNs and their corresponding PSN
#[derive(Default, Debug, Clone)]
pub(crate) struct MessageTracker {
    /// Maps MSNs to their last PSN
    inner: BTreeMap<u32, u16>,
}

impl MessageTracker {
    /// Inserts a new message tracking entry.
    ///
    /// # Returns
    ///
    /// The last PSN for this message (base_psn + psn_total)
    pub(crate) fn insert(&mut self, msn: u16, end_psn: u32) {
        if self.inner.insert(end_psn, msn).is_some() {
            tracing::error!("Duplicate end psn, psn: {end_psn}");
        }
    }

    // FIXME: wrapped PSN
    /// Acknowledges messages up to the given PSN and returns the MSNs of all
    /// acknowledged messages.
    pub(crate) fn ack(&mut self, psn: u32) -> Vec<u16> {
        let mut acked = Vec::new();
        while self.inner.first_entry().is_some_and(|e| *e.key() <= psn) {
            if let Some((_, msn)) = self.inner.pop_first() {
                acked.push(msn);
            }
        }
        acked
    }
}
