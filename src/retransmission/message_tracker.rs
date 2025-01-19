use std::collections::HashMap;

/// Tracks MSNs and their corresponding PSN
pub(crate) struct MessageTracker {
    /// Maps MSNs to their last PSN
    inner: HashMap<u16, u32>,
}

impl MessageTracker {
    /// Inserts a new message tracking entry.
    ///
    /// # Returns
    ///
    /// The last PSN for this message (base_psn + psn_total)
    pub(crate) fn insert(&mut self, msn: u16, base_psn: u32, psn_total: u32) -> u32 {
        let end_psn = base_psn.wrapping_add(psn_total);
        if self.inner.insert(msn, end_psn).is_some() {
            tracing::debug!("Duplicate first packet, MSN: {msn}");
        }
        end_psn
    }

    /// Gets the last PSN for a given message sequence number.
    ///
    /// # Returns
    ///
    /// The final PSN if the MSN exists, None otherwise
    pub(crate) fn get_end_psn(&mut self, msn: u16) -> Option<u32> {
        self.inner.get(&msn).copied()
    }
}

