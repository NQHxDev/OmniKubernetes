//! Integration tests for Kubernetes Probes (/livez, /readyz, /startupz, /healthz).

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use omni_metrics::{
    DependencyCriticality, HealthIndicator, HealthRegistry, HealthStatus, MetricsConfig,
    ProbeContext, ProbeResponse, ProbeState, probe_router,
};
use tower::ServiceExt;

#[derive(Debug)]
struct MockIndicator {
    name: &'static str,
    criticality: DependencyCriticality,
    healthy: bool,
    delay: Duration,
    err_msg: &'static str,
}

#[async_trait]
impl HealthIndicator for MockIndicator {
    fn name(&self) -> &'static str {
        self.name
    }

    fn criticality(&self) -> DependencyCriticality {
        self.criticality
    }

    async fn check(&self) -> Result<(), String> {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        if self.healthy {
            Ok(())
        } else {
            Err(self.err_msg.to_string())
        }
    }
}

fn setup_probe_app(
    startup_complete: bool,
    stall_threshold: Duration,
    readiness_timeout: Duration,
) -> (axum::Router, Arc<ProbeState>, HealthRegistry) {
    let state = Arc::new(ProbeState::new(startup_complete, stall_threshold));
    let registry = HealthRegistry::new();
    let config = MetricsConfig {
        service_name: "test-service".to_string(),
        service_version: "1.0.0".to_string(),
        environment: "test".to_string(),
        readiness_probe_timeout: readiness_timeout,
        ..Default::default()
    };
    let ctx = ProbeContext::new(state.clone(), registry.clone(), config);
    let app = probe_router(ctx);
    (app, state, registry)
}

