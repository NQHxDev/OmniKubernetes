//! 3-State Resilient Circuit Breaker (Closed, Open, Half-Open).

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{extract::Request, http::StatusCode, response::Response};
use parking_lot::Mutex;
use thiserror::Error;
use tower::{Layer, Service};
use tracing::{info, warn};

use crate::config::ResilienceConfig;
use crate::resilience::{
    error::ResilienceError,
    metrics::{
        record_circuit_breaker_rejected, record_circuit_breaker_state,
        record_circuit_breaker_transition,
    },
};

/// Outcome of a single call tracked by the Circuit Breaker sliding window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOutcome {
    Success,
    InfrastructureFailure,
    SlowCall,
}

/// Helper to classify whether an HTTP status code is an infrastructure failure.
#[must_use]
pub fn is_infrastructure_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

/// Internal state machine of the Circuit Breaker.
#[derive(Debug)]
enum StateMachine {
    Closed {
        window: VecDeque<CallOutcome>,
    },
    Open {
        opened_at: Instant,
    },
    HalfOpen {
        probes_allowed: u32,
        probes_in_flight: u32,
        successful_probes: u32,
    },
}

/// Enterprise 3-state Circuit Breaker protecting downstream services.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    target_service: String,
    enabled: bool,
    error_threshold_pct: u8,
    min_requests: u32,
    reset_timeout: Duration,
    half_open_probes: u32,
    slow_call_threshold: Duration,
    state: Arc<Mutex<StateMachine>>,
}

impl CircuitBreaker {
    /// Creates a new Circuit Breaker from configuration.
    #[must_use]
    pub fn new(config: &ResilienceConfig, target_service: impl Into<String>) -> Self {
        let target_service = target_service.into();
        let error_threshold_pct = config.circuit_breaker_error_threshold_pct.clamp(1, 100);
        let min_requests = config.circuit_breaker_min_requests.max(5);
        let reset_timeout = Duration::from_secs(config.circuit_breaker_reset_timeout_secs.max(1));
        let half_open_probes = config.circuit_breaker_half_open_probes.clamp(1, 100);

        record_circuit_breaker_state(&target_service, 0.0);

        Self {
            target_service,
            enabled: config.circuit_breaker_enabled,
            error_threshold_pct,
            min_requests,
            reset_timeout,
            half_open_probes,
            slow_call_threshold: Duration::from_secs(2),
            state: Arc::new(Mutex::new(StateMachine::Closed {
                window: VecDeque::with_capacity(100),
            })),
        }
    }

    /// Sets custom slow call latency threshold (default 2s).
    #[must_use]
    pub fn with_slow_call_threshold(mut self, threshold: Duration) -> Self {
        self.slow_call_threshold = threshold;
        self
    }

    /// Target service identifier.
    #[must_use]
    pub fn target_service(&self) -> &str {
        &self.target_service
    }

    /// Acquires permission to execute a request.
    pub fn acquire_permit(&self) -> Result<CircuitPermit, ResilienceError> {
        if !self.enabled {
            return Ok(CircuitPermit {
                circuit: self.clone(),
                is_probe: false,
                start_time: Instant::now(),
                reported: false,
            });
        }

        let mut lock = self.state.lock();

        match &mut *lock {
            StateMachine::Closed { .. } => Ok(CircuitPermit {
                circuit: self.clone(),
                is_probe: false,
                start_time: Instant::now(),
                reported: false,
            }),
            StateMachine::Open { opened_at } => {
                if opened_at.elapsed() >= self.reset_timeout {
                    info!(
                        target_service = %self.target_service,
                        "Circuit breaker reset timeout elapsed. Transitioning OPEN -> HALF_OPEN."
                    );
                    record_circuit_breaker_transition(&self.target_service, "OPEN", "HALF_OPEN");
                    record_circuit_breaker_state(&self.target_service, 1.0);

                    *lock = StateMachine::HalfOpen {
                        probes_allowed: self.half_open_probes,
                        probes_in_flight: 1,
                        successful_probes: 0,
                    };

                    Ok(CircuitPermit {
                        circuit: self.clone(),
                        is_probe: true,
                        start_time: Instant::now(),
                        reported: false,
                    })
                } else {
                    record_circuit_breaker_rejected(&self.target_service);
                    let remaining_secs = self
                        .reset_timeout
                        .saturating_sub(opened_at.elapsed())
                        .as_secs()
                        .max(1);

                    Err(ResilienceError::CircuitBreakerOpen {
                        target_service: self.target_service.clone(),
                        retry_after_secs: remaining_secs,
                    })
                }
            }
            StateMachine::HalfOpen {
                probes_allowed,
                probes_in_flight,
                successful_probes,
            } => {
                if *probes_in_flight + *successful_probes < *probes_allowed {
                    *probes_in_flight += 1;
                    Ok(CircuitPermit {
                        circuit: self.clone(),
                        is_probe: true,
                        start_time: Instant::now(),
                        reported: false,
                    })
                } else {
                    record_circuit_breaker_rejected(&self.target_service);
                    Err(ResilienceError::CircuitBreakerOpen {
                        target_service: self.target_service.clone(),
                        retry_after_secs: self.reset_timeout.as_secs().max(1),
                    })
                }
            }
        }
    }

