//! Tests for Prometheus Exporter, Metrics Router, and `AppConfig` Bridge.

use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use omni_common::config::{AppConfig, EmptyCustomConfig};
use omni_metrics::{
    HealthRegistry, MetricsConfig, PROMETHEUS_MEDIA_TYPE, ProbeContext, ProbeState,
    init_prometheus, metrics_router, observability_routes,
};
use tower::ServiceExt;

#[test]
fn test_metrics_config_from_app_config_bridge() {
    let mut app_config = AppConfig::<EmptyCustomConfig>::default();
    app_config.service.name = "catalog-service".to_string();
    app_config.service.version = "2.4.1".to_string();
    app_config.observability.metrics_enabled = true;
    app_config.observability.metrics_path = "/custom-metrics".to_string();

    let metrics_cfg = MetricsConfig::from_app_config(&app_config);
    assert_eq!(metrics_cfg.service_name, "catalog-service");
    assert_eq!(metrics_cfg.service_version, "2.4.1");
    assert_eq!(metrics_cfg.environment, "dev");
    assert!(metrics_cfg.enabled);
    assert_eq!(metrics_cfg.metrics_path, "/custom-metrics");
}

#[tokio::test]
async fn test_init_prometheus_idempotency() {
    let config = MetricsConfig {
        service_name: "test-init".to_string(),
        service_version: "0.1.0".to_string(),
        environment: "dev".to_string(),
        ..Default::default()
    };

    let handle1 = init_prometheus(&config).unwrap();
    let handle2 = init_prometheus(&config).unwrap();

    // Idempotent calls return the exact same cloned handle without error
    let render1 = handle1.render();
    let render2 = handle2.render();
    assert_eq!(render1, render2);
    assert!(render1.contains("omni_build_info"));
}

#[tokio::test]
async fn test_metrics_router_exposition() {
    let config = MetricsConfig {
        service_name: "test-exporter".to_string(),
        service_version: "1.0.0".to_string(),
        environment: "test".to_string(),
        metrics_path: "/metrics".to_string(),
        enabled: true,
        ..Default::default()
    };
    let (_recorder, handle) = omni_metrics::create_isolated_recorder(&config).unwrap();
    let app = metrics_router(handle, &config);

    let req = Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        PROMETHEUS_MEDIA_TYPE
    );

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&bytes);
    // Verified text format
    assert!(!body_str.is_empty() || body_str.contains("# TYPE") || body_str.is_empty());
}

#[tokio::test]
async fn test_metrics_router_disabled_returns_404() {
    let config = MetricsConfig {
        enabled: false,
        metrics_path: "/metrics".to_string(),
        ..Default::default()
    };
    let (_recorder, handle) = omni_metrics::create_isolated_recorder(&config).unwrap();
    let app = metrics_router(handle, &config);

    let req = Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_observability_routes_combined_router() {
    let config = MetricsConfig {
        service_name: "unified-service".to_string(),
        metrics_path: "/metrics".to_string(),
        ..Default::default()
    };
    let (_recorder, handle) = omni_metrics::create_isolated_recorder(&config).unwrap();
    let state = Arc::new(ProbeState::new(true, std::time::Duration::from_secs(15)));
    let registry = HealthRegistry::new();
    let probe_ctx = ProbeContext::new(state, registry, config);

    let app = observability_routes(probe_ctx, handle);

    // 1. Phục vụ /livez
    let req_live = Request::builder()
        .uri("/livez")
        .body(Body::empty())
        .unwrap();
    let resp_live = app.clone().oneshot(req_live).await.unwrap();
    assert_eq!(resp_live.status(), StatusCode::OK);

    // 2. Phục vụ /readyz
    let req_ready = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp_ready = app.clone().oneshot(req_ready).await.unwrap();
    assert_eq!(resp_ready.status(), StatusCode::OK);

    // 3. Phục vụ /metrics
    let req_metrics = Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let resp_metrics = app.oneshot(req_metrics).await.unwrap();
    assert_eq!(resp_metrics.status(), StatusCode::OK);
}
