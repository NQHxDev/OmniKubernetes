//! Tests for URI path sanitization and route template normalization.

use omni_metrics::sanitizer::{UNMATCHED_ROUTE_LABEL, normalize_matched_path, sanitize_uri_path};

#[test]
fn test_normalize_matched_path_syntax() {
    assert_eq!(
        normalize_matched_path("/api/v1/orders/{id}").as_ref(),
        "/api/v1/orders/:id"
    );
    assert_eq!(
        normalize_matched_path("/api/v1/orders/{order_id}/items/{item_id}").as_ref(),
        "/api/v1/orders/:order_id/items/:item_id"
    );
    assert_eq!(
        normalize_matched_path("/files/{*wildcard}").as_ref(),
        "/files/:wildcard"
    );
    assert_eq!(
        normalize_matched_path("/static/path").as_ref(),
        "/static/path"
    );
    assert_eq!(normalize_matched_path("/").as_ref(), "/");
}

#[test]
fn test_sanitize_uuid_variants() {
    // UUIDv4
    assert_eq!(
        sanitize_uri_path("/api/v1/orders/a1b2c3d4-e5f6-7a8b-9c0d-1e2f3a4b5c6d"),
        "/api/v1/orders/:id"
    );
    // UUIDv7
    assert_eq!(
        sanitize_uri_path("/api/v1/orders/0192d4b8-f2e1-7e83-a4c3-b8162e24cf81"),
        "/api/v1/orders/:id"
    );
    // Shorter hyphenated hex (Ruled Hex)
    assert_eq!(
        sanitize_uri_path("/api/v1/orders/018f4a2b-8a5e-7a1b"),
        "/api/v1/orders/:id"
    );
}

#[test]
fn test_sanitize_numeric_ids() {
    assert_eq!(
        sanitize_uri_path("/api/v1/users/12345/posts/67890"),
        "/api/v1/users/:id/posts/:id"
    );
    assert_eq!(sanitize_uri_path("/orders/1"), "/orders/:id");
    assert_eq!(sanitize_uri_path("/999999999999"), "/:id");
}

#[test]
fn test_sanitize_prefixed_domain_ids() {
    assert_eq!(
        sanitize_uri_path("/users/usr_01HZX87Q1K7/invoices/inv_987654"),
        "/users/:id/invoices/:id"
    );
    assert_eq!(
        sanitize_uri_path("/payments/pay_a1b2c3d4e5f6"),
        "/payments/:id"
    );
    assert_eq!(sanitize_uri_path("/orders/ord_123"), "/orders/:id");
}

#[test]
fn test_sanitize_ulid_and_long_hex() {
    // ULID (26 Crockford Base32 characters)
    assert_eq!(
        sanitize_uri_path("/items/01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        "/items/:id"
    );
    // 32-hex Trace ID / 24-hex Mongo ObjectID
    assert_eq!(
        sanitize_uri_path("/traces/4bf92f3577b34da6a3ce929d0e0e4736"),
        "/traces/:id"
    );
    assert_eq!(
        sanitize_uri_path("/documents/507f1f77bcf86cd799439011"),
        "/documents/:id"
    );
}

#[test]
fn test_preserve_api_version_and_static_segments() {
    assert_eq!(sanitize_uri_path("/api/v1/orders"), "/api/v1/orders");
    assert_eq!(sanitize_uri_path("/api/v2/products"), "/api/v2/products");
    assert_eq!(sanitize_uri_path("/v3/checkout"), "/v3/checkout");
    assert_eq!(sanitize_uri_path("/healthz"), "/healthz");
    assert_eq!(sanitize_uri_path("/metrics"), "/metrics");
}

#[test]
fn test_query_string_and_whitespace_handling() {
    assert_eq!(
        sanitize_uri_path("/api/v1/orders/12345?filter=active&sort=desc"),
        "/api/v1/orders/:id"
    );
    assert_eq!(sanitize_uri_path("   /api/v1/users   "), "/api/v1/users");
    assert_eq!(sanitize_uri_path(""), "/");
    assert_eq!(sanitize_uri_path("/"), "/");
    assert_eq!(sanitize_uri_path("///"), "/");
}

#[test]
fn test_max_segments_truncation() {
    let deep_path = "/a/b/c/d/e/f/g/h/i/j/k";
    assert_eq!(sanitize_uri_path(deep_path), "/a/b/c/d/e/f/g/h/:rest");
}

#[test]
fn test_tokens_and_long_hashes() {
    let token =
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.e30.t-IDcSemACt8x4iTMCda8Yhe3iZaWbvV5XKSTbuAn0M";
    assert_eq!(
        sanitize_uri_path(&format!("/verify/{token}")),
        "/verify/:id"
    );
}

#[test]
fn test_unmatched_route_label_constant() {
    assert_eq!(UNMATCHED_ROUTE_LABEL, "__unmatched__");
}