    /// Records call outcome and triggers state transitions if thresholds are breached.
    pub fn record_outcome(&self, is_probe: bool, outcome: CallOutcome) {
        if !self.enabled {
            return;
        }

        let mut lock = self.state.lock();

        match &mut *lock {
            StateMachine::Closed { window } => {
                window.push_back(outcome);
                if window.len() > 100 {
                    window.pop_front();
                }

                if window.len() >= self.min_requests as usize {
                    let total = window.len() as u64;
                    let failures = window
                        .iter()
                        .filter(|&&o| o == CallOutcome::InfrastructureFailure)
                        .count() as u64;

                    let error_rate_pct = (failures * 100) / total;

                    if error_rate_pct >= u64::from(self.error_threshold_pct) {
                        warn!(
                            target_service = %self.target_service,
                            error_rate_pct = error_rate_pct,
                            threshold_pct = self.error_threshold_pct,
                            "Circuit breaker trip condition met! Transitioning CLOSED -> OPEN."
                        );
                        record_circuit_breaker_transition(&self.target_service, "CLOSED", "OPEN");
                        record_circuit_breaker_state(&self.target_service, 2.0);

                        *lock = StateMachine::Open {
                            opened_at: Instant::now(),
                        };
                    }
                }
            }
            StateMachine::Open { .. } => {
                // Already open
            }
            StateMachine::HalfOpen {
                probes_in_flight,
                successful_probes,
                probes_allowed,
            } => {
                if is_probe {
                    *probes_in_flight = probes_in_flight.saturating_sub(1);

                    if outcome == CallOutcome::InfrastructureFailure {
                        warn!(
                            target_service = %self.target_service,
                            "Probing request failed in HALF_OPEN! Trip back to OPEN."
                        );
                        record_circuit_breaker_transition(
                            &self.target_service,
                            "HALF_OPEN",
                            "OPEN",
                        );
                        record_circuit_breaker_state(&self.target_service, 2.0);

                        *lock = StateMachine::Open {
                            opened_at: Instant::now(),
                        };
                    } else {
                        *successful_probes += 1;
                        if *successful_probes >= *probes_allowed {
                            info!(
                                target_service = %self.target_service,
                                probes = *probes_allowed,
                                "All probe requests succeeded in HALF_OPEN! Recovered: Transitioning HALF_OPEN -> CLOSED."
                            );
                            record_circuit_breaker_transition(
                                &self.target_service,
                                "HALF_OPEN",
                                "CLOSED",
                            );
                            record_circuit_breaker_state(&self.target_service, 0.0);

                            *lock = StateMachine::Closed {
                                window: VecDeque::with_capacity(100),
                            };
                        }
                    }
                }
            }
        }
    }

    /// Executes an async operation protected by the Circuit Breaker.
    pub async fn call<F, Fut, T, E>(&self, op: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut permit = self.acquire_permit().map_err(CircuitBreakerError::Open)?;
        let res = op().await;

        match &res {
            Ok(_) => {
                permit.report_success();
            }
            Err(_) => {
                // Generic error counts as infrastructure failure
                permit.report_failure();
            }
        }

        res.map_err(CircuitBreakerError::Inner)
    }
}

/// Error type returned by [`CircuitBreaker::call`].
#[derive(Debug, Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is OPEN: {0}")]
    Open(ResilienceError),
    #[error(transparent)]
    Inner(E),
}

/// Permit guard tracking a single in-flight request execution.
#[derive(Debug)]
pub struct CircuitPermit {
    circuit: CircuitBreaker,
    is_probe: bool,
    start_time: Instant,
    reported: bool,
}

impl CircuitPermit {
    /// Explicitly reports that the request was successful.
    pub fn report_success(&mut self) {
        if !self.reported {
            self.reported = true;
            let outcome = if self.start_time.elapsed() >= self.circuit.slow_call_threshold {
                CallOutcome::SlowCall
            } else {
                CallOutcome::Success
            };
            self.circuit.record_outcome(self.is_probe, outcome);
        }
    }

    /// Explicitly reports an infrastructure failure.
    pub fn report_failure(&mut self) {
        if !self.reported {
            self.reported = true;
            self.circuit
                .record_outcome(self.is_probe, CallOutcome::InfrastructureFailure);
        }
    }
}

impl Drop for CircuitPermit {
    fn drop(&mut self) {
        if !self.reported {
            // Default to success if not explicitly marked failure
            self.report_success();
        }
    }
}

/// Tower Layer for wrapping service endpoints with Circuit Breaker protection.
#[derive(Debug, Clone)]
pub struct CircuitBreakerLayer {
    breaker: CircuitBreaker,
}

impl CircuitBreakerLayer {
    /// Creates a new `CircuitBreakerLayer` from configuration.
    #[must_use]
    pub fn new(config: &ResilienceConfig, target_service: impl Into<String>) -> Self {
        Self {
            breaker: CircuitBreaker::new(config, target_service),
        }
    }
}

impl<S> Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreakerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreakerService {
            inner,
            breaker: self.breaker.clone(),
        }
    }
}

/// Tower Service implementation for Circuit Breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerService<S> {
    inner: S,
    breaker: CircuitBreaker,
}

impl<S> Service<Request> for CircuitBreakerService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let path = req.uri().path().to_string();

        match self.breaker.acquire_permit() {
            Ok(mut permit) => {
                let mut inner = self.inner.clone();
                Box::pin(async move {
                    let response = inner.call(req).await?;
                    if is_infrastructure_status(response.status()) {
                        permit.report_failure();
                    } else {
                        permit.report_success();
                    }
                    Ok(response)
                })
            }
            Err(err) => Box::pin(async move { Ok(err.to_response(&path, None)) }),
        }
    }
}
