use std::{
    io,
    sync::{atomic::AtomicBool, Arc},
};

use crate::{
    ack_responder::AckResponse,
    completion::CompletionTask,
    mem::{
        virt_to_phy::{AddressResolver, PhysAddrResolverLinuxX86},
        DmaBuf, PageWithPhysAddr,
    },
    meta_worker::{MetaHandler, MetaWorker},
    packet_retransmit::PacketRetransmitTask,
    protocol_impl::{
        desc::{
            MetaReportQueueAckDesc, MetaReportQueueAckExtraDesc, MetaReportQueueDescFirst,
            MetaReportQueueDescNext, MetaReportQueuePacketBasicInfoDesc,
            MetaReportQueueReadReqExtendInfoDesc, RingBufDescUntyped,
        },
        device::{
            mode::Mode, proxy::build_meta_report_queue_proxies, CsrBaseAddrAdaptor, DeviceAdaptor,
        },
        MetaReportQueueCtx, MetaReportQueueHandler,
    },
    qp::QueuePairAttrTable,
    rdma_write_worker::RdmaWriteTask,
    timeout_retransmit::RetransmitTask,
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

    /// Tries to poll next valid entry from the queue
    pub(crate) fn try_pop(&mut self) -> Option<MetaReportQueueDesc> {
        let first = self.inner.pop().map(MetaReportQueueDescFirst::from)?;

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

        let next = self.inner.pop().map_or_else(
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

    pub(crate) fn tail(&self) -> u32 {
        self.inner.tail() as u32
    }

    pub(crate) fn set_head(&mut self, head: u32) {
        self.inner.set_head(head);
    }

    pub(crate) fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn init_and_spawn_meta_worker<Dev>(
    dev: &Dev,
    pages: Vec<DmaBuf>,
    mode: Mode,
    ack_tx: flume::Sender<AckResponse>,
    retransmit_tx: flume::Sender<RetransmitTask>,
    packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
    completion_tx: flume::Sender<CompletionTask>,
    rdma_write_tx: flume::Sender<RdmaWriteTask>,
    is_shutdown: Arc<AtomicBool>,
) -> io::Result<()>
where
    Dev: Clone + DeviceAdaptor + Send + 'static,
{
    let mut mrq_proxies = build_meta_report_queue_proxies(dev.clone(), mode);
    for (proxy, page) in mrq_proxies.iter_mut().zip(pages.iter()) {
        proxy.write_base_addr(page.phys_addr)?;
    }
    let ctxs: Vec<_> = pages
        .into_iter()
        .map(|p| MetaReportQueue::new(DescRingBuffer::new(p.buf)))
        .zip(mrq_proxies)
        .map(|(q, p)| MetaReportQueueCtx::new(q, p))
        .collect();

    let handler = MetaHandler::new(
        ack_tx,
        retransmit_tx,
        packet_retransmit_tx,
        completion_tx,
        rdma_write_tx,
    );
    MetaWorker::new(MetaReportQueueHandler::new(ctxs), handler).spawn(is_shutdown);

    Ok(())
}
