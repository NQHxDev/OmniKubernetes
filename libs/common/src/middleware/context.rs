//! Request Context Extraction, Span Correlation, and Infallible Extractor (`RequestContext`).
//!
//! Connects Envoy Gateway / Ingress headers (`X-Request-Id`, `traceparent`, `tracestate`, `Accept-Language`, `X-Locale`)
//! with Axum handlers, `tracing` spans, and [`ResponseMeta`].

use std::{borrow::Cow, convert::Infallible, net::IpAddr};

use axum::{
    body::Body,
    extract::FromRequestParts,
    http::{HeaderMap, HeaderName, HeaderValue, Request, Response, header, request::Parts},
    middleware::Next,
};
use chrono::{DateTime, Utc};

use crate::{
    i18n::extractor::RequestLocale,
    middleware::traceparent::{
        extract_or_generate_request_id, extract_or_generate_trace_context,
        extract_or_generate_trace_id, generate_span_id,
    },
    response::{ResponseMeta, X_REQUEST_ID_HEADER},
};

/// Per-request observability and localization context extracted at the middleware ingress boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    /// Unique request identifier (`UUIDv7`) for 100% log correlation.
    pub request_id: Cow<'static, str>,
    /// W3C Distributed Trace ID (32 lowercase hex characters) for APM span correlation.
    pub trace_id: Cow<'static, str>,
    /// Active Span ID of the current service handling the request (16 hex chars).
    pub span_id: Cow<'static, str>,
    /// Parent Span ID if request was received from an upstream service.
    pub parent_span_id: Option<Cow<'static, str>>,
    /// W3C `tracestate` header preserving vendor-specific routing tags.
    pub tracestate: Option<Cow<'static, str>>,
    /// Whether this request is sampled for distributed tracing.
    pub is_sampled: bool,
    /// Canonical BCP 47 locale resolved via the 5-step waterfall.
    pub locale: Cow<'static, str>,
    /// Request URI path (used as RFC 9457 `instance`).
    pub instance: Cow<'static, str>,
    /// UTC timestamp when the request context was initialized.
    pub timestamp: DateTime<Utc>,
    /// Client IP address verified via trusted proxy chain.
    pub client_ip: Option<Cow<'static, str>>,
    /// User-Agent of the invoking client.
    pub user_agent: Option<Cow<'static, str>>,
    /// Authenticated User ID populated by auth middleware.
    pub user_id: Option<Cow<'static, str>>,
    /// Tenant ID for multi-tenant deployments.
    pub tenant_id: Option<Cow<'static, str>>,
}

