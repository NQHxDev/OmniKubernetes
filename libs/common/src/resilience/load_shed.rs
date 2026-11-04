//! Concurrency limiting and adaptive load shedding based on Little's Law.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use axum::{extract::Request, response::Response};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tower::{Layer, Service};
use tracing::warn;

use crate::config::ResilienceConfig;
use crate::resilience::{
    error::ResilienceError,
    metrics::{record_concurrency_in_flight, record_load_shed_rejected},
};

/// Controller managing concurrency limits and load shedding decisions.
#[derive(Debug, Clone)]
pub struct LoadShedController {
    enabled: bool,
    service_name: String,
    max_concurrent: usize,
    semaphore: Arc<Semaphore>,
    in_flight: Arc<AtomicUsize>,
}

impl LoadShedController {
    /// Creates a new load shedding controller.
    #[must_use]
    pub fn new(config: &ResilienceConfig, service_name: impl Into<String>) -> Self {
        let max_concurrent = config.max_concurrent_requests.max(1);
        Self {
            enabled: config.load_shed_enabled,
            service_name: service_name.into(),
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Attempts to acquire an execution permit.
    pub fn try_acquire(&self) -> Result<ConcurrencyPermit, ResilienceError> {
        if !self.enabled {
            return Ok(ConcurrencyPermit {
                _permit: None,
                in_flight: None,
                service_name: self.service_name.clone(),
            });
        }

        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                record_concurrency_in_flight(&self.service_name, current);

                Ok(ConcurrencyPermit {
                    _permit: Some(permit),
                    in_flight: Some(Arc::clone(&self.in_flight)),
                    service_name: self.service_name.clone(),
                })
            }
            Err(TryAcquireError::NoPermits) => {
                record_load_shed_rejected("concurrency_saturated");
                warn!(
                    service = %self.service_name,
                    max_concurrent = self.max_concurrent,
                    "Adaptive LoadShedding triggered: maximum concurrent requests saturated. Fast-failing with 503."
                );
                Err(ResilienceError::LoadShedRejected {
                    retry_after_secs: 1,
                })
            }
            Err(TryAcquireError::Closed) => {
                record_load_shed_rejected("semaphore_closed");
                Err(ResilienceError::LoadShedRejected {
                    retry_after_secs: 1,
                })
            }
        }
    }

    /// Current number of in-flight requests.
    #[must_use]
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }

    /// Maximum concurrent requests threshold.
    #[must_use]
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

/// RAII Guard that releases the semaphore permit and updates metrics on drop.
#[derive(Debug)]
pub struct ConcurrencyPermit {
    _permit: Option<OwnedSemaphorePermit>,
    in_flight: Option<Arc<AtomicUsize>>,
    service_name: String,
}

impl Drop for ConcurrencyPermit {
    fn drop(&mut self) {
        if let Some(in_flight) = &self.in_flight {
            let prev = in_flight.fetch_sub(1, Ordering::SeqCst);
            let current = prev.saturating_sub(1);
            record_concurrency_in_flight(&self.service_name, current);
        }
    }
}

/// Tower Layer for concurrency limiting and adaptive load shedding.
#[derive(Debug, Clone)]
pub struct LoadShedLayer {
    controller: Arc<LoadShedController>,
}

impl LoadShedLayer {
    /// Creates a new `LoadShedLayer` from configuration.
    #[must_use]
    pub fn new(config: &ResilienceConfig, service_name: impl Into<String>) -> Self {
        Self {
            controller: Arc::new(LoadShedController::new(config, service_name)),
        }
    }
}

impl<S> Layer<S> for LoadShedLayer {
    type Service = LoadShedService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoadShedService {
            inner,
            controller: Arc::clone(&self.controller),
        }
    }
}

/// Tower Service implementation for concurrency limiting.
#[derive(Debug, Clone)]
pub struct LoadShedService<S> {
    inner: S,
    controller: Arc<LoadShedController>,
}

impl<S> Service<Request> for LoadShedService<S>
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

        match self.controller.try_acquire() {
            Ok(permit) => {
                let mut inner = self.inner.clone();
                Box::pin(async move {
                    let _permit = permit;
                    inner.call(req).await
                })
            }
            Err(err) => Box::pin(async move { Ok(err.to_response(&path, None)) }),
        }
    }
}
