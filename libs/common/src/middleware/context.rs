//! Request Context Extraction, Span Correlation, and Egress Header Enforcement (`RequestContext`).
//!
//! Connects Envoy Gateway / Ingress headers (`X-Request-Id`, `traceparent`, `Accept-Language`, `X-Locale`)
//! with Axum handlers, `tracing` spans, and [`ResponseMeta`].

use std::{borrow::Cow, convert::Infallible};

use axum::{
    body::Body,
    extract::FromRequestParts,
    http::{HeaderName, HeaderValue, Request, Response, header, request::Parts},
    middleware::Next,
};
use chrono::{DateTime, Utc};

use crate::{
    i18n::extractor::RequestLocale,
    middleware::traceparent::{extract_or_generate_request_id, extract_or_generate_trace_id},
    response::{ResponseMeta, X_REQUEST_ID_HEADER},
};

/// Per-request observability and localization context extracted at the middleware ingress boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    /// Unique request identifier (`UUIDv7`) for 100% log correlation.
    pub request_id: Cow<'static, str>,
    /// W3C Distributed Trace ID (32 lowercase hex characters) for APM span correlation.
    pub trace_id: Cow<'static, str>,
    /// Canonical BCP 47 locale resolved via the 5-step waterfall.
    pub locale: Cow<'static, str>,
    /// Request URI path (used as RFC 9457 `instance`).
    pub instance: Cow<'static, str>,
    /// UTC timestamp when the request context was initialized.
    pub timestamp: DateTime<Utc>,
}

impl RequestContext {
    /// Creates a new [`RequestContext`].
    pub fn new(
        request_id: impl Into<Cow<'static, str>>,
        trace_id: impl Into<Cow<'static, str>>,
        locale: impl Into<Cow<'static, str>>,
        instance: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            locale: locale.into(),
            instance: instance.into(),
            timestamp: Utc::now(),
        }
    }

    /// Extracts or constructs a [`RequestContext`] from Axum request [`Parts`].
    pub fn from_parts(parts: &Parts) -> Self {
        if let Some(existing) = parts.extensions.get::<RequestContext>() {
            return existing.clone();
        }

        let request_id = extract_or_generate_request_id(&parts.headers);
        let trace_id = extract_or_generate_trace_id(&parts.headers);
        let locale = RequestLocale::from_parts(parts).into_inner();
        let instance = parts.uri.path().to_string();

        Self {
            request_id: Cow::Owned(request_id),
            trace_id: Cow::Owned(trace_id),
            locale,
            instance: Cow::Owned(instance),
            timestamp: Utc::now(),
        }
    }

    /// Converts this [`RequestContext`] into a [`ResponseMeta`] with a fresh `Utc::now()` response timestamp.
    pub fn to_meta(&self) -> ResponseMeta {
        ResponseMeta {
            request_id: self.request_id.clone(),
            trace_id: self.trace_id.clone(),
            timestamp: Utc::now(),
            locale: self.locale.clone(),
        }
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        let meta = ResponseMeta::default();
        Self {
            request_id: meta.request_id,
            trace_id: meta.trace_id,
            locale: meta.locale,
            instance: Cow::Borrowed("/"),
            timestamp: meta.timestamp,
        }
    }
}

impl From<RequestContext> for ResponseMeta {
    fn from(ctx: RequestContext) -> Self {
        ResponseMeta {
            request_id: ctx.request_id,
            trace_id: ctx.trace_id,
            timestamp: Utc::now(),
            locale: ctx.locale,
        }
    }
}

impl From<&RequestContext> for ResponseMeta {
    fn from(ctx: &RequestContext) -> Self {
        ctx.to_meta()
    }
}

impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = Self::from_parts(parts);
        std::future::ready(Ok(ctx))
    }
}

impl<S> FromRequestParts<S> for ResponseMeta
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = RequestContext::from_parts(parts);
        std::future::ready(Ok(ctx.to_meta()))
    }
}

/// Middleware that extracts [`RequestContext`] and [`RequestLocale`], records `request_id` and `trace_id`
/// on the active `tracing::Span`, injects them into request extensions, and guarantees
/// `Content-Language` and `X-Request-Id` headers on the outgoing HTTP response.
pub async fn request_context_middleware(mut req: Request<Body>, next: Next) -> Response<Body> {
    let request_id = extract_or_generate_request_id(req.headers());
    let trace_id = extract_or_generate_trace_id(req.headers());
    let request_locale = RequestLocale::resolve(req.uri(), req.headers(), req.extensions());
    let instance = req.uri().path().to_string();

    // Record correlation IDs into the current tracing Span
    let current_span = tracing::Span::current();
    current_span.record("request_id", request_id.as_str());
    current_span.record("trace_id", trace_id.as_str());

    let ctx = RequestContext::new(
        Cow::Owned(request_id.clone()),
        Cow::Owned(trace_id),
        request_locale.0.clone(),
        Cow::Owned(instance),
    );

    req.extensions_mut().insert(request_locale.clone());
    req.extensions_mut().insert(ctx);

    let mut response = next.run(req).await;

    // Ensure Content-Language and X-Request-Id headers are always present on the outgoing response
    if !response.headers().contains_key(header::CONTENT_LANGUAGE)
        && let Ok(val) = HeaderValue::from_str(request_locale.as_str())
    {
        response.headers_mut().insert(header::CONTENT_LANGUAGE, val);
    }

    if !response.headers().contains_key(X_REQUEST_ID_HEADER)
        && let Ok(val) = HeaderValue::from_str(&request_id)
    {
        response
            .headers_mut()
            .insert(HeaderName::from_static(X_REQUEST_ID_HEADER), val);
    }

    response
}