#[tokio::test]
async fn test_livez_healthy_and_stalled() {
    let (app, state, _) =
        setup_probe_app(true, Duration::from_millis(50), Duration::from_millis(1500));

    // 1. Khỏe mạnh -> 200 OK
    let req = Request::builder()
        .uri("/livez")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, no-cache, must-revalidate"
    );

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let probe_resp: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(probe_resp.status, HealthStatus::Healthy);
    assert_eq!(probe_resp.service, "test-service");
    assert!(probe_resp.checks.is_empty());

    // 2. Chờ vượt ngưỡng stall_threshold mà không có heartbeat -> 503 Service Unavailable
    tokio::time::sleep(Duration::from_millis(80)).await;
    let req2 = Request::builder()
        .uri("/livez")
        .body(Body::empty())
        .unwrap();
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::SERVICE_UNAVAILABLE);

    let bytes2 = to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
    let probe_resp2: ProbeResponse = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(probe_resp2.status, HealthStatus::Unhealthy);

    // 3. Sau khi phục hồi nhịp tim -> trở lại 200 OK
    state.record_watchdog_heartbeat();
    let req3 = Request::builder()
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let resp3 = app.oneshot(req3).await.unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_startupz_and_readyz_lifecycle() {
    let (app, state, _) =
        setup_probe_app(false, Duration::from_secs(15), Duration::from_millis(1500));

    // 1. Khi đang khởi động (startup_complete = false) -> Cả /startupz và /readyz đều trả về 503 Starting
    let req_s = Request::builder()
        .uri("/startupz")
        .body(Body::empty())
        .unwrap();
    let resp_s = app.clone().oneshot(req_s).await.unwrap();
    assert_eq!(resp_s.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = to_bytes(resp_s.into_body(), usize::MAX).await.unwrap();
    let body: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.status, HealthStatus::Starting);

    let req_r = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp_r = app.clone().oneshot(req_r).await.unwrap();
    assert_eq!(resp_r.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes_r = to_bytes(resp_r.into_body(), usize::MAX).await.unwrap();
    let body_r: ProbeResponse = serde_json::from_slice(&bytes_r).unwrap();
    assert_eq!(body_r.status, HealthStatus::Starting);

    // 2. Hoàn tất khởi động
    state.mark_startup_complete();

    let startup_req2 = Request::builder()
        .uri("/startupz")
        .body(Body::empty())
        .unwrap();
    let startup_resp2 = app.clone().oneshot(startup_req2).await.unwrap();
    assert_eq!(startup_resp2.status(), StatusCode::OK);
    let bytes2 = to_bytes(startup_resp2.into_body(), usize::MAX)
        .await
        .unwrap();
    let body2: ProbeResponse = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(body2.status, HealthStatus::Healthy);

    let ready_req2 = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let ready_resp2 = app.oneshot(ready_req2).await.unwrap();
    assert_eq!(ready_resp2.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_readyz_all_healthy_dependencies() {
    let (app, _, registry) =
        setup_probe_app(true, Duration::from_secs(15), Duration::from_millis(1500));

    registry.register(MockIndicator {
        name: "postgres",
        criticality: DependencyCriticality::Critical,
        healthy: true,
        delay: Duration::from_millis(2),
        err_msg: "",
    });
    registry.register(MockIndicator {
        name: "redis",
        criticality: DependencyCriticality::Critical,
        healthy: true,
        delay: Duration::from_millis(1),
        err_msg: "",
    });

    let req = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.status, HealthStatus::Healthy);
    assert_eq!(body.checks.len(), 2);

    let pg_check = body.checks.get("postgres").unwrap();
    assert_eq!(pg_check.status, HealthStatus::Healthy);
    assert_eq!(pg_check.criticality, DependencyCriticality::Critical);
    assert!(pg_check.latency_ms > 0.0);
    assert!(pg_check.message.is_none());
}

#[tokio::test]
async fn test_readyz_non_critical_failure_results_in_degraded() {
    let (app, _, registry) =
        setup_probe_app(true, Duration::from_secs(15), Duration::from_millis(1500));

    // Critical dependency passes
    registry.register(MockIndicator {
        name: "postgres",
        criticality: DependencyCriticality::Critical,
        healthy: true,
        delay: Duration::ZERO,
        err_msg: "",
    });
    // Non-critical dependency fails
    registry.register(MockIndicator {
        name: "recommendation_cache",
        criticality: DependencyCriticality::NonCritical,
        healthy: false,
        delay: Duration::ZERO,
        err_msg: "Cache connection lost",
    });

    let req = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // Non-critical failure MUST still return HTTP 200 OK with "degraded" status
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.status, HealthStatus::Degraded);

    let cache_check = body.checks.get("recommendation_cache").unwrap();
    assert_eq!(cache_check.status, HealthStatus::Unhealthy);
    assert_eq!(cache_check.criticality, DependencyCriticality::NonCritical);
    assert_eq!(
        cache_check.message.as_deref(),
        Some("Cache connection lost")
    );
}

#[tokio::test]
async fn test_readyz_critical_failure_results_in_unhealthy() {
    let (app, _, registry) =
        setup_probe_app(true, Duration::from_secs(15), Duration::from_millis(1500));

    registry.register(MockIndicator {
        name: "postgres",
        criticality: DependencyCriticality::Critical,
        healthy: false,
        delay: Duration::ZERO,
        err_msg: "PostgreSQL pool timeout",
    });

    let req = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.status, HealthStatus::Unhealthy);
    assert_eq!(
        body.checks["postgres"].message.as_deref(),
        Some("PostgreSQL pool timeout")
    );
}

#[tokio::test]
async fn test_readyz_dependency_timeout() {
    // Set per-check timeout to 50ms
    let (app, _, registry) =
        setup_probe_app(true, Duration::from_secs(15), Duration::from_millis(50));

    // Slow indicator that sleeps for 200ms
    registry.register(MockIndicator {
        name: "slow_storage",
        criticality: DependencyCriticality::Critical,
        healthy: true,
        delay: Duration::from_millis(200),
        err_msg: "",
    });

    let req = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.status, HealthStatus::Unhealthy);
    assert!(
        body.checks["slow_storage"]
            .message
            .as_ref()
            .unwrap()
            .contains("Health check timed out after 50ms")
    );
}

#[tokio::test]
async fn test_readyz_graceful_shutdown_draining() {
    let (app, state, _) =
        setup_probe_app(true, Duration::from_secs(15), Duration::from_millis(1500));

    // Signal SIGTERM shutdown
    state.begin_shutdown();

    let req = Request::builder()
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: ProbeResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.status, HealthStatus::Draining);
}
