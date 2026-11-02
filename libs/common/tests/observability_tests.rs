//! Acceptance & Conformance Test Suite for Enterprise Observability Architecture.
//!
//! Validates:
//! 1. W3C Trace Context Level 1 Conformance & Sampling flags.
//! 2. RFC 9562 `UUIDv7` Request ID Generation and Inheritance.
//! 3. Axum `observability_middleware` & Infallible `RequestContext` / `ResponseMeta` Extractors.
//! 4. Distributed Egress Context Continuity (caller `span_id` as downstream `parent_id`).
//! 5. Kubernetes Noise Filter (`/healthz`, `/livez`, `/readyz`, `/metrics`).
//! 6. Trusted Client IP (`IpAddr` validation) and Anti-IP-Spoofing.

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    middleware::from_fn,
    routing::get,
};
use omni_common::{
    ApiResponse, RequestContext, ResponseMeta,
    middleware::{
        SERVER_TIMING_HEADER, TRACEPARENT_HEADER, TRACESTATE_HEADER, X_RESPONSE_TIME_HEADER,
        extract_or_generate_request_id, extract_trusted_client_ip, format_traceparent,
        generate_span_id, inject_egress_headers, is_health_probe_path, observability_middleware,
        parse_traceparent,
    },
    response::X_REQUEST_ID_HEADER,
};
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn test_w3c_traceparent_spec_conformance() {
    // 1. Valid Level 1 Traceparent with Sampled bit = 1
    let sampled_raw = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let ctx = parse_traceparent(sampled_raw).expect("Must parse valid sampled traceparent");
    assert_eq!(ctx.version, "00");
    assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(ctx.parent_id, "00f067aa0ba902b7");
    assert_eq!(ctx.trace_flags, "01");
    assert!(ctx.is_sampled);

    // 2. Valid Level 1 Traceparent with Sampled bit = 0
    let unsampled_raw = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00";
    let ctx_unsampled = parse_traceparent(unsampled_raw).expect("Must parse unsampled traceparent");
    assert!(!ctx_unsampled.is_sampled);

    // 3. Flags with higher reserved bits set: 03 (binary 00000011 -> bit 0 is 1)
    let flags_03 = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-03";
    let ctx_03 = parse_traceparent(flags_03).unwrap();
    assert!(ctx_03.is_sampled);

    // 4. Flags with bit 0 unset: 02 (binary 00000010 -> bit 0 is 0)
    let flags_02 = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-02";
    let ctx_02 = parse_traceparent(flags_02).unwrap();
    assert!(!ctx_02.is_sampled);

    // 5. Invalid: All-zero trace_id
    assert!(parse_traceparent("00-00000000000000000000000000000000-00f067aa0ba902b7-01").is_none());

    // 6. Invalid: All-zero parent_id
    assert!(parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01").is_none());

    // 7. Invalid: Version ff
    assert!(parse_traceparent("ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01").is_none());

    // 8. Invalid: Version 00 with extra trailing parts
    assert!(
        parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra")
            .is_none()
    );

    // 9. Invalid: Non-hex characters
    assert!(parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e473g-00f067aa0ba902b7-01").is_none());

    // 10. Format traceparent round-trip
    let formatted =
        format_traceparent("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7", true);
    assert_eq!(
        formatted,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
}

#[test]
fn test_uuidv7_request_id_and_span_id_generation() {
    let mut headers = HeaderMap::new();

    // 1. Without header: Generates valid UUIDv7
    let req_id1 = extract_or_generate_request_id(&headers);
    let parsed_uuid1 = Uuid::parse_str(&req_id1).expect("Generated request_id must be valid UUID");
    assert_eq!(parsed_uuid1.get_version_num(), 7);

    // 2. Monotonic time ordering: UUIDv7 generated slightly later is greater
    std::thread::sleep(std::time::Duration::from_millis(2));
    let req_id2 = extract_or_generate_request_id(&headers);
    let parsed_uuid2 = Uuid::parse_str(&req_id2).unwrap();
    assert!(parsed_uuid2 > parsed_uuid1);

    // 3. With existing X-Request-Id header: Preserved
    headers.insert(
        X_REQUEST_ID_HEADER,
        HeaderValue::from_static("custom-req-12345"),
    );
    assert_eq!(extract_or_generate_request_id(&headers), "custom-req-12345");

    // 4. Span ID generation: 16 hex characters, not all zeros
    let span_id = generate_span_id();
    assert_eq!(span_id.len(), 16);
    assert!(span_id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(!span_id.bytes().all(|b| b == b'0'));
}

#[test]
fn test_trusted_client_ip_and_anti_spoofing() {
    // 1. Envoy trusted header takes priority
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-envoy-external-address",
        HeaderValue::from_static("203.0.113.195"),
    );
    headers.insert("x-real-ip", HeaderValue::from_static("10.0.0.1"));
    headers.insert("x-forwarded-for", HeaderValue::from_static("192.168.1.1"));
    assert_eq!(
        extract_trusted_client_ip(&headers),
        Some("203.0.113.195".to_string())
    );

    // 2. X-Real-IP takes second priority
    let mut headers2 = HeaderMap::new();
    headers2.insert("x-real-ip", HeaderValue::from_static("198.51.100.42"));
    headers2.insert("x-forwarded-for", HeaderValue::from_static("10.1.1.1"));
    assert_eq!(
        extract_trusted_client_ip(&headers2),
        Some("198.51.100.42".to_string())
    );

    // 3. Fallback to leftmost X-Forwarded-For
    let mut headers3 = HeaderMap::new();
    headers3.insert(
        "x-forwarded-for",
        HeaderValue::from_static("192.0.2.1, 10.0.0.2, 172.16.0.1"),
    );
    assert_eq!(
        extract_trusted_client_ip(&headers3),
        Some("192.0.2.1".to_string())
    );

    // 4. Malformed/Spoofed string is safely rejected (returns None)
    let mut headers4 = HeaderMap::new();
    headers4.insert(
        "x-forwarded-for",
        HeaderValue::from_static("<script>alert(1)</script>, 10.0.0.1"),
    );
    assert_eq!(extract_trusted_client_ip(&headers4), None);
}

#[test]
fn test_noise_filtering_endpoint_matching() {
    assert!(is_health_probe_path("/healthz"));
    assert!(is_health_probe_path("/livez"));
    assert!(is_health_probe_path("/readyz"));
    assert!(is_health_probe_path("/startupz"));
    assert!(is_health_probe_path("/metrics"));
    assert!(is_health_probe_path("/health"));

    assert!(!is_health_probe_path("/api/v1/orders"));
    assert!(!is_health_probe_path("/users"));
}

#[tokio::test]
async fn test_observability_middleware_full_ingress_and_egress_flow() {
    let app = Router::new()
        .route(
            "/api/v1/items",
            get(|ctx: RequestContext| async move {
                // Assert context attributes inside handler
                assert_eq!(ctx.request_id, "custom-test-req-001");
                assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
                assert_eq!(ctx.parent_span_id.as_deref(), Some("00f067aa0ba902b7"));
                assert!(ctx.is_sampled);
                assert_eq!(ctx.tracestate.as_deref(), Some("congo=t61rcWkgMzE"));
                assert_eq!(ctx.client_ip.as_deref(), Some("203.0.113.50"));
                assert_eq!(ctx.locale, "vi-VN");

                ApiResponse::ok("payload", "Success", ctx.to_meta())
            }),
        )
        .route("/healthz", get(|| async { "OK" }))
        .layer(from_fn(observability_middleware));

    // 1. Business request with full incoming W3C Context
    let req = Request::builder()
        .uri("/api/v1/items")
        .header(X_REQUEST_ID_HEADER, "custom-test-req-001")
        .header(
            TRACEPARENT_HEADER,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .header(TRACESTATE_HEADER, "congo=t61rcWkgMzE")
        .header("x-envoy-external-address", "203.0.113.50")
        .header("x-locale", "vi-VN")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Egress headers check
    assert_eq!(
        resp.headers().get(X_REQUEST_ID_HEADER).unwrap(),
        "custom-test-req-001"
    );
    let traceparent_resp = resp
        .headers()
        .get(TRACEPARENT_HEADER)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(traceparent_resp.starts_with("00-4bf92f3577b34da6a3ce929d0e0e4736-"));
    assert!(traceparent_resp.ends_with("-01"));

    // tracestate header preserved
    assert_eq!(
        resp.headers().get(TRACESTATE_HEADER).unwrap(),
        "congo=t61rcWkgMzE"
    );

    // Timing headers check
    assert!(
        resp.headers()
            .get(SERVER_TIMING_HEADER)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("total;dur=")
    );
    assert!(
        resp.headers()
            .get(X_RESPONSE_TIME_HEADER)
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("ms")
    );
    assert_eq!(
        resp.headers().get(header::CONTENT_LANGUAGE).unwrap(),
        "vi-VN"
    );

    // 2. Healthz probe: Noise filter must skip observability egress headers
    let req_probe = Request::builder()
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();

    let resp_probe = app.oneshot(req_probe).await.unwrap();
    assert_eq!(resp_probe.status(), StatusCode::OK);
    // Must NOT have traceparent or Server-Timing
    assert!(resp_probe.headers().get(TRACEPARENT_HEADER).is_none());
    assert!(resp_probe.headers().get(SERVER_TIMING_HEADER).is_none());
}

#[tokio::test]
async fn test_infallible_extractor_self_healing_without_middleware() {
    // A route that uses RequestContext & ResponseMeta but forgets to attach observability_middleware
    let uninstrumented_router = Router::new().route(
        "/uninstrumented",
        get(|ctx: RequestContext, meta: ResponseMeta| async move {
            assert!(!ctx.request_id.is_empty());
            assert_eq!(ctx.trace_id.len(), 32);
            assert_eq!(ctx.instance, "/uninstrumented");
            assert_eq!(meta.request_id, ctx.request_id);
            (StatusCode::OK, "Self-healed without panic")
        }),
    );

    let req = Request::builder()
        .uri("/uninstrumented")
        .body(Body::empty())
        .unwrap();

    let resp = uninstrumented_router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_downstream_egress_propagation_utilities() {
    let ctx = RequestContext::full(
        "req-egress-99",
        "4bf92f3577b34da6a3ce929d0e0e4736",
        "00f067aa0ba902b7", // Caller's span_id
        Some("parent-span-1"),
        Some("rojo=123"),
        true,
        "ja-JP",
        "/outgoing",
        Some("10.0.0.1"),
        Some("UserAgent/1.0"),
    );

    let mut outgoing_headers = HeaderMap::new();
    inject_egress_headers(&mut outgoing_headers, &ctx);

    // Downstream receives:
    // 1. x-request-id
    assert_eq!(
        outgoing_headers.get(X_REQUEST_ID_HEADER).unwrap(),
        "req-egress-99"
    );

    // 2. traceparent: caller's span_id (00f067aa0ba902b7) becomes parent_id for downstream!
    let traceparent_out = outgoing_headers
        .get(TRACEPARENT_HEADER)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        traceparent_out,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );

    // 3. tracestate
    assert_eq!(outgoing_headers.get(TRACESTATE_HEADER).unwrap(), "rojo=123");

    // 4. x-locale
    assert_eq!(outgoing_headers.get("x-locale").unwrap(), "ja-JP");

    // 5. Test Reqwest HTTP builder injection
    let client = reqwest::Client::new();
    let req_builder = client.get("http://localhost:8080/downstream");
    let injected_builder = omni_common::middleware::inject_egress_context(req_builder, &ctx);
    let req = injected_builder.build().unwrap();
    assert_eq!(
        req.headers().get(X_REQUEST_ID_HEADER).unwrap(),
        "req-egress-99"
    );
    assert_eq!(
        req.headers().get(TRACEPARENT_HEADER).unwrap(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
    assert_eq!(req.headers().get(TRACESTATE_HEADER).unwrap(), "rojo=123");
    assert_eq!(req.headers().get("x-locale").unwrap(), "ja-JP");

    // 6. Test Tonic gRPC request metadata injection
    let mut grpc_req = tonic::Request::new("dummy-payload");
    omni_common::middleware::inject_grpc_egress_context(&mut grpc_req, &ctx);
    assert_eq!(
        grpc_req.metadata().get(X_REQUEST_ID_HEADER).unwrap(),
        "req-egress-99"
    );
    assert_eq!(
        grpc_req.metadata().get(TRACEPARENT_HEADER).unwrap(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
    assert_eq!(
        grpc_req.metadata().get(TRACESTATE_HEADER).unwrap(),
        "rojo=123"
    );
    assert_eq!(grpc_req.metadata().get("x-locale").unwrap(), "ja-JP");
}
