//! OWASP Standard Security Headers Middleware.
//!
//! Injects standard security headers to mitigate common web vulnerabilities:
//! - `X-Content-Type-Options: nosniff` (MIME confusion prevention)
//! - `X-Frame-Options: DENY` (Clickjacking defense)
//! - `Strict-Transport-Security: max-age=31536000; includeSubDomains` (Force HTTPS)
//! - `Content-Security-Policy: default-src 'self'` (XSS / Injection mitigation)
//! - `Referrer-Policy: strict-origin-when-cross-origin` (Information leakage mitigation)
//! - `X-XSS-Protection: 0` (Disable buggy legacy browser filters in favor of CSP)

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};

static HEADER_XSS_PROTECTION: HeaderName = HeaderName::from_static("x-xss-protection");

/// Middleware to inject standard OWASP security headers.
pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers
        .entry(header::X_CONTENT_TYPE_OPTIONS)
        .or_insert(HeaderValue::from_static("nosniff"));

    headers
        .entry(header::X_FRAME_OPTIONS)
        .or_insert(HeaderValue::from_static("DENY"));

    headers
        .entry(header::STRICT_TRANSPORT_SECURITY)
        .or_insert(HeaderValue::from_static(
            "max-age=31536000; includeSubDomains",
        ));

    headers
        .entry(header::CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static("default-src 'self'"));

    headers
        .entry(header::REFERRER_POLICY)
        .or_insert(HeaderValue::from_static("strict-origin-when-cross-origin"));

    headers
        .entry(HEADER_XSS_PROTECTION.clone())
        .or_insert(HeaderValue::from_static("0"));

    response
}
