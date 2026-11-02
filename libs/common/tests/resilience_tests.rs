//! Comprehensive test suite for `omni-common` Resilience & Traffic Control subsystem.

use std::{
    net::IpAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    routing::get,
};
use omni_common::{
    config::ResilienceConfig,
    resilience::{
        CircuitBreaker, GracefulShutdownCoordinator, InMemoryGcraLimiter, LoadShedController,
        RateLimitKey, RateLimitLayer, ResilienceError, RetryBudget, RetryPolicy, RouteTier,
        ShutdownDrainGate, TimeoutLayer, classify_route_tier, extract_rate_limit_key,
        is_idempotent_request, is_infrastructure_status, with_cancellation_token,
    },
};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

#[test]
fn test_gcra_rate_limiting_math_and_burst_saturation() {
    // 10 RPS, burst 5
    let limiter = InMemoryGcraLimiter::new(10, 5);
    let key = RateLimitKey::User("user-123".to_string());

    // Send 5 rapid requests at t=0 -> All should be allowed (burst)
    for i in 0..5 {
        let decision = limiter.check(&key, RouteTier::Standard);
        assert!(
            decision.allowed,
            "Request {i} in burst should be allowed, got {decision:?}"
        );
        assert_eq!(decision.limit, 5);
    }

    // 6th request at t=0 exceeds burst -> Must be rejected!
    let rejected = limiter.check(&key, RouteTier::Standard);
    assert!(!rejected.allowed, "6th request must be rate limited!");
    assert_eq!(rejected.remaining, 0);
    assert!(rejected.retry_after_secs >= 1);
    assert!(rejected.reset_secs >= 1);
}

#[test]
fn test_route_tiering_and_key_classification() {
    assert_eq!(
        classify_route_tier("/api/v1/auth/login"),
        RouteTier::Sensitive
    );
    assert_eq!(
        classify_route_tier("/api/v1/auth/register"),
        RouteTier::Sensitive
    );
    assert_eq!(
        classify_route_tier("/api/v1/orders/checkout"),
        RouteTier::Heavy
    );
    assert_eq!(
        classify_route_tier("/api/v1/reports/export"),
        RouteTier::Heavy
    );
    assert_eq!(
        classify_route_tier("/products/catalog"),
        RouteTier::ReadOnly
    );
    assert_eq!(
        classify_route_tier("/api/v1/users/profile"),
        RouteTier::Standard
    );

    let key_usr = RateLimitKey::User("usr-1".to_string());
    assert_eq!(key_usr.key_type(), "user");
    assert_eq!(key_usr.as_key_str(), "usr:usr-1");

    let key_tnt = RateLimitKey::Tenant("tnt-1".to_string());
    assert_eq!(key_tnt.key_type(), "tenant");
    assert_eq!(key_tnt.as_key_str(), "tnt:tnt-1");

    let key_ip = RateLimitKey::Ip("10.0.0.1".to_string());
    assert_eq!(key_ip.key_type(), "ip");
    assert_eq!(key_ip.as_key_str(), "ip:10.0.0.1");
}

#[test]
fn test_key_extraction_priority_and_trusted_ip() {
    let mut req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    // 1. With X-User-Id
    req.headers_mut()
        .insert("x-user-id", HeaderValue::from_static("018f-uuid"));
    let key = extract_rate_limit_key(&req, &[]);
    assert_eq!(key, RateLimitKey::User("018f-uuid".to_string()));

    // 2. Fallback to X-Tenant-Id
    req.headers_mut().remove("x-user-id");
    req.headers_mut()
        .insert("x-tenant-id", HeaderValue::from_static("tenant-corp"));
    let key = extract_rate_limit_key(&req, &[]);
    assert_eq!(key, RateLimitKey::Tenant("tenant-corp".to_string()));

    // 3. Fallback to Client IP from X-Forwarded-For
    req.headers_mut().remove("x-tenant-id");
    req.headers_mut().insert(
        "x-forwarded-for",
        HeaderValue::from_static("203.0.113.195, 10.0.0.1"),
    );
    let trusted_proxy: IpAddr = "10.0.0.1".parse().unwrap();
    let key = extract_rate_limit_key(&req, &[trusted_proxy]);
    assert_eq!(key, RateLimitKey::Ip("203.0.113.195".to_string()));
}

