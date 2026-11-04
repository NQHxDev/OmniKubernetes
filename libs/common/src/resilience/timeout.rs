//! Multi-tier request timeout, cancellation propagation, and deadline injection.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue},
    response::Response,
};
use thiserror::Error;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tower::{Layer, Service};

use crate::resilience::error::ResilienceError;

/// Standard HTTP Header for propagating request deadlines downstream.
pub const HEADER_REQUEST_TIMEOUT: &str = "request-timeout";

/// Error returned when an operation is cancelled via [`CancellationToken`].
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("Operation cancelled by downstream caller or timeout")]
pub struct CancelledError;

/// Executes a future linked to a [`CancellationToken`], cancelling execution if token triggers.
pub async fn with_cancellation_token<F, T>(
    token: &CancellationToken,
    future: F,
) -> Result<T, CancelledError>
where
    F: Future<Output = T>,
{
    tokio::select! {
        () = token.cancelled() => Err(CancelledError),
        res = future => Ok(res),
    }
}

/// Injects remaining deadline into outgoing downstream HTTP request headers.
pub fn inject_deadline_header(headers: &mut HeaderMap, remaining: Duration) {
    let secs = remaining.as_secs_f64();
    if let Ok(val) = HeaderValue::from_str(&format!("{secs:.3}")) {
        headers.insert(HEADER_REQUEST_TIMEOUT, val);
    }
}

/// Tower Layer for imposing execution deadlines on HTTP requests.
#[derive(Debug, Clone)]
pub struct TimeoutLayer {
    timeout_duration: Duration,
}

impl TimeoutLayer {
    /// Creates a new `TimeoutLayer` with the specified duration.
    #[must_use]
    pub fn new(timeout_duration: Duration) -> Self {
        Self { timeout_duration }
    }
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = TimeoutService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimeoutService {
            inner,
            timeout_duration: self.timeout_duration,
        }
    }
}

/// Tower Service implementation for request timeout.
#[derive(Debug, Clone)]
pub struct TimeoutService<S> {
    inner: S,
    timeout_duration: Duration,
}

impl<S> Service<Request> for TimeoutService<S>
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
        let timeout_duration = self.timeout_duration;
        let mut inner = self.inner.clone();

        Box::pin(async move {
            if let Ok(response_result) = timeout(timeout_duration, inner.call(req)).await {
                response_result
            } else {
                let err = ResilienceError::RequestTimeout {
                    timeout_secs: timeout_duration.as_secs().max(1),
                };
                Ok(err.to_response(&path, None))
            }
        })
    }
}
