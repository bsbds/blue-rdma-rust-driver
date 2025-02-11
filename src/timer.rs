use std::time::{self, Duration, Instant};

use thiserror::Error;

const INIT_RETRY_COUNT: usize = 5;

#[derive(Debug)]
pub(crate) struct TransportTimer {
    timeout_interval: Option<Duration>,
    last_start: Option<Instant>,
    retry_counter: usize,
}

impl TransportTimer {
    pub(crate) fn new(local_ack_timeout: u8) -> Self {
        let timeout_nanos = if local_ack_timeout == 0 {
            // disabled
            None
        } else {
            // 4.096 uS * 2^(Local ACK Timeout)
            Some(4096u64 << local_ack_timeout)
        };

        Self {
            timeout_interval: timeout_nanos.map(Duration::from_nanos),
            last_start: None,
            retry_counter: INIT_RETRY_COUNT,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.retry_counter = INIT_RETRY_COUNT;
        self.restart();
    }

    pub(crate) fn stop(&mut self) {
        self.last_start = None;
    }

    /// Returns `Ok(true)` if timeout
    pub(crate) fn check_timeout(&mut self) -> Result<bool, TimerError> {
        let Some(timeout_interval) = self.timeout_interval else {
            return Ok(false);
        };
        let Some(start_time) = self.last_start else {
            return Ok(false);
        };
        let elapsed = start_time.elapsed();
        if elapsed < timeout_interval {
            return Ok(false);
        }
        if self.retry_counter == 0 {
            return Err(TimerError);
        }
        self.retry_counter -= 1;
        self.restart();
        Ok(true)
    }

    pub(crate) fn is_running(&self) -> bool {
        self.last_start.is_some()
    }

    fn restart(&mut self) {
        self.last_start = Some(Instant::now());
    }
}

#[non_exhaustive]
#[derive(Debug, Error, Clone, Copy)]
#[error("reached maximum retry limit")]
pub(crate) struct TimerError;
