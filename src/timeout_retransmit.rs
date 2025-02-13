use std::{io, iter, thread, time::Duration};

use tracing::error;

use crate::{
    constants::MAX_QP_CNT,
    device_protocol::{WorkReqSend, WrChunk},
    protocol_impl_hardware::SendQueueScheduler,
    qp::qpn_index,
    timer::TransportTimer,
};

const DEFAULT_INIT_RETRY_COUNT: usize = 5;
const DEFAULT_TIMEOUT_CHECK_DURATION: u8 = 8;
const DEFAULT_LOCAL_ACK_TIMEOUT: u8 = 4;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AckTimeoutConfig {
    // 4.096 uS * 2^(CHECK DURATION)
    check_duration: u8,
    // 4.096 uS * 2^(Local ACK Timeout)
    local_ack_timeout: u8,
    init_retry_count: usize,
}

impl Default for AckTimeoutConfig {
    fn default() -> Self {
        Self {
            check_duration: DEFAULT_TIMEOUT_CHECK_DURATION,
            local_ack_timeout: DEFAULT_LOCAL_ACK_TIMEOUT,
            init_retry_count: DEFAULT_INIT_RETRY_COUNT,
        }
    }
}

impl AckTimeoutConfig {
    pub(crate) fn new(check_duration: u8, local_ack_timeout: u8, init_retry_count: usize) -> Self {
        Self {
            check_duration,
            local_ack_timeout,
            init_retry_count,
        }
    }
}

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
    config: AckTimeoutConfig,
}

impl TimeoutRetransmitWorker {
    pub(crate) fn new(
        receiver: flume::Receiver<RetransmitTask>,
        wr_sender: SendQueueScheduler,
        config: AckTimeoutConfig,
    ) -> Self {
        Self {
            receiver,
            wr_sender,
            table: TransportTimerTable::new(),
            config,
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
        let check_duration_ns = Duration::from_nanos(4096u64 << self.config.check_duration);
        loop {
            spin_sleep::sleep(check_duration_ns);
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
