//! Comprehensive Observability, W3C Distributed Context, Span Recording & Access Logging Middleware.
//!
//! Standardizes the Ingress & Egress boundary of Axum microservices by:
//! 1. Filtering infrastructure noise (`/healthz`, `/livez`, `/readyz`, `/metrics`).
//! 2. Correlating `request_id` (`UUIDv7`) and W3C `trace_id` / `span_id`.
//! 3. Creating and instrumenting OpenTelemetry-compliant `tracing::Span` without holding guards across `.await`.
//! 4. Calculating zero-cost latency and injecting `Server-Timing`, `X-Response-Time`, `traceparent`, `X-Request-Id`.
//! 5. Emitting structured Newline-Delimited JSON Access Logs.
//! 6. Providing downstream egress propagation utilities for HTTP and gRPC.

use std::{borrow::Cow, time::Instant};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{
    i18n::extractor::RequestLocale,
    middleware::{
        context::{RequestContext, extract_trusted_client_ip},
        traceparent::{
            TRACEPARENT_HEADER, TRACESTATE_HEADER, extract_or_generate_request_id,
            extract_or_generate_trace_context, format_traceparent, generate_span_id,
        },
    },
    response::X_REQUEST_ID_HEADER,
};

/// W3C Server-Timing header name.
pub const SERVER_TIMING_HEADER: &str = "server-timing";

/// Legacy response time header name.
pub const X_RESPONSE_TIME_HEADER: &str = "x-response-time";

/// Access log tracing target name.
pub const ACCESS_LOG_TARGET: &str = "omni_common::access_log";

/// Checks whether an HTTP request path is an infrastructure health check or Prometheus scrape endpoint.
#[inline]
pub fn is_health_probe_path(path: &str) -> bool {
    matches!(
        path,
        "/healthz" | "/livez" | "/readyz" | "/startupz" | "/metrics" | "/health"
    )
}

/// Unified observability middleware orchestrating distributed context, tracing span, and egress headers.
pub async fn observability_middleware(mut req: Request<Body>, next: Next) -> Response {
    let start_instant = Instant::now();
    let path = req.uri().path().to_string();

    // 1. Noise Filter: Skip tracing span creation and access logs for Kubernetes probes and metrics
    if is_health_probe_path(&path) {
        return next.run(req).await;
    }

    // 2. Correlation IDs extraction / generation
    let request_id = extract_or_generate_request_id(req.headers());
    let (trace_id, parent_span_id, is_sampled, tracestate) =
        extract_or_generate_trace_context(req.headers());
    let current_span_id = generate_span_id();

    // 3. Network & Client metadata
    let client_ip = extract_trusted_client_ip(req.headers());
    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    // 4. Localization waterfall
    let request_locale = RequestLocale::resolve(req.uri(), req.headers(), req.extensions());
    let locale_str = request_locale.0.clone();

    // 5. Encapsulate into RequestContext and inject into request extensions
    let context = RequestContext::full(
        Cow::Owned(request_id.clone()),
        Cow::Owned(trace_id.clone()),
        Cow::Owned(current_span_id.clone()),
        parent_span_id.map(Cow::Owned),
        tracestate.as_deref().map(|s| Cow::Owned(s.to_string())),
        is_sampled,
        locale_str.clone(),
        Cow::Owned(path.clone()),
        client_ip.map(Cow::Owned),
        user_agent.map(Cow::Owned),
    );

    req.extensions_mut().insert(context.clone());
    req.extensions_mut().insert(request_locale.clone());

    // 6. Root Tracing Span creation following OpenTelemetry Semantic Conventions v1.28+
    let method = req.method().to_string();
    let span = tracing::info_span!(
        "http_request",
        "http.request.method" = %method,
        "http.route" = %path,
        "url.path" = %path,
        "request_id" = %request_id,
        "trace_id" = %trace_id,
        "span_id" = %current_span_id,
        "http.response.status_code" = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
        "user_id" = tracing::field::Empty,
        "tenant_id" = tracing::field::Empty,
    );

    // Kế thừa Parent Context từ OTel nếu có header W3C hợp lệ
    let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(req.headers()))
    });
    let _ = span.set_parent(parent_cx);

    // 7. Execute downstream handler wrapped in the span instrument (Thread-Safe Async)
    let mut response = next.run(req).instrument(span.clone()).await;

    // 8. Record execution metrics & status
    let status_code = response.status().as_u16();
    span.record("http.response.status_code", status_code);

    let duration_ms = start_instant.elapsed().as_secs_f64() * 1000.0;

    if response.status().is_server_error() {
        span.record("error.type", "INTERNAL_SERVER_ERROR");
    }

    // 9. Single Source of Truth: Egress Response Header Injection
    inject_response_headers(
        &mut response,
        &request_id,
        &trace_id,
        &current_span_id,
        is_sampled,
        duration_ms,
        locale_str.as_ref(),
        tracestate.as_deref(),
    );

    // 10. Structured JSON Access Logging attached to the Span
    tracing::info!(
        target: "omni_common::access_log",
        parent: &span,
        method = %method,
        path = %path,
        status = status_code,
        duration_ms = duration_ms,
        request_id = %request_id,
        trace_id = %trace_id,
        "Handled HTTP request"
    );

    response
}

