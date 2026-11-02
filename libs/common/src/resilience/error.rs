//! Resilience error types and RFC 9457 Problem Details serialization.

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::borrow::Cow;
use thiserror::Error;

use crate::response::ResponseMeta;

/// Problem details URI definitions matching `OmniKubernetes` standards.
pub const TYPE_RATE_LIMITED: &str = "https://errors.omnikubernetes.com/rate-limited";
pub const TYPE_SERVICE_UNAVAILABLE: &str = "https://errors.omnikubernetes.com/service-unavailable";
pub const TYPE_GATEWAY_TIMEOUT: &str = "https://errors.omnikubernetes.com/gateway-timeout";

/// Machine-readable error codes.
pub const ERR_RATE_LIMITED: &str = "ERR_RATE_LIMITED";
pub const ERR_SERVICE_UNAVAILABLE: &str = "ERR_SERVICE_UNAVAILABLE";
pub const ERR_TIMEOUT: &str = "ERR_TIMEOUT";

/// Standard MIME type for RFC 9457.
pub const PROBLEM_JSON_MIME: &str = "application/problem+json; charset=utf-8";

/// Standard IETF `RateLimit` Header constants.
pub const HEADER_RATE_LIMIT_LIMIT: &str = "ratelimit-limit";
pub const HEADER_RATE_LIMIT_REMAINING: &str = "ratelimit-remaining";
pub const HEADER_RATE_LIMIT_RESET: &str = "ratelimit-reset";

/// Categorized resilience errors mapped to RFC 9457 responses.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ResilienceError {
    /// HTTP 429 Too Many Requests (Rate limit quota exceeded).
    #[error("Rate limit exceeded. Quota resets in {reset_secs} seconds.")]
    RateLimited {
        /// Retry-After duration in seconds.
        retry_after_secs: u64,
        /// Maximum allowed requests in window.
        limit: u64,
        /// Remaining requests in current window.
        remaining: u64,
        /// Seconds remaining until full reset.
        reset_secs: u64,
    },

    /// HTTP 503 Service Unavailable (Concurrency limit / Adaptive Load Shedding).
    #[error("Server is temporarily overloaded. Please retry later.")]
    LoadShedRejected {
        /// Recommended retry-after backoff in seconds (typically 1).
        retry_after_secs: u64,
    },

    /// HTTP 503 Service Unavailable (Circuit Breaker is OPEN).
    #[error("Downstream service '{target_service}' circuit breaker is OPEN.")]
    CircuitBreakerOpen {
        /// Target downstream dependency name.
        target_service: String,
        /// Recommended retry-after backoff in seconds until probe.
        retry_after_secs: u64,
    },

    /// HTTP 504 Gateway Timeout (Execution deadline exceeded).
    #[error("Request execution exceeded deadline of {timeout_secs} seconds.")]
    RequestTimeout {
        /// Configured timeout threshold in seconds.
        timeout_secs: u64,
    },
}

/// RFC 9457 Problem Details payload structure for resilience errors.
#[derive(Debug, Clone, Serialize)]
pub struct ProblemDetailsPayload {
    #[serde(rename = "type")]
    pub problem_type: Cow<'static, str>,
    pub title: Cow<'static, str>,
    pub status: u16,
    pub detail: Cow<'static, str>,
    pub instance: Cow<'static, str>,
    pub code: Cow<'static, str>,
    pub meta: ResponseMeta,
}

impl ResilienceError {
    /// HTTP status code for this resilience error.
    #[must_use]
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::LoadShedRejected { .. } | Self::CircuitBreakerOpen { .. } => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::RequestTimeout { .. } => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// Machine-readable error code string.
    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::RateLimited { .. } => ERR_RATE_LIMITED,
            Self::LoadShedRejected { .. } | Self::CircuitBreakerOpen { .. } => {
                ERR_SERVICE_UNAVAILABLE
            }
            Self::RequestTimeout { .. } => ERR_TIMEOUT,
        }
    }

    /// RFC 9457 type URI.
    #[must_use]
    pub fn problem_type(&self) -> &'static str {
        match self {
            Self::RateLimited { .. } => TYPE_RATE_LIMITED,
            Self::LoadShedRejected { .. } | Self::CircuitBreakerOpen { .. } => {
                TYPE_SERVICE_UNAVAILABLE
            }
            Self::RequestTimeout { .. } => TYPE_GATEWAY_TIMEOUT,
        }
    }

    /// Human-readable title for problem details.
    #[must_use]
    pub fn title(&self) -> &'static str {
        match self {
            Self::RateLimited { .. } => "Too Many Requests",
            Self::LoadShedRejected { .. } | Self::CircuitBreakerOpen { .. } => {
                "Service Unavailable"
            }
            Self::RequestTimeout { .. } => "Gateway Timeout",
        }
    }

    /// Human-readable detailed description.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::RateLimited { reset_secs, .. } => {
                format!("Rate limit exceeded. Quota resets in {reset_secs} seconds.")
            }
            Self::LoadShedRejected { .. } => {
                "Server is temporarily overloaded. Please retry later.".to_string()
            }
            Self::CircuitBreakerOpen { target_service, .. } => {
                format!(
                    "Downstream service '{target_service}' circuit breaker is OPEN. Fast failing request."
                )
            }
            Self::RequestTimeout { timeout_secs } => {
                format!("Request execution exceeded deadline of {timeout_secs} seconds.")
            }
        }
    }

    /// Converts this error into a formatted Axum HTTP [`Response`].
    #[must_use]
    pub fn to_response(&self, instance: &str, meta: Option<ResponseMeta>) -> Response {
        let status = self.status_code();
        let meta = meta.unwrap_or_default();

        let payload = ProblemDetailsPayload {
            problem_type: Cow::Borrowed(self.problem_type()),
            title: Cow::Borrowed(self.title()),
            status: status.as_u16(),
            detail: Cow::Owned(self.detail()),
            instance: Cow::Owned(instance.to_string()),
            code: Cow::Borrowed(self.error_code()),
            meta,
        };

        let json_body = serde_json::to_string(&payload).unwrap_or_else(|_| {
            r#"{"type":"about:blank","title":"Internal Error","status":500,"detail":"Serialization failure","instance":"/","code":"ERR_INTERNAL"}"#.to_string()
        });

        let mut res = Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, PROBLEM_JSON_MIME)
            .body(Body::from(json_body))
            .unwrap_or_else(|_| Response::new(Body::empty()));

        let headers = res.headers_mut();

        match self {
            Self::RateLimited {
                retry_after_secs,
                limit,
                remaining,
                reset_secs,
            } => {
                if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                    headers.insert(header::RETRY_AFTER, v);
                }
                if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
                    headers.insert(HEADER_RATE_LIMIT_LIMIT, v);
                }
                if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
                    headers.insert(HEADER_RATE_LIMIT_REMAINING, v);
                }
                if let Ok(v) = HeaderValue::from_str(&reset_secs.to_string()) {
                    headers.insert(HEADER_RATE_LIMIT_RESET, v);
                }
            }
            Self::LoadShedRejected { retry_after_secs }
            | Self::CircuitBreakerOpen {
                retry_after_secs, ..
            } => {
                if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                    headers.insert(header::RETRY_AFTER, v);
                }
            }
            Self::RequestTimeout { .. } => {}
        }

        res
    }
}

impl IntoResponse for ResilienceError {
    fn into_response(self) -> Response {
        self.to_response("/", None)
    }
}
