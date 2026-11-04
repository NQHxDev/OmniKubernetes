//! High-Level Domain & Application Error Adapter (`AppError`).
//!
//! Applies the Adapter Pattern to translate domain/service errors into RFC 9457 [`ApiErrorResponse`]
//! contracts with automatic Mozilla Fluent localization and Section 8.1 security hardening.

use std::borrow::Cow;

use axum::response::{IntoResponse, Response};
use omni_common::{i18n::I18nManager, middleware::RequestContext, response::ResponseMeta};
use thiserror::Error;

use crate::problem::{ApiErrorResponse, InvalidParamError};

/// Standardized domain error enumeration that maps cleanly to RFC 9457 [`ApiErrorResponse`].
#[derive(Debug, Clone, Error)]
pub enum AppError {
    /// HTTP 400 Bad Request.
    #[error("Bad request")]
    BadRequest {
        /// Optional custom detail override.
        detail: Option<Cow<'static, str>>,
    },

    /// HTTP 401 Unauthorized.
    #[error("Unauthorized")]
    Unauthorized,

    /// HTTP 403 Forbidden.
    #[error("Forbidden")]
    Forbidden,

    /// HTTP 404 Not Found.
    #[error("Resource not found")]
    NotFound,

    /// HTTP 405 Method Not Allowed.
    #[error("Method not allowed")]
    MethodNotAllowed,

    /// HTTP 409 Conflict.
    #[error("Conflict")]
    Conflict {
        /// Optional custom detail override.
        detail: Option<Cow<'static, str>>,
    },

    /// HTTP 422 Unprocessable Entity (Validation Error).
    #[error("Validation failed with {} field violation(s)", .0.len())]
    ValidationFailed(Vec<InvalidParamError>),

    /// HTTP 429 Too Many Requests.
    #[error("Rate limit exceeded (retry after {retry_after_secs}s)")]
    RateLimited {
        /// Seconds until the client may retry.
        retry_after_secs: u64,
    },

    /// HTTP 500 Internal Server Error (internal details logged only, never exposed to client).
    #[error("Internal server error: {0}")]
    Internal(String),

    /// HTTP 503 Service Unavailable.
    #[error("Service unavailable")]
    ServiceUnavailable,

    /// Pre-constructed [`ApiErrorResponse`].
    #[error("{0:?}")]
    Problem(Box<ApiErrorResponse>),
}

impl AppError {
    /// Converts this [`AppError`] into a localized RFC 9457 [`ApiErrorResponse`] using [`RequestContext`] and [`I18nManager`].
    pub fn into_problem_with_context(
        self,
        ctx: &RequestContext,
        i18n: &I18nManager,
    ) -> ApiErrorResponse {
        self.into_problem(ctx.instance.clone(), ctx.to_meta(), i18n)
    }

    /// Converts this [`AppError`] into a localized RFC 9457 [`ApiErrorResponse`].
    pub fn into_problem(
        self,
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> ApiErrorResponse {
        match self {
            Self::BadRequest { detail } => {
                let mut resp = ApiErrorResponse::bad_request(instance, meta, i18n);
                if let Some(d) = detail {
                    resp.detail = d;
                }
                resp
            }
            Self::Unauthorized => ApiErrorResponse::unauthorized(instance, meta, i18n),
            Self::Forbidden => ApiErrorResponse::forbidden(instance, meta, i18n),
            Self::NotFound => ApiErrorResponse::not_found(instance, meta, i18n),
            Self::MethodNotAllowed => ApiErrorResponse::method_not_allowed(instance, meta, i18n),
            Self::Conflict { detail } => {
                let mut resp = ApiErrorResponse::conflict(instance, meta, i18n);
                if let Some(d) = detail {
                    resp.detail = d;
                }
                resp
            }
            Self::ValidationFailed(params) => {
                ApiErrorResponse::validation_failed(params, instance, meta, i18n)
            }
            Self::RateLimited { retry_after_secs } => {
                ApiErrorResponse::rate_limited(retry_after_secs, instance, meta, i18n)
            }
            Self::Internal(cause) => {
                ApiErrorResponse::internal_server_error(cause, instance, meta, i18n)
            }
            Self::ServiceUnavailable => ApiErrorResponse::service_unavailable(instance, meta, i18n),
            Self::Problem(problem) => *problem,
        }
    }
}

impl From<ApiErrorResponse> for AppError {
    fn from(problem: ApiErrorResponse) -> Self {
        Self::Problem(Box::new(problem))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let i18n = I18nManager::default();
        let meta = ResponseMeta::default();
        self.into_problem("/", meta, &i18n).into_response()
    }
}