/// Injects standard egress headers into the outgoing HTTP response.
#[allow(clippy::too_many_arguments)]
pub fn inject_response_headers(
    res: &mut Response,
    request_id: &str,
    trace_id: &str,
    span_id: &str,
    is_sampled: bool,
    duration_ms: f64,
    locale: &str,
    tracestate: Option<&str>,
) {
    let headers = res.headers_mut();

    // 1. X-Request-Id
    if let Ok(val) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static(X_REQUEST_ID_HEADER), val);
    }

    // 2. W3C traceparent
    let traceparent_resp = format_traceparent(trace_id, span_id, is_sampled);
    if let Ok(val) = HeaderValue::from_str(&traceparent_resp) {
        headers.insert(HeaderName::from_static(TRACEPARENT_HEADER), val);
    }

    // 3. W3C tracestate (if present)
    if let Some(state) = tracestate
        && let Ok(val) = HeaderValue::from_str(state)
    {
        headers.insert(HeaderName::from_static(TRACESTATE_HEADER), val);
    }

    // 4. W3C Server-Timing: total;dur=12.40
    let timing = format!("total;dur={duration_ms:.2}");
    if let Ok(val) = HeaderValue::from_str(&timing) {
        headers.insert(HeaderName::from_static(SERVER_TIMING_HEADER), val);
    }

    // 5. X-Response-Time legacy APM compatibility
    let x_time = format!("{duration_ms:.2}ms");
    if let Ok(val) = HeaderValue::from_str(&x_time) {
        headers.insert(HeaderName::from_static(X_RESPONSE_TIME_HEADER), val);
    }

    // 6. Content-Language
    if let Ok(val) = HeaderValue::from_str(locale) {
        headers.insert(header::CONTENT_LANGUAGE, val);
    }
}

/// Injects W3C Distributed Context and correlation headers into an outgoing HTTP/gRPC [`HeaderMap`].
pub fn inject_egress_headers(headers: &mut HeaderMap, ctx: &RequestContext) {
    if let Ok(val) = HeaderValue::from_str(ctx.request_id.as_ref()) {
        headers.insert(HeaderName::from_static(X_REQUEST_ID_HEADER), val);
    }

    // Use current caller span_id as parent_id for the downstream callee
    let traceparent =
        format_traceparent(ctx.trace_id.as_ref(), ctx.span_id.as_ref(), ctx.is_sampled);
    if let Ok(val) = HeaderValue::from_str(&traceparent) {
        headers.insert(HeaderName::from_static(TRACEPARENT_HEADER), val);
    }

    if let Some(state) = &ctx.tracestate
        && let Ok(val) = HeaderValue::from_str(state.as_ref())
    {
        headers.insert(HeaderName::from_static(TRACESTATE_HEADER), val);
    }

    if let Ok(val) = HeaderValue::from_str(ctx.locale.as_ref()) {
        headers.insert(HeaderName::from_static("x-locale"), val);
    }
}

/// Injects W3C traceparent, tracestate, X-Request-Id and X-Locale into an outgoing HTTP Reqwest request builder.
pub fn inject_egress_context(
    mut builder: reqwest::RequestBuilder,
    ctx: &RequestContext,
) -> reqwest::RequestBuilder {
    let flags = if ctx.is_sampled { "01" } else { "00" };
    // Use current caller span_id as parent_id for the downstream callee
    let traceparent = format!("00-{}-{}-{}", ctx.trace_id, ctx.span_id, flags);

    builder = builder
        .header(X_REQUEST_ID_HEADER, ctx.request_id.as_ref())
        .header(TRACEPARENT_HEADER, traceparent)
        .header("x-locale", ctx.locale.as_ref());

    if let Some(state) = &ctx.tracestate {
        builder = builder.header(TRACESTATE_HEADER, state.as_ref());
    }

    builder
}

/// Injects W3C traceparent, tracestate, X-Request-Id and X-Locale into an outgoing gRPC request metadata.
pub fn inject_grpc_egress_context<T>(req: &mut tonic::Request<T>, ctx: &RequestContext) {
    let metadata = req.metadata_mut();
    let flags = if ctx.is_sampled { "01" } else { "00" };
    let traceparent = format!("00-{}-{}-{}", ctx.trace_id, ctx.span_id, flags);

    if let Ok(val) = tonic::metadata::MetadataValue::try_from(ctx.request_id.as_ref()) {
        metadata.insert(X_REQUEST_ID_HEADER, val);
    }
    if let Ok(val) = tonic::metadata::MetadataValue::try_from(&traceparent) {
        metadata.insert(TRACEPARENT_HEADER, val);
    }
    if let Some(state) = &ctx.tracestate
        && let Ok(val) = tonic::metadata::MetadataValue::try_from(state.as_ref())
    {
        metadata.insert(TRACESTATE_HEADER, val);
    }
    if let Ok(val) = tonic::metadata::MetadataValue::try_from(ctx.locale.as_ref()) {
        metadata.insert("x-locale", val);
    }
}

/// Helper extracting HTTP headers for OpenTelemetry `TextMapPropagator`
struct HeaderExtractor<'a>(&'a HeaderMap);

impl opentelemetry::propagation::Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(axum::http::HeaderName::as_str).collect()
    }
}
