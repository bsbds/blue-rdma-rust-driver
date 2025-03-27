mod handler;

pub(crate) use handler::MetaHandler;

use std::{
    io,
    sync::{
        atomic::{fence, AtomicBool, Ordering},
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
    queue_pair::QueuePairAttrTable,
    rdma_write_worker::RdmaWriteTask,
    timeout_retransmit::RetransmitTask,
};

/// A worker for processing packet meta
pub(crate) struct MetaWorker<T> {
    /// Inner meta report queue
    inner: T,
    handler: MetaHandler,
}

impl<T: MetaReport + Send + 'static> MetaWorker<T> {
    pub(crate) fn new(inner: T, handler: MetaHandler) -> Self {
        Self { inner, handler }
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
                if self.handler.handle_meta(meta).is_none() {
                    error!("invalid meta: {meta:?}");
                }
            };
        }

        Ok(())
    }
}
