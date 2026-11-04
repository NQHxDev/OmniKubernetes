//! Observability, Timing, and Request Context Middleware for Axum Microservices.
//!
//! Includes:
//! - [`observability`]: Unified Ingress/Egress observability middleware, tracing spans, and noise filtering.
//! - [`context`]: `RequestContext` extraction, infallible fallback, and legacy middleware.
//! - [`timing`]: W3C `Server-Timing` and `X-Response-Time` zero-copy header measurement.
//! - [`traceparent`]: W3C Trace Context Level 1 (`traceparent`, `tracestate`) and `UUIDv7` correlation.

pub mod context;
pub mod observability;
pub mod security;
pub mod timing;
pub mod traceparent;

pub use context::{RequestContext, extract_trusted_client_ip, request_context_middleware};
pub use observability::{
    ACCESS_LOG_TARGET, inject_egress_context, inject_egress_headers, inject_grpc_egress_context,
    inject_response_headers, is_health_probe_path, observability_middleware,
};
pub use security::security_headers_middleware;
pub use timing::{SERVER_TIMING_HEADER, X_RESPONSE_TIME_HEADER, timing_middleware};
pub use traceparent::{
    TRACEPARENT_HEADER, TRACESTATE_HEADER, TraceContext, X_TRACE_ID_HEADER,
    extract_or_generate_request_id, extract_or_generate_trace_context,
    extract_or_generate_trace_id, extract_tracestate, format_traceparent, generate_span_id,
    is_valid_trace_id, parse_traceparent,
};