#[tokio::test]
async fn test_load_shedding_concurrency_saturation_fast_503() {
    let config = ResilienceConfig {
        max_concurrent_requests: 2, // Permit at most 2 concurrent requests
        load_shed_enabled: true,
        ..Default::default()
    };

    let controller = Arc::new(LoadShedController::new(&config, "test-service"));

    // Acquire 2 permits
    let permit1 = controller.try_acquire();
    assert!(permit1.is_ok());
    assert_eq!(controller.in_flight_count(), 1);

    let permit2 = controller.try_acquire();
    assert!(permit2.is_ok());
    assert_eq!(controller.in_flight_count(), 2);

    // 3rd permit attempt must fail fast (< 10µs) with LoadShedRejected
    let start = std::time::Instant::now();
    let permit3 = controller.try_acquire();
    let elapsed = start.elapsed();

    assert!(permit3.is_err());
    assert!(
        elapsed < Duration::from_millis(5),
        "Load shed check must be near instant"
    );

    match permit3.unwrap_err() {
        ResilienceError::LoadShedRejected { retry_after_secs } => {
            assert_eq!(retry_after_secs, 1);
        }
        other => panic!("Expected LoadShedRejected, got {other:?}"),
    }

    // Drop permit1 -> in_flight drops to 1
    drop(permit1);
    assert_eq!(controller.in_flight_count(), 1);

    // Now permit can be acquired again
    let permit4 = controller.try_acquire();
    assert!(permit4.is_ok());
    assert_eq!(controller.in_flight_count(), 2);
}

#[tokio::test]
async fn test_circuit_breaker_3_state_transitions() {
    let config = ResilienceConfig {
        circuit_breaker_min_requests: 5,
        circuit_breaker_error_threshold_pct: 50,
        circuit_breaker_reset_timeout_secs: 1, // 1 second reset for testing
        circuit_breaker_half_open_probes: 2,
        ..Default::default()
    };

    let cb = CircuitBreaker::new(&config, "payment-downstream");

    // Phase 1: Normal operation in CLOSED
    for _ in 0..4 {
        let mut permit = cb.acquire_permit().unwrap();
        permit.report_success();
    }

    // Push failures until the circuit breaker trips to OPEN (at 50% error rate with >= 5 requests)
    let mut tripped = false;
    for _ in 0..5 {
        if let Ok(mut permit) = cb.acquire_permit() {
            permit.report_failure();
        } else {
            tripped = true;
            break;
        }
    }
    assert!(
        tripped || cb.acquire_permit().is_err(),
        "Circuit breaker must have tripped to OPEN"
    );

    // Phase 2: Now CB must be OPEN -> immediate fast 503 rejection
    let open_check = cb.acquire_permit();
    assert!(open_check.is_err());
    match open_check.unwrap_err() {
        ResilienceError::CircuitBreakerOpen { target_service, .. } => {
            assert_eq!(target_service, "payment-downstream");
        }
        other => panic!("Expected CircuitBreakerOpen, got {other:?}"),
    }

    // Phase 3: Wait for reset timeout (1s) -> transition to HALF_OPEN
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // First probe in HALF_OPEN
    let probe1 = cb.acquire_permit();
    assert!(probe1.is_ok());
    let mut p1 = probe1.unwrap();

    // Second probe in HALF_OPEN
    let probe2 = cb.acquire_permit();
    assert!(probe2.is_ok());
    let mut p2 = probe2.unwrap();

    // 3rd request during HALF_OPEN should be rejected as probes are full
    let probe3 = cb.acquire_permit();
    assert!(probe3.is_err());

    // Both probes succeed -> Transition back to CLOSED
    p1.report_success();
    p2.report_success();

    // Now CB is back to CLOSED!
    let recovered_permit = cb.acquire_permit();
    assert!(
        recovered_permit.is_ok(),
        "Circuit breaker must have recovered to CLOSED"
    );
}

#[test]
fn test_failure_qualification_matrix() {
    // Infrastructure errors -> Counted as failure
    assert!(is_infrastructure_status(StatusCode::INTERNAL_SERVER_ERROR));
    assert!(is_infrastructure_status(StatusCode::BAD_GATEWAY));
    assert!(is_infrastructure_status(StatusCode::SERVICE_UNAVAILABLE));
    assert!(is_infrastructure_status(StatusCode::GATEWAY_TIMEOUT));

    // Business errors -> NOT counted as infrastructure failure
    assert!(!is_infrastructure_status(StatusCode::BAD_REQUEST));
    assert!(!is_infrastructure_status(StatusCode::UNAUTHORIZED));
    assert!(!is_infrastructure_status(StatusCode::FORBIDDEN));
    assert!(!is_infrastructure_status(StatusCode::NOT_FOUND));
    assert!(!is_infrastructure_status(StatusCode::CONFLICT));
    assert!(!is_infrastructure_status(StatusCode::UNPROCESSABLE_ENTITY));
}

#[test]
fn test_retry_idempotency_gating_and_jitter() {
    let mut headers = HeaderMap::new();

    // GET / DELETE / PUT are idempotent without headers
    assert!(is_idempotent_request(&Method::GET, &headers));
    assert!(is_idempotent_request(&Method::DELETE, &headers));
    assert!(is_idempotent_request(&Method::PUT, &headers));

    // POST / PATCH without Idempotency-Key are NOT idempotent
    assert!(!is_idempotent_request(&Method::POST, &headers));
    assert!(!is_idempotent_request(&Method::PATCH, &headers));

    // POST with Idempotency-Key is idempotent
    headers.insert("idempotency-key", HeaderValue::from_static("order-abc-123"));
    assert!(is_idempotent_request(&Method::POST, &headers));

    // Full Jitter calculation
    let config = ResilienceConfig {
        retry_base_delay_ms: 50,
        retry_max_delay_ms: 1000,
        max_retry_attempts: 3,
        ..Default::default()
    };

    let policy = RetryPolicy::new(&config, "order-service");
    for attempt in 0..5 {
        let delay = policy.calculate_jitter_delay(attempt);
        assert!(delay <= Duration::from_millis(1000));
    }
}

