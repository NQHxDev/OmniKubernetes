//! Tests for `AppBuilder`, 9-layer middleware onion, fallback and panic handlers.

use std::sync::Arc;

use axum::{
    Json, Router,
    http::{HeaderValue, StatusCode, header},
    routing::get,
};
use omni_common::{
    bootstrap::{AppBuilder, handle_panic},
    resilience::{AtomicDrainGate, ShutdownDrainGate},
    testing::TestClient,
};
use serde_json::Value;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct MockAppState {
    service_id: String,
}

#[tokio::test]
async fn test_app_builder_success_flow_and_layers() {
    let state = MockAppState {
        service_id: "test-order-service".to_string(),
    };

    let drain_gate = Arc::new(AtomicDrainGate::new());

    let cors = CorsLayer::new()
        .allow_origin("https://omnikubernetes.com".parse::<HeaderValue>().unwrap())
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST]);

    let domain_routes = Router::new().route(
        "/info",
        get(
            |axum::extract::State(st): axum::extract::State<MockAppState>| async move {
                Json(serde_json::json!({ "service": st.service_id }))
            },
        ),
    );

    let app = AppBuilder::new("test-service")
        .with_state(state)
        .with_k8s_probes(drain_gate.clone())
        .with_cors(cors)
        .with_security_headers(true)
        .with_observability(true)
        .mount_routes("/api/v1", domain_routes)
        .build();

    let client = TestClient::new(app);

    // 1. Test Domain route & state extraction
    let res = client.get("/api/v1/info").await;
    assert_eq!(res.status(), StatusCode::OK);
    let json: Value = res.json().expect("Valid json");
    assert_eq!(json["service"], "test-order-service");

    // 2. Test Security Headers
    assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(res.header("x-frame-options"), Some("DENY"));
    assert_eq!(
        res.header("content-security-policy"),
        Some("default-src 'self'")
    );
    assert!(res.header("strict-transport-security").is_some());

    // 3. Test Observability & Timing Headers
    assert!(res.header("x-request-id").is_some());
    assert!(res.header("server-timing").is_some() || res.header("x-response-time").is_some());

    // 4. Test K8s Probes (/livez, /readyz)
    let livez_res = client.get("/livez").await;
    assert_eq!(livez_res.status(), StatusCode::OK);
    let livez_json: Value = livez_res.json().expect("Valid livez json");
    assert_eq!(livez_json["status"], "Healthy");

    let readyz_res = client.get("/readyz").await;
    assert_eq!(readyz_res.status(), StatusCode::OK);

    // 5. Trigger Drain Gate and verify /readyz switches to 503 Draining
    drain_gate.begin_shutdown();
    let draining_res = client.get("/readyz").await;
    assert_eq!(draining_res.status(), StatusCode::SERVICE_UNAVAILABLE);
    let draining_json: Value = draining_res.json().expect("Valid draining json");
    assert_eq!(draining_json["status"], "Draining");
}

#[tokio::test]
async fn test_app_builder_rfc9457_fallback_404() {
    let state = MockAppState {
        service_id: "test".to_string(),
    };

    let app = AppBuilder::new("test-service")
        .with_state(state)
        .mount_routes(
            "/api",
            Router::new().route("/ping", get(|| async { "pong" })),
        )
        .build();

    let client = TestClient::new(app);

    let res = client.get("/non-existent-endpoint").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        res.header("content-type"),
        Some("application/problem+json; charset=utf-8")
    );

    let json: Value = res.json().expect("Valid problem details json");
    assert_eq!(json["type"], "https://errors.omnikubernetes.com/not-found");
    assert_eq!(json["title"], "Not Found");
    assert_eq!(json["status"], 404);
    assert_eq!(json["code"], "ERR_NOT_FOUND");
    assert!(
        json["detail"]
            .as_str()
            .unwrap()
            .contains("/non-existent-endpoint")
    );
}

#[tokio::test]
async fn test_catch_panic_produces_rfc9457_500() {
    let panic_res = handle_panic(Box::new("Simulated database connection failure"));
    assert_eq!(panic_res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        panic_res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json; charset=utf-8"
    );

    let body_bytes = axum::body::to_bytes(panic_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(
        json["type"],
        "https://errors.omnikubernetes.com/internal-server-error"
    );
    assert_eq!(json["status"], 500);
    assert_eq!(json["code"], "ERR_INTERNAL_SERVER");
    assert_eq!(json["title"], "Internal Server Error");
}
