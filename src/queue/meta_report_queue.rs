use std::io;

use crate::desc::{
    MetaReportQueueAckDesc, MetaReportQueueAckExtraDesc, MetaReportQueueDescFirst,
    MetaReportQueueDescNext, MetaReportQueuePacketBasicInfoDesc,
    MetaReportQueueReadReqExtendInfoDesc, RingBufDescUntyped,
};

use super::DescRingBuffer;

/// Meta report queue descriptors
pub(crate) enum MetaReportQueueDesc {
    /// Packet info for write operations
    WritePacketInfo(MetaReportQueuePacketBasicInfoDesc),
    /// Packet info for read operations
    ReadPacketInfo(
        (
            MetaReportQueuePacketBasicInfoDesc,
            MetaReportQueueReadReqExtendInfoDesc,
        ),
    ),
    /// Packet info for congestion event
    CnpPacketInfo(MetaReportQueuePacketBasicInfoDesc),
    /// Ack
    Ack(MetaReportQueueAckDesc),
    /// Nak
    Nak((MetaReportQueueAckDesc, MetaReportQueueAckExtraDesc)),
}

/// A transmit queue for the simple NIC device.
pub(crate) struct MetaReportQueue {
    /// Inner ring buffer
    inner: DescRingBuffer,
}

impl MetaReportQueue {
    pub(crate) fn new(inner: DescRingBuffer) -> Self {
        Self { inner }
    }

    /// Returns the base address of the buffer
    pub(crate) fn base_addr(&self) -> u64 {
        self.inner.base_addr()
    }

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<MetaReportQueueDesc> {
        let first = self
            .inner
            .try_pop()
            .copied()
            .map(MetaReportQueueDescFirst::from)?;

        if !first.has_next() {
            return match first {
                MetaReportQueueDescFirst::PacketInfo(d) if d.ecn_marked() => {
                    Some(MetaReportQueueDesc::CnpPacketInfo(d))
                }
                MetaReportQueueDescFirst::PacketInfo(d) => {
                    Some(MetaReportQueueDesc::WritePacketInfo(d))
                }
                MetaReportQueueDescFirst::Ack(d) => Some(MetaReportQueueDesc::Ack(d)),
            };
        }

        let next = self.inner.try_pop().copied().map_or_else(
            || unreachable!("failed to read next descriptor"),
            MetaReportQueueDescNext::from,
        );
        match (first, next) {
            (MetaReportQueueDescFirst::PacketInfo(f), MetaReportQueueDescNext::ReadInfo(n)) => {
                Some(MetaReportQueueDesc::ReadPacketInfo((f, n)))
            }
            (MetaReportQueueDescFirst::Ack(f), MetaReportQueueDescNext::AckExtra(n)) => {
                Some(MetaReportQueueDesc::Nak((f, n)))
            }
            (MetaReportQueueDescFirst::PacketInfo(_), MetaReportQueueDescNext::AckExtra(_))
            | (MetaReportQueueDescFirst::Ack(_), MetaReportQueueDescNext::ReadInfo(_)) => {
                unreachable!("invalid descriptor format")
            }
        }
    }
}