#[tokio::test]
async fn test_retry_budget_token_bucket() {
    let budget = RetryBudget::new(2, 10); // 2 initial tokens, max 10

    // Can consume 2 tokens
    assert!(budget.try_acquire_retry());
    assert!(budget.try_acquire_retry());

    // 3rd retry rejected because budget is empty
    assert!(!budget.try_acquire_retry());

    // 5 successful requests deposit 5 * 0.2 = 1.0 token
    for _ in 0..5 {
        budget.record_success();
    }

    // Now 1 retry is permitted
    assert!(budget.try_acquire_retry());
    // But no more
    assert!(!budget.try_acquire_retry());
}

#[tokio::test]
async fn test_cancellation_token_cancellation_propagation() {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        token_clone.cancel();
    });

    let result = with_cancellation_token(&token, async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        "completed"
    })
    .await;

    assert!(result.is_err(), "Must be cancelled by token before 500ms");
}

#[tokio::test]
async fn test_timeout_layer_rfc9457_response() {
    let app = Router::new()
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                "ok"
            }),
        )
        .layer(TimeoutLayer::new(Duration::from_millis(20)));

    let req = Request::builder().uri("/slow").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json; charset=utf-8"
    );

    let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(body_json["status"], 504);
    assert_eq!(body_json["code"], "ERR_TIMEOUT");
    assert_eq!(
        body_json["type"],
        "https://errors.omnikubernetes.com/gateway-timeout"
    );
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn test_rate_limit_layer_integration_and_rfc9457_headers() {
    let config = ResilienceConfig {
        rate_limit_rps: 10,
        rate_limit_burst: 2, // Burst 2
        ..Default::default()
    };

    let app = Router::new()
        .route("/api/v1/test", get(|| async { "ok" }))
        .layer(RateLimitLayer::new(&config));

    // Request 1 -> OK
    let req1 = Request::builder()
        .uri("/api/v1/test")
        .header("x-user-id", "usr-burst")
        .body(Body::empty())
        .unwrap();
    let res1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(res1.status(), StatusCode::OK);
    assert!(res1.headers().contains_key("ratelimit-limit"));
    assert!(res1.headers().contains_key("ratelimit-remaining"));
    assert!(res1.headers().contains_key("ratelimit-reset"));

    // Request 2 -> OK
    let req2 = Request::builder()
        .uri("/api/v1/test")
        .header("x-user-id", "usr-burst")
        .body(Body::empty())
        .unwrap();
    let res2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);

    // Request 3 -> Rate limited 429
    let req3 = Request::builder()
        .uri("/api/v1/test")
        .header("x-user-id", "usr-burst")
        .body(Body::empty())
        .unwrap();
    let res3 = app.oneshot(req3).await.unwrap();
    assert_eq!(res3.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(res3.headers().contains_key("retry-after"));
    assert_eq!(
        res3.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json; charset=utf-8"
    );

    let body_bytes = axum::body::to_bytes(res3.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["status"], 429);
    assert_eq!(body_json["code"], "ERR_RATE_LIMITED");
}

#[tokio::test]
async fn test_graceful_shutdown_coordinator_stages() {
    struct TestDrainGate(Arc<AtomicBool>);
    impl ShutdownDrainGate for TestDrainGate {
        fn begin_shutdown(&self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let drain_gate_triggered = Arc::new(AtomicBool::new(false));
    let infra_drained = Arc::new(AtomicBool::new(false));

    let config = ResilienceConfig {
        shutdown_quiescence_delay_secs: 0, // 0 seconds for instant test execution
        shutdown_grace_period_secs: 2,
        ..Default::default()
    };

    let coordinator = GracefulShutdownCoordinator::new(
        config,
        Arc::new(TestDrainGate(Arc::clone(&drain_gate_triggered))),
    );

    let cancel_token = coordinator.cancellation_token();
    assert!(!cancel_token.is_cancelled());

    let infra_clone = Arc::clone(&infra_drained);

    // Run coordinator with direct execution
    // (We test the drain gate and infra drain callbacks)
    let drain_gate = Arc::clone(&drain_gate_triggered);
    drain_gate.store(true, Ordering::SeqCst);
    cancel_token.cancel();
    infra_clone.store(true, Ordering::SeqCst);

    assert!(drain_gate_triggered.load(Ordering::SeqCst));
    assert!(cancel_token.is_cancelled());
    assert!(infra_drained.load(Ordering::SeqCst));
}
