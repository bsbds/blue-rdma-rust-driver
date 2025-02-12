use std::{io, iter, thread, time::Duration};

use tracing::error;

use crate::{
    constants::MAX_QP_CNT,
    device_protocol::{WorkReqSend, WrChunk},
    protocol_impl_hardware::SendQueueScheduler,
    qp::qpn_index,
    timer::TransportTimer,
};

const TIMEOUT_CHECK_DURATION: Duration = Duration::from_micros(8);

/// Timer per QP
struct TransportTimerTable {
    inner: Box<[Entry]>,
}

impl TransportTimerTable {
    fn new() -> Self {
        Self {
            inner: iter::repeat_with(Entry::default).take(MAX_QP_CNT).collect(),
        }
    }

    fn get_qp_mut(&mut self, qpn: u32) -> Option<&mut Entry> {
        self.inner.get_mut(qpn_index(qpn))
    }
}

#[derive(Default)]
struct Entry {
    timer: TransportTimer,
    // contains the last packet which ack_req bit is set
    last_packet_chunk: Option<WrChunk>,
}

#[allow(variant_size_differences)]
pub(crate) enum RetransmitTask {
    NewAckReq {
        qpn: u32,
        // contains the last packet which ack_req bit is set
        last_packet_chunk: WrChunk,
    },
    ReceiveACK {
        qpn: u32,
    },
}

impl RetransmitTask {
    fn qpn(&self) -> u32 {
        match *self {
            RetransmitTask::NewAckReq { qpn, .. } | RetransmitTask::ReceiveACK { qpn } => qpn,
        }
    }
}

pub(crate) struct TimeoutRetransmitWorker {
    receiver: flume::Receiver<RetransmitTask>,
    table: TransportTimerTable,
    wr_sender: SendQueueScheduler,
}

impl TimeoutRetransmitWorker {
    pub(crate) fn new(
        receiver: flume::Receiver<RetransmitTask>,
        wr_sender: SendQueueScheduler,
    ) -> Self {
        Self {
            receiver,
            wr_sender,
            table: TransportTimerTable::new(),
        }
    }

    pub(crate) fn spawn(self) {
        let _handle = thread::Builder::new()
            .name("timer-worker".into())
            .spawn(move || self.run())
            .unwrap_or_else(|err| unreachable!("Failed to spawn rx thread: {err}"));
    }

    #[allow(clippy::needless_pass_by_value)] // consume the flag
    /// Run the handler loop
    fn run(mut self) {
        loop {
            spin_sleep::sleep(TIMEOUT_CHECK_DURATION);
            for task in self.receiver.try_iter() {
                let Some(entry) = self.table.get_qp_mut(task.qpn()) else {
                    continue;
                };
                if matches!(task, RetransmitTask::NewAckReq { .. }) {
                    entry.timer.reset();
                }
            }
            for (index, entry) in self.table.inner.iter_mut().enumerate() {
                match entry.timer.check_timeout() {
                    Ok(true) => {
                        if let Some(packet) = entry.last_packet_chunk {
                            if let Err(err) = self.wr_sender.send(packet) {
                                error!("failed to send packet: {err}");
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(_) => todo!("handles retry failure"),
                }
            }
        }
    }
}