impl RequestContext {
    /// Creates a basic [`RequestContext`] (backward-compatible constructor).
    pub fn new(
        request_id: impl Into<Cow<'static, str>>,
        trace_id: impl Into<Cow<'static, str>>,
        locale: impl Into<Cow<'static, str>>,
        instance: impl Into<Cow<'static, str>>,
    ) -> Self {
        let span_id = generate_span_id();
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            span_id: Cow::Owned(span_id),
            parent_span_id: None,
            tracestate: None,
            is_sampled: true,
            locale: locale.into(),
            instance: instance.into(),
            timestamp: Utc::now(),
            client_ip: None,
            user_agent: None,
            user_id: None,
            tenant_id: None,
        }
    }

    /// Creates a comprehensive [`RequestContext`] with all distributed tracing attributes.
    #[allow(clippy::too_many_arguments)]
    pub fn full(
        request_id: impl Into<Cow<'static, str>>,
        trace_id: impl Into<Cow<'static, str>>,
        span_id: impl Into<Cow<'static, str>>,
        parent_span_id: Option<impl Into<Cow<'static, str>>>,
        tracestate: Option<impl Into<Cow<'static, str>>>,
        is_sampled: bool,
        locale: impl Into<Cow<'static, str>>,
        instance: impl Into<Cow<'static, str>>,
        client_ip: Option<impl Into<Cow<'static, str>>>,
        user_agent: Option<impl Into<Cow<'static, str>>>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_span_id: parent_span_id.map(Into::into),
            tracestate: tracestate.map(Into::into),
            is_sampled,
            locale: locale.into(),
            instance: instance.into(),
            timestamp: Utc::now(),
            client_ip: client_ip.map(Into::into),
            user_agent: user_agent.map(Into::into),
            user_id: None,
            tenant_id: None,
        }
    }

    /// Extracts or constructs a [`RequestContext`] from Axum request [`Parts`].
    pub fn from_parts(parts: &Parts) -> Self {
        if let Some(existing) = parts.extensions.get::<RequestContext>() {
            return existing.clone();
        }

        let request_id = extract_or_generate_request_id(&parts.headers);
        let (trace_id, parent_span_id, is_sampled, tracestate) =
            extract_or_generate_trace_context(&parts.headers);
        let current_span_id = generate_span_id();
        let locale = RequestLocale::from_parts(parts).into_inner();
        let instance = parts.uri.path().to_string();
        let client_ip = extract_trusted_client_ip(&parts.headers);
        let user_agent = parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);

        Self {
            request_id: Cow::Owned(request_id),
            trace_id: Cow::Owned(trace_id),
            span_id: Cow::Owned(current_span_id),
            parent_span_id: parent_span_id.map(Cow::Owned),
            tracestate: tracestate.map(Cow::Owned),
            is_sampled,
            locale,
            instance: Cow::Owned(instance),
            timestamp: Utc::now(),
            client_ip: client_ip.map(Cow::Owned),
            user_agent: user_agent.map(Cow::Owned),
            user_id: None,
            tenant_id: None,
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
        let span_id = generate_span_id();
        Self {
            request_id: meta.request_id,
            trace_id: meta.trace_id,
            span_id: Cow::Owned(span_id),
            parent_span_id: None,
            tracestate: None,
            is_sampled: true,
            locale: meta.locale,
            instance: Cow::Borrowed("/"),
            timestamp: meta.timestamp,
            client_ip: None,
            user_agent: None,
            user_id: None,
            tenant_id: None,
        }
    }
}

impl From<RequestContext> for ResponseMeta {
    fn from(ctx: RequestContext) -> Self {
        ctx.to_meta()
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
        let ctx = if let Some(existing) = parts.extensions.get::<RequestContext>() {
            existing.clone()
        } else {
            let created = Self::from_parts(parts);
            parts.extensions.insert(created.clone());
            created
        };
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
        let meta = if let Some(existing) = parts.extensions.get::<RequestContext>() {
            existing.to_meta()
        } else {
            let created = RequestContext::from_parts(parts);
            parts.extensions.insert(created.clone());
            created.to_meta()
        };
        std::future::ready(Ok(meta))
    }
}

/// Safely extracts trusted client IP address, parsed and validated as [`IpAddr`].
pub fn extract_trusted_client_ip(headers: &HeaderMap) -> Option<String> {
    // Header injected by Envoy Gateway after ingress termination
    if let Some(envoy_ip) = headers
        .get("x-envoy-external-address")
        .and_then(|v| v.to_str().ok())
    {
        let trimmed = envoy_ip.trim();
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            return Some(ip.to_string());
        }
    }

    // X-Real-IP from trusted Ingress Controller
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = real_ip.trim();
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            return Some(ip.to_string());
        }
    }

    // Fallback X-Forwarded-For: leftmost client address
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first_ip) = xff.split(',').next()
    {
        let trimmed = first_ip.trim();
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            return Some(ip.to_string());
        }
    }

    None
}

/// Legacy middleware that extracts [`RequestContext`] and [`RequestLocale`], records `request_id` and `trace_id`
/// on the active `tracing::Span`, and guarantees `Content-Language` and `X-Request-Id` headers.
///
/// NOTE: For comprehensive OpenTelemetry tracing, noise filtering, Server-Timing, and Access Logging,
/// prefer using [`crate::middleware::observability_middleware`].
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
