use tracing::error;

use crate::{
    constants::PSN_MASK,
    device_protocol::{AckMeta, NakMeta},
    packet_retransmit::PacketRetransmitTask,
    queue_pair::Tracker,
    timeout_retransmit::RetransmitTask,
};

use super::{CompletionTask, MetaWorker};

impl<T> MetaWorker<T> {
    pub(super) fn handle_nak(&mut self, meta: NakMeta) {
        if meta.is_send_by_driver {
            self.handle_nak_driver(meta);
        } else {
            self.handle_nak_hw(meta);
        }
    }

    fn handle_nak_hw(&mut self, meta: NakMeta) {
        let NakMeta {
            qpn,
            msn: ack_msn,
            psn_now,
            now_bitmap,
            pre_bitmap,
            psn_before_slide: psn_pre,
            is_send_by_local_hw,
            ..
        } = meta;
        let table = if is_send_by_local_hw {
            &mut self.recv_table
        } else {
            &mut self.send_table
        };
        let Some(tracker) = table.get_mut(qpn) else {
            error!("qp number: {qpn} does not exist");
            return;
        };
        let ack_msn = (!is_send_by_local_hw).then_some(ack_msn);
        let x = Self::update_packet_tracker_nak(tracker, psn_now, now_bitmap);
        let y = Self::update_packet_tracker_nak(tracker, psn_pre, pre_bitmap);
        for psn in x.into_iter().chain(y) {
            self.send_update(!is_send_by_local_hw, qpn, psn);
        }
        if !is_send_by_local_hw {
            // TODO: implement more fine-grained retransmission
            let _ignore = self
                .packet_retransmit_tx
                .send(PacketRetransmitTask::RetransmitRange {
                    qpn,
                    psn_low: psn_pre,
                    psn_high: psn_now.wrapping_add(128) % PSN_MASK,
                });
        }
    }

    fn handle_nak_driver(&mut self, meta: NakMeta) {
        let NakMeta {
            qpn,
            msn: ack_msn,
            psn_now,
            psn_before_slide: psn_pre,
            ..
        } = meta;
        let table = &mut self.send_table;
        let Some(tracker) = table.get_mut(qpn) else {
            return;
        };
        let _ignore = self
            .packet_retransmit_tx
            .send(PacketRetransmitTask::RetransmitRange {
                qpn,
                psn_low: psn_pre,
                psn_high: psn_now,
            });
    }

    // TODO: handles msn of NAKs
    fn update_packet_tracker_nak(
        tracker: &mut Tracker,
        base_psn: u32,
        bitmap: u128,
    ) -> Option<u32> {
        tracker.merge_bitmap(base_psn, bitmap)
    }
}
