//! Exponential Backoff with Full Jitter Policy (`backoff.rs`).
//!
//! Implements the AWS Architecture research recommendation for resilient retry loops:
//! `sleep = random_between(0, min(max_backoff, initial_backoff * multiplier^(attempt - 1)))`.

use std::time::Duration;

use rand::Rng;

use crate::config::ConsumerConfig;

/// Chính sách tính toán độ trễ lùi lũy tiến kèm Full Jitter ngẫu nhiên.
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    /// Thời gian chờ thử lại ban đầu.
    pub initial_backoff: Duration,
    /// Thời gian chờ thử lại tối đa.
    pub max_backoff: Duration,
    /// Hệ số nhân lũy tiến (thường là 2.0).
    pub multiplier: f64,
    /// Bật/tắt Full Jitter để triệt tiêu hiện tượng Thundering Herd.
    pub jitter: bool,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(60),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl From<&ConsumerConfig> for BackoffPolicy {
    fn from(config: &ConsumerConfig) -> Self {
        Self {
            initial_backoff: config.initial_retry_backoff,
            max_backoff: config.max_retry_backoff,
            multiplier: config.backoff_multiplier,
            jitter: config.backoff_jitter,
        }
    }
}

impl BackoffPolicy {
    /// Tính toán thời gian chờ ngủ dựa trên số lần thử lại hiện tại (1-indexed).
    #[must_use]
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let exp = f64::from(attempt.saturating_sub(1));
        let factor = self.multiplier.powf(exp);
        let millis = self.initial_backoff.as_millis() as f64 * factor;
        let capped = millis.min(self.max_backoff.as_millis() as f64);

        if self.jitter {
            let mut rng = rand::rng();
            let jittered = rng.random_range(0.0..=capped);
            Duration::from_millis(jittered as u64)
        } else {
            Duration::from_millis(capped as u64)
        }
    }
}
