//! AWS Full Jitter Exponential Backoff, Idempotency Gating, and Retry Budget Token Bucket.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::http::{HeaderMap, Method};
use rand::Rng;
use tracing::warn;

use crate::config::ResilienceConfig;
use crate::resilience::metrics::{record_retry_attempt, record_retry_exhausted};

/// Header names indicating request idempotency.
pub const HEADER_IDEMPOTENCY_KEY: &str = "idempotency-key";
pub const HEADER_X_IDEMPOTENCY_KEY: &str = "x-idempotency-key";

/// Checks whether an HTTP request is safe/idempotent to retry.
#[must_use]
pub fn is_idempotent_request(method: &Method, headers: &HeaderMap) -> bool {
    // 1. Naturally idempotent methods under RFC 9110
    if matches!(
        *method,
        Method::GET | Method::HEAD | Method::PUT | Method::DELETE | Method::OPTIONS
    ) {
        return true;
    }

    // 2. POST / PATCH with valid Idempotency-Key
    if matches!(*method, Method::POST | Method::PATCH) {
        if let Some(key) = headers.get(HEADER_IDEMPOTENCY_KEY)
            && let Ok(s) = key.to_str()
            && !s.trim().is_empty()
        {
            return true;
        }

        if let Some(key) = headers.get(HEADER_X_IDEMPOTENCY_KEY)
            && let Ok(s) = key.to_str()
            && !s.trim().is_empty()
        {
            return true;
        }
    }

    false
}

/// Token bucket tracking Retry Budget to prevent retry storms.
///
/// Ensures retry requests do not exceed ~20% of normal traffic.
#[derive(Debug)]
pub struct RetryBudget {
    // Store tokens as micro-tokens (1.0 token = 1_000_000 micro-tokens)
    tokens: AtomicU64,
    max_tokens: u64,
}

impl RetryBudget {
    /// Creates a new Retry Budget with initial and max tokens.
    #[must_use]
    pub fn new(initial_tokens: u64, max_tokens: u64) -> Self {
        Self {
            tokens: AtomicU64::new(initial_tokens * 1_000_000),
            max_tokens: max_tokens * 1_000_000,
        }
    }

    /// Records a successful regular request, adding 0.2 token (200,000 micro-tokens).
    pub fn record_success(&self) {
        let deposit = 200_000;
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            let next = (current + deposit).min(self.max_tokens);
            match self.tokens.compare_exchange_weak(
                current,
                next,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Attempts to consume 1.0 token (`1_000_000` micro-tokens) to permit a retry.
    pub fn try_acquire_retry(&self) -> bool {
        let cost = 1_000_000;
        let mut current = self.tokens.load(Ordering::Relaxed);
        loop {
            if current < cost {
                return false;
            }
            let next = current - cost;
            match self.tokens.compare_exchange_weak(
                current,
                next,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }
}

impl Default for RetryBudget {
    fn default() -> Self {
        Self::new(10, 50)
    }
}

/// Calculator for Exponential Backoff with AWS Full Jitter.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    target_service: String,
    max_attempts: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
    budget: Arc<RetryBudget>,
}

impl RetryPolicy {
    /// Creates a new `RetryPolicy` from `ResilienceConfig`.
    #[must_use]
    pub fn new(config: &ResilienceConfig, target_service: impl Into<String>) -> Self {
        Self {
            target_service: target_service.into(),
            max_attempts: config.max_retry_attempts,
            base_delay_ms: config.retry_base_delay_ms.max(10),
            max_delay_ms: config.retry_max_delay_ms.max(config.retry_base_delay_ms),
            budget: Arc::new(RetryBudget::default()),
        }
    }

    /// Maximum configured retry attempts.
    #[must_use]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Computes sleep duration for an attempt using AWS Full Jitter:
    /// `sleep = random(0, min(max_delay, base_delay * 2^attempt))`
    #[must_use]
    pub fn calculate_jitter_delay(&self, attempt: u32) -> Duration {
        let exponent = attempt.min(30);
        let exp_factor = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let ceiling_ms = self
            .base_delay_ms
            .saturating_mul(exp_factor)
            .min(self.max_delay_ms);

        let random_ms = rand::rng().random_range(0..=ceiling_ms);
        Duration::from_millis(random_ms)
    }

    /// Checks if a retry is allowed by budget and attempts.
    pub fn can_retry(&self, attempt: u32) -> bool {
        if attempt >= self.max_attempts {
            record_retry_exhausted(&self.target_service);
            return false;
        }

        if !self.budget.try_acquire_retry() {
            record_retry_exhausted(&self.target_service);
            warn!(
                target_service = %self.target_service,
                "Retry budget exhausted (exceeds ~20% of traffic). Suppressing retry to prevent retry storm."
            );
            return false;
        }

        record_retry_attempt(&self.target_service, attempt + 1);
        true
    }

    /// Records a normal successful execution into the retry budget.
    pub fn record_success(&self) {
        self.budget.record_success();
    }
}

/// Executes an operation with full jitter exponential backoff and budget protection.
pub async fn execute_with_retry<F, Fut, T, E>(
    policy: &RetryPolicy,
    mut is_retryable: impl FnMut(&E) -> bool,
    mut op: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut attempt = 0;

    loop {
        match op().await {
            Ok(val) => {
                policy.record_success();
                return Ok(val);
            }
            Err(err) => {
                if !is_retryable(&err) || !policy.can_retry(attempt) {
                    return Err(err);
                }

                let sleep_duration = policy.calculate_jitter_delay(attempt);
                attempt += 1;
                tokio::time::sleep(sleep_duration).await;
            }
        }
    }
}
