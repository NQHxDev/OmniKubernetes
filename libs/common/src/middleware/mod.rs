//! Observability, Timing, and Request Context Middleware for Axum Microservices.
//!
//! Includes:
//! - [`context`]: `RequestContext` extraction, `tracing::Span` recording, and egress header enforcement.
//! - [`timing`]: W3C `Server-Timing` and `X-Response-Time` zero-copy header measurement.
//! - [`traceparent`]: W3C Trace Context Level 1 (`traceparent`) and `X-Request-Id` `UUIDv7` extraction.

pub mod context;
pub mod timing;
pub mod traceparent;

pub use context::{RequestContext, request_context_middleware};
pub use timing::{SERVER_TIMING_HEADER, X_RESPONSE_TIME_HEADER, timing_middleware};
pub use traceparent::{
    TRACEPARENT_HEADER, TraceContext, X_TRACE_ID_HEADER, extract_or_generate_request_id,
    extract_or_generate_trace_id, is_valid_trace_id, parse_traceparent,
};
