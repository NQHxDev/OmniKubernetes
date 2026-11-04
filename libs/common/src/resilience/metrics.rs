//! Resilience and Traffic Control Prometheus metrics.

use metrics::{counter, gauge, histogram};
use std::time::Duration;

pub const METRIC_RATE_LIMITED_TOTAL: &str = "http_requests_rate_limited_total";
pub const METRIC_CIRCUIT_BREAKER_STATE: &str = "resilience_circuit_breaker_state";
pub const METRIC_CIRCUIT_BREAKER_TRANSITIONS: &str = "resilience_circuit_breaker_transitions_total";
pub const METRIC_CIRCUIT_BREAKER_REJECTED: &str = "resilience_circuit_breaker_rejected_total";
pub const METRIC_LOAD_SHED_REJECTED: &str = "resilience_load_shed_rejected_total";
pub const METRIC_CONCURRENCY_IN_FLIGHT: &str = "resilience_concurrency_in_flight";
pub const METRIC_RETRY_ATTEMPTS: &str = "resilience_retry_attempts_total";
pub const METRIC_RETRY_EXHAUSTED: &str = "resilience_retry_exhausted_total";
pub const METRIC_SHUTDOWN_STAGE_DURATION: &str = "resilience_shutdown_stage_duration_seconds";

/// Records a rate limited request occurrence.
#[inline]
pub fn record_rate_limited(route: &str, key_type: &str) {
    counter!(
        METRIC_RATE_LIMITED_TOTAL,
        "route" => route.to_string(),
        "key_type" => key_type.to_string()
    )
    .increment(1);
}

/// Records the current state of a circuit breaker (0=Closed, 1=HalfOpen, 2=Open).
#[inline]
pub fn record_circuit_breaker_state(target_service: &str, state_val: f64) {
    gauge!(
        METRIC_CIRCUIT_BREAKER_STATE,
        "target_service" => target_service.to_string()
    )
    .set(state_val);
}

/// Records a circuit breaker FSM state transition.
#[inline]
pub fn record_circuit_breaker_transition(
    target_service: &str,
    from: &'static str,
    to: &'static str,
) {
    counter!(
        METRIC_CIRCUIT_BREAKER_TRANSITIONS,
        "target_service" => target_service.to_string(),
        "from" => from,
        "to" => to
    )
    .increment(1);
}

/// Records a request rejected by an OPEN circuit breaker.
#[inline]
pub fn record_circuit_breaker_rejected(target_service: &str) {
    counter!(
        METRIC_CIRCUIT_BREAKER_REJECTED,
        "target_service" => target_service.to_string()
    )
    .increment(1);
}

/// Records an adaptive load shedding rejection.
#[inline]
pub fn record_load_shed_rejected(reason: &'static str) {
    counter!(
        METRIC_LOAD_SHED_REJECTED,
        "reason" => reason
    )
    .increment(1);
}

/// Updates the in-flight concurrent requests gauge for a service.
#[inline]
#[allow(clippy::cast_precision_loss)]
pub fn record_concurrency_in_flight(service: &str, in_flight: usize) {
    gauge!(
        METRIC_CONCURRENCY_IN_FLIGHT,
        "service" => service.to_string()
    )
    .set(in_flight as f64);
}

/// Records an attempt to retry a request.
#[inline]
pub fn record_retry_attempt(target_service: &str, attempt: u32) {
    counter!(
        METRIC_RETRY_ATTEMPTS,
        "target_service" => target_service.to_string(),
        "attempt" => attempt.to_string()
    )
    .increment(1);
}

/// Records exhaustion of all retry attempts.
#[inline]
pub fn record_retry_exhausted(target_service: &str) {
    counter!(
        METRIC_RETRY_EXHAUSTED,
        "target_service" => target_service.to_string()
    )
    .increment(1);
}

/// Records execution duration of a graceful shutdown stage.
#[inline]
pub fn record_shutdown_stage_duration(stage: &'static str, duration: Duration) {
    histogram!(
        METRIC_SHUTDOWN_STAGE_DURATION,
        "stage" => stage
    )
    .record(duration.as_secs_f64());
}
