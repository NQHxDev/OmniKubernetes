//! # Enterprise Resilience & Traffic Control Standard (`omni-common` Resilience).
//!
//! Provides the core resilience patterns for `OmniKubernetes` microservices:
//! - **Adaptive Rate Limiting (`rate_limit`):** In-Memory lock-free GCRA and distributed Redis sliding window.
//! - **Concurrency Limiting & Load Shedding (`load_shed`):** Semaphore bounds based on Little's Law.
//! - **Request Timeout & Cancellation Propagation (`timeout`):** Future dropping, deadline headers, and `CancellationToken`.
//! - **Enterprise Circuit Breaker (`circuit_breaker`):** 3-state FSM (`Closed`, `Open`, `HalfOpen`) isolating failing downstreams.
//! - **Retry Storm Prevention (`retry`):** AWS Full Jitter exponential backoff, idempotency checks, and retry budget token bucket.
//! - **Zero-Downtime Graceful Shutdown (`shutdown`):** 7-stage Kubernetes shutdown coordinator.
//! - **Observability & Error Contracts (`error`, `metrics`):** RFC 9457 Problem Details and low-cardinality Prometheus telemetry.

pub mod circuit_breaker;
pub mod error;
pub mod load_shed;
pub mod metrics;
pub mod rate_limit;
pub mod retry;
pub mod shutdown;
pub mod timeout;

use std::time::Duration;

pub use circuit_breaker::{
    CallOutcome, CircuitBreaker, CircuitBreakerError, CircuitBreakerLayer, CircuitBreakerService,
    CircuitPermit, is_infrastructure_status,
};
pub use error::{
    ERR_RATE_LIMITED, ERR_SERVICE_UNAVAILABLE, ERR_TIMEOUT, HEADER_RATE_LIMIT_LIMIT,
    HEADER_RATE_LIMIT_REMAINING, HEADER_RATE_LIMIT_RESET, PROBLEM_JSON_MIME, ProblemDetailsPayload,
    ResilienceError, TYPE_GATEWAY_TIMEOUT, TYPE_RATE_LIMITED, TYPE_SERVICE_UNAVAILABLE,
};
pub use load_shed::{ConcurrencyPermit, LoadShedController, LoadShedLayer, LoadShedService};
pub use rate_limit::{
    AtomicGcraCell, DistributedRateLimiter, InMemoryGcraLimiter, REDIS_SLIDING_WINDOW_LUA,
    RateLimitDecision, RateLimitKey, RateLimitLayer, RateLimitService, RateLimiter, RouteTier,
    classify_route_tier, extract_rate_limit_key, inject_ratelimit_headers,
};
pub use retry::{
    HEADER_IDEMPOTENCY_KEY, HEADER_X_IDEMPOTENCY_KEY, RetryBudget, RetryPolicy, execute_with_retry,
    is_idempotent_request,
};
pub use shutdown::{
    AtomicDrainGate, GracefulShutdownCoordinator, NoOpDrainGate, ShutdownDrainGate,
    create_probes_router,
};
pub use timeout::{
    CancelledError, HEADER_REQUEST_TIMEOUT, TimeoutLayer, TimeoutService, inject_deadline_header,
    with_cancellation_token,
};

use crate::config::ResilienceConfig;

/// Helper to create a [`RateLimitLayer`] from [`ResilienceConfig`].
#[must_use]
pub fn rate_limit_layer(config: &ResilienceConfig) -> RateLimitLayer {
    RateLimitLayer::new(config)
}

/// Helper to create a [`LoadShedLayer`] from [`ResilienceConfig`] and service name.
#[must_use]
pub fn load_shed_layer(
    config: &ResilienceConfig,
    service_name: impl Into<String>,
) -> LoadShedLayer {
    LoadShedLayer::new(config, service_name)
}

/// Helper to create a [`TimeoutLayer`] from a timeout [`Duration`].
#[must_use]
pub fn timeout_layer(timeout_duration: Duration) -> TimeoutLayer {
    TimeoutLayer::new(timeout_duration)
}

/// Helper to create a [`CircuitBreakerLayer`] from [`ResilienceConfig`] and downstream target service name.
#[must_use]
pub fn circuit_breaker_layer(
    config: &ResilienceConfig,
    target_service: impl Into<String>,
) -> CircuitBreakerLayer {
    CircuitBreakerLayer::new(config, target_service)
}
