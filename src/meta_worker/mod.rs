mod ack_impl;
mod header_impl;

use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use tracing::error;

use crate::{
    ack_responder::AckResponse,
    completion::CompletionTask,
    device_protocol::{MetaReport, ReportMeta},
    packet_retransmit::PacketRetransmitTask,
    queue_pair::{QueuePairAttrTable, TrackerTable},
    rdma_write_worker::RdmaWriteTask,
    timeout_retransmit::RetransmitTask,
};

/// A worker for processing packet meta
pub(crate) struct MetaWorker<T> {
    /// Inner meta report queue
    inner: T,
    send_table: TrackerTable,
    recv_table: TrackerTable,
    ack_tx: flume::Sender<AckResponse>,
    retransmit_tx: flume::Sender<RetransmitTask>,
    packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
    completion_tx: flume::Sender<CompletionTask>,
    rdma_write_tx: flume::Sender<RdmaWriteTask>,
}

impl<T: MetaReport + Send + 'static> MetaWorker<T> {
    pub(crate) fn new(
        inner: T,
        ack_tx: flume::Sender<AckResponse>,
        retransmit_tx: flume::Sender<RetransmitTask>,
        packet_retransmit_tx: flume::Sender<PacketRetransmitTask>,
        completion_tx: flume::Sender<CompletionTask>,
        rdma_write_tx: flume::Sender<RdmaWriteTask>,
    ) -> Self {
        Self {
            inner,
            ack_tx,
            retransmit_tx,
            packet_retransmit_tx,
            completion_tx,
            rdma_write_tx,
            send_table: TrackerTable::new(),
            recv_table: TrackerTable::new(),
        }
    }

    pub(crate) fn spawn(self, is_shutdown: Arc<AtomicBool>) {
        let _handle = thread::Builder::new()
            .name("meta-worker".into())
            .spawn(move || self.run(is_shutdown))
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    #[allow(clippy::needless_pass_by_value)] // consume the flag
    /// Run the handler loop
    fn run(mut self, is_shutdown: Arc<AtomicBool>) -> io::Result<()> {
        while !is_shutdown.load(Ordering::Relaxed) {
            if let Some(meta) = self.inner.try_recv_meta()? {
                self.handle_meta(meta);
            };
        }

        Ok(())
    }

    fn handle_meta(&mut self, meta: ReportMeta) {
        match meta {
            ReportMeta::Write(x) => self.handle_header_write(x),
            ReportMeta::Ack(x) => self.handle_ack(x),
            ReportMeta::Nak(x) => self.handle_nak(x),
            ReportMeta::Read(x) => self.handle_header_read(x),
            ReportMeta::Cnp { .. } => todo!(),
        }
    }
}
