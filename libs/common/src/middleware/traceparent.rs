//! W3C Trace Context (`traceparent`, `tracestate`) and `X-Request-Id` Extraction & Generation.
//!
//! Implements two-tier observability correlation:
//! - `request_id`: `UUIDv7` generated per request (100% coverage for Loki/ELK log aggregation).
//! - `trace_id`: 32-character lowercase hex identifier parsed from W3C `traceparent` header
//!   (for OpenTelemetry / Grafana Tempo / Jaeger distributed tracing).
//! - `tracestate`: Optional opaque vendor routing and state pairs preserved across spans.

use axum::http::HeaderMap;
use uuid::Uuid;

use crate::response::X_REQUEST_ID_HEADER;

/// Standard W3C Trace Context header name (`traceparent`).
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Standard W3C Trace State header name (`tracestate`).
pub const TRACESTATE_HEADER: &str = "tracestate";

/// Optional fallback trace ID header name (`x-trace-id`).
pub const X_TRACE_ID_HEADER: &str = "x-trace-id";

/// Parsed W3C Trace Context Level 1 fields (`version-trace_id-parent_id-trace_flags`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// Version byte as 2 lowercase hex characters (e.g. `"00"`).
    pub version: String,
    /// 32-character lowercase hex trace ID.
    pub trace_id: String,
    /// 16-character lowercase hex parent span ID.
    pub parent_id: String,
    /// 2-character lowercase hex trace flags (e.g. `"01"` for sampled).
    pub trace_flags: String,
    /// Whether the sampled flag bit (`0x01`) is active.
    pub is_sampled: bool,
    /// Optional W3C `tracestate` header value.
    pub tracestate: Option<String>,
}

/// Parses a W3C `traceparent` header string according to W3C Trace Context Level 1 specification.
///
/// Format: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
pub fn parse_traceparent(raw: &str) -> Option<TraceContext> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('-');

    let version = parts.next()?;
    let trace_id = parts.next()?;
    let parent_id = parts.next()?;
    let trace_flags = parts.next()?;

    // Version "00" forbids trailing fields; version "ff" is invalid per W3C spec
    if version.len() != 2 || !is_lowercase_hex(version) || version == "ff" {
        return None;
    }
    if version == "00" && parts.next().is_some() {
        return None;
    }

    // trace_id must be 32 lowercase hex characters and cannot be all zeros
    if !is_valid_trace_id(trace_id) {
        return None;
    }

    // parent_id must be 16 lowercase hex characters and cannot be all zeros
    if parent_id.len() != 16 || !is_lowercase_hex(parent_id) || parent_id.bytes().all(|b| b == b'0')
    {
        return None;
    }

    // trace_flags must be 2 lowercase hex characters
    if trace_flags.len() != 2 || !is_lowercase_hex(trace_flags) {
        return None;
    }

    let flags_byte = u8::from_str_radix(trace_flags, 16).ok()?;
    let is_sampled = (flags_byte & 0x01) == 0x01;

    Some(TraceContext {
        version: version.to_string(),
        trace_id: trace_id.to_string(),
        parent_id: parent_id.to_string(),
        trace_flags: trace_flags.to_string(),
        is_sampled,
        tracestate: None,
    })
}

/// Validates whether a string is a valid 32-character lowercase hex W3C `trace_id` (not all zeros).
#[inline]
pub fn is_valid_trace_id(candidate: &str) -> bool {
    candidate.len() == 32 && is_lowercase_hex(candidate) && !candidate.bytes().all(|b| b == b'0')
}

/// Generates a valid 16-character lowercase hex W3C `span_id` (8 bytes, not all zeros).
#[allow(clippy::cast_possible_truncation)]
pub fn generate_span_id() -> String {
    let mut val = (Uuid::now_v7().as_u128() as u64) ^ (Uuid::new_v4().as_u128() as u64);
    if val == 0 {
        val = 0x1234_5678_90ab_cdef;
    }
    format!("{val:016x}")
}

#[inline]
fn is_lowercase_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Formats a W3C `traceparent` header string from individual components.
pub fn format_traceparent(trace_id: &str, span_id: &str, is_sampled: bool) -> String {
    let flags = if is_sampled { "01" } else { "00" };
    format!("00-{trace_id}-{span_id}-{flags}")
}

/// Extracts the raw `tracestate` header value if present and non-empty.
pub fn extract_tracestate(headers: &HeaderMap) -> Option<String> {
    headers
        .get(TRACESTATE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extracts W3C `trace_id`, optional `parent_span_id`, `is_sampled`, and `tracestate` from headers.
/// If headers do not contain a valid W3C traceparent, generates a new 32-hex `trace_id` with `is_sampled = true`.
pub fn extract_or_generate_trace_context(
    headers: &HeaderMap,
) -> (String, Option<String>, bool, Option<String>) {
    let tracestate = extract_tracestate(headers);

    if let Some(raw) = headers
        .get(TRACEPARENT_HEADER)
        .and_then(|v| v.to_str().ok())
        && let Some(ctx) = parse_traceparent(raw)
    {
        return (
            ctx.trace_id,
            Some(ctx.parent_id),
            ctx.is_sampled,
            tracestate,
        );
    }

    if let Some(raw) = headers.get(X_TRACE_ID_HEADER).and_then(|v| v.to_str().ok()) {
        let trimmed = raw.trim().to_ascii_lowercase();
        if is_valid_trace_id(&trimmed) {
            return (trimmed, None, true, tracestate);
        }
    }

    // Fallback: Generate a fresh 16-byte (32-hex) time-ordered trace ID
    let new_trace_id = Uuid::now_v7().simple().to_string();
    (new_trace_id, None, true, tracestate)
}

/// Extracts `trace_id` from W3C `traceparent` (or fallback `x-trace-id`), or generates a new 32-hex ID.
pub fn extract_or_generate_trace_id(headers: &HeaderMap) -> String {
    extract_or_generate_trace_context(headers).0
}

/// Extracts `request_id` from `X-Request-Id` header if present and non-empty, or generates a new `UUIDv7`.
pub fn extract_or_generate_request_id(headers: &HeaderMap) -> String {
    if let Some(raw) = headers
        .get(X_REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    Uuid::now_v7().to_string()
}
