//! End-to-end integration test for AppBuilder and Server Bootstrap.

use std::sync::Arc;

use axum::{Json, Router, http::StatusCode, routing::get};
use omni_common::{
    bootstrap::AppBuilder,
    resilience::{AtomicDrainGate, ShutdownDrainGate},
    testing::TestClient,
};
use serde_json::Value;

#[tokio::test]
async fn test_full_bootstrap_lifecycle() {
    let drain_gate = Arc::new(AtomicDrainGate::new());

    let app = AppBuilder::new("integration-test-service")
        .with_state(())
        .with_k8s_probes(drain_gate.clone())
        .with_security_headers(true)
        .with_observability(true)
        .mount_routes(
            "/api/v1",
            Router::new().route(
                "/hello",
                get(|| async { Json(serde_json::json!({"msg": "world"})) }),
            ),
        )
        .build();

    let client = TestClient::new(app);

    // Verify domain endpoint
    let res = client.get("/api/v1/hello").await;
    assert_eq!(res.status(), StatusCode::OK);
    let json: Value = res.json().unwrap();
    assert_eq!(json["msg"], "world");

    // Verify security headers
    assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(res.header("x-frame-options"), Some("DENY"));

    // Verify probes
    let livez = client.get("/livez").await;
    assert_eq!(livez.status(), StatusCode::OK);

    let readyz = client.get("/readyz").await;
    assert_eq!(readyz.status(), StatusCode::OK);

    // Verify drain gate
    drain_gate.begin_shutdown();
    let readyz_draining = client.get("/readyz").await;
    assert_eq!(readyz_draining.status(), StatusCode::SERVICE_UNAVAILABLE);
}
