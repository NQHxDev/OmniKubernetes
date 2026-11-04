//! W3C `Server-Timing` and `X-Response-Time` Latency Measurement Middleware.
//!
//! Measures end-to-end request processing duration at the egress layer with $O(1)$ header insertion
//! and zero JSON payload mutation (preserving Zero-Cost Abstraction).

use std::time::Instant;

use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request, Response},
    middleware::Next,
};

/// Standard W3C Server-Timing header name (`server-timing`).
pub const SERVER_TIMING_HEADER: &str = "server-timing";

/// Legacy response time header name (`x-response-time`).
pub const X_RESPONSE_TIME_HEADER: &str = "x-response-time";

/// Egress middleware that measures total request execution time and injects:
/// - `Server-Timing: total;dur=<ms>` (W3C Server-Timing Recommendation)
/// - `X-Response-Time: <ms>ms` (Traditional APM compatibility)
pub async fn timing_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let start = Instant::now();
    let mut response = next.run(req).await;
    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    // 1. W3C Server-Timing: total;dur=12.40
    let server_timing = format!("total;dur={duration_ms:.2}");
    if let Ok(val) = HeaderValue::from_str(&server_timing) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(SERVER_TIMING_HEADER), val);
    }

    // 2. Traditional X-Response-Time: 12.40ms
    let x_response_time = format!("{duration_ms:.2}ms");
    if let Ok(val) = HeaderValue::from_str(&x_response_time) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(X_RESPONSE_TIME_HEADER), val);
    }

    response
}
