//! Tests for RED Metrics Middleware, Low-Cardinality Guardrails, and Error Code Attribution.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use omni_common::{I18nManager, ResponseMeta};
use omni_errors::{ApiErrorResponse, AppError, InvalidParamError, VAL_NUMBER_MIN_VALUE};
use omni_metrics::{MetricsConfig, create_isolated_recorder, red_metrics_middleware};
use tower::ServiceExt;

async fn ok_handler() -> &'static str {
    "OK"
}

async fn created_handler() -> (StatusCode, &'static str) {
    (StatusCode::CREATED, "CREATED")
}

async fn validation_error_handler() -> impl IntoResponse {
    let i18n = I18nManager::default();
    let meta = ResponseMeta::default();
    let param = InvalidParamError::localized(
        "quantity",
        VAL_NUMBER_MIN_VALUE,
        VAL_NUMBER_MIN_VALUE,
        None,
        &i18n,
        "en-US",
    );
    ApiErrorResponse::validation_failed(vec![param], "/api/v1/fail-validation", meta, &i18n)
}

async fn internal_error_handler() -> Result<&'static str, AppError> {
    Err(AppError::Internal(
        "Database connection pool exhausted".to_string(),
    ))
}

fn build_test_app() -> Router {
    Router::new()
        .route("/api/v1/orders/{id}", get(ok_handler))
        .route("/api/v1/orders", post(created_handler))
        .route("/api/v1/fail-validation", post(validation_error_handler))
        .route("/api/v1/internal-error", get(internal_error_handler))
        .route("/livez", get(ok_handler))
        .route("/metrics", get(ok_handler))
        .layer(middleware::from_fn(red_metrics_middleware))
}

#[test]
fn test_red_metrics_success_flow_and_histogram_buckets() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let config = MetricsConfig {
        service_name: "order-service".to_string(),
        service_version: "1.0.0".to_string(),
        environment: "test".to_string(),
        ..Default::default()
    };
    let (recorder, handle) = create_isolated_recorder(&config).unwrap();

    metrics::with_local_recorder(&recorder, || {
        rt.block_on(async {
            let app = build_test_app();

            // 1. GET /api/v1/orders/0192d4b8-f2e1-7e83-a4c3-b8162e24cf81
            let req1 = Request::builder()
                .method("GET")
                .uri("/api/v1/orders/0192d4b8-f2e1-7e83-a4c3-b8162e24cf81")
                .body(Body::empty())
                .unwrap();
            let resp1 = app.clone().oneshot(req1).await.unwrap();
            assert_eq!(resp1.status(), StatusCode::OK);

            // 2. POST /api/v1/orders
            let req2 = Request::builder()
                .method("POST")
                .uri("/api/v1/orders")
                .body(Body::empty())
                .unwrap();
            let resp2 = app.clone().oneshot(req2).await.unwrap();
            assert_eq!(resp2.status(), StatusCode::CREATED);
        });
    });

    let rendered = handle.render();

    // Rate: http_requests_total với route đã chuẩn hóa :id
    assert!(
        rendered.contains("http_requests_total"),
        "Prometheus render must contain http_requests_total"
    );
    assert!(rendered.contains("route=\"/api/v1/orders/:id\""));
    assert!(rendered.contains("status_code=\"200\""));
    assert!(rendered.contains("method=\"GET\""));

    assert!(rendered.contains("route=\"/api/v1/orders\""));
    assert!(rendered.contains("status_code=\"201\""));
    assert!(rendered.contains("method=\"POST\""));

    // Global labels
    assert!(rendered.contains("service=\"order-service\""));
    assert!(rendered.contains("environment=\"test\""));

    // Duration: histogram buckets
    assert!(rendered.contains("http_request_duration_seconds_bucket"));
    assert!(rendered.contains("le=\"0.005\""));
    assert!(rendered.contains("le=\"10\"") || rendered.contains("le=\"10.0\""));
}

#[test]
fn test_red_metrics_zero_copy_error_code_attribution() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let config = MetricsConfig {
        service_name: "order-service".to_string(),
        service_version: "1.0.0".to_string(),
        environment: "test".to_string(),
        ..Default::default()
    };
    let (recorder, handle) = create_isolated_recorder(&config).unwrap();

    metrics::with_local_recorder(&recorder, || {
        rt.block_on(async {
            let app = build_test_app();

            // 1. Validation failure: returns 422 Unprocessable Entity
            let req_val = Request::builder()
                .method("POST")
                .uri("/api/v1/fail-validation")
                .body(Body::empty())
                .unwrap();
            let resp_val = app.clone().oneshot(req_val).await.unwrap();
            assert_eq!(resp_val.status(), StatusCode::UNPROCESSABLE_ENTITY);

            // 2. Internal server error: returns 500 Internal Server Error
            let req_err = Request::builder()
                .method("GET")
                .uri("/api/v1/internal-error")
                .body(Body::empty())
                .unwrap();
            let resp_err = app.clone().oneshot(req_err).await.unwrap();
            assert_eq!(resp_err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });
    });

    let rendered = handle.render();

    // Kiểm tra phân loại lỗi 4xx và trích xuất ErrorCodeExtension("ERR_VALIDATION_FAILED")
    assert!(rendered.contains("http_request_errors_total"));
    assert!(rendered.contains("error_code=\"ERR_VALIDATION_FAILED\""));
    assert!(rendered.contains("error_class=\"4xx\""));
    assert!(rendered.contains("status_code=\"422\""));

    // Kiểm tra phân loại lỗi 5xx và trích xuất ErrorCodeExtension("ERR_INTERNAL_SERVER")
    assert!(rendered.contains("error_code=\"ERR_INTERNAL_SERVER\""));
    assert!(rendered.contains("error_class=\"5xx\""));
    assert!(rendered.contains("status_code=\"500\""));
}

#[test]
fn test_red_metrics_unmatched_404_cardinality_protection() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let config = MetricsConfig {
        service_name: "order-service".to_string(),
        service_version: "1.0.0".to_string(),
        environment: "test".to_string(),
        ..Default::default()
    };
    let (recorder, handle) = create_isolated_recorder(&config).unwrap();

    metrics::with_local_recorder(&recorder, || {
        rt.block_on(async {
            let app = build_test_app();

            // Gửi 2 request quét lỗ hổng ngẫu nhiên khác nhau không khớp bất kỳ route nào
            let req1 = Request::builder()
                .method("GET")
                .uri("/vulnerability-scan/random-payload-1")
                .body(Body::empty())
                .unwrap();
            let resp1 = app.clone().oneshot(req1).await.unwrap();
            assert_eq!(resp1.status(), StatusCode::NOT_FOUND);

            let req2 = Request::builder()
                .method("GET")
                .uri("/wp-admin/setup-config.php?token=998877")
                .body(Body::empty())
                .unwrap();
            let resp2 = app.clone().oneshot(req2).await.unwrap();
            assert_eq!(resp2.status(), StatusCode::NOT_FOUND);
        });
    });

    let rendered = handle.render();

    // Toàn bộ request 404 không khớp route PHẢI được gom về nhãn "__unmatched__"
    assert!(rendered.contains("route=\"__unmatched__\""));
    assert!(rendered.contains("status_code=\"404\""));
    assert!(rendered.contains("error_code=\"ERR_NOT_FOUND\""));
    assert!(rendered.contains("error_class=\"4xx\""));

    // Đảm bảo không tạo metric http_requests_total cho các đường dẫn quét lỗ hổng
    assert!(!rendered.contains(
        "http_requests_total{environment=\"test\",method=\"GET\",route=\"/vulnerability-scan"
    ));
    assert!(
        !rendered
            .contains("http_requests_total{environment=\"test\",method=\"GET\",route=\"/wp-admin")
    );
    assert!(!rendered.contains("http_request_errors_total{environment=\"test\",error_class=\"4xx\",error_code=\"ERR_NOT_FOUND\",method=\"GET\",route=\"/vulnerability-scan"));
}

#[test]
fn test_red_metrics_infrastructure_probe_noise_filtering() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let config = MetricsConfig {
        service_name: "order-service".to_string(),
        service_version: "1.0.0".to_string(),
        environment: "test".to_string(),
        ..Default::default()
    };
    let (recorder, handle) = create_isolated_recorder(&config).unwrap();

    metrics::with_local_recorder(&recorder, || {
        rt.block_on(async {
            let app = build_test_app();

            // Gọi /livez và /metrics
            let req_live = Request::builder()
                .uri("/livez")
                .body(Body::empty())
                .unwrap();
            let _ = app.clone().oneshot(req_live).await.unwrap();

            let req_prom = Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap();
            let _ = app.clone().oneshot(req_prom).await.unwrap();
        });
    });

    let rendered = handle.render();

    // Tuyệt đối không ghi nhận RED metrics cho /livez và /metrics
    assert!(!rendered.contains("route=\"/livez\""));
    assert!(!rendered.contains("route=\"/metrics\""));
}

#[test]
fn test_in_flight_guard_drop_behavior() {
    let config = MetricsConfig::default();
    let (recorder, handle) = create_isolated_recorder(&config).unwrap();

    metrics::with_local_recorder(&recorder, || {
        {
            let _guard =
                omni_metrics::InFlightGuard::new("POST".to_string(), "/api/v1/orders".to_string());
            let rendered = handle.render();
            assert!(rendered.contains("http_requests_in_flight"));
            assert!(
                rendered.contains(" 1\n") || rendered.contains(" 1.0\n") || rendered.contains(" 1")
            );
        }
        // Sau khi guard bị drop, in_flight phải trở về 0
        let rendered_after = handle.render();
        assert!(rendered_after.contains("http_requests_in_flight"));
        assert!(
            rendered_after.contains(" 0\n")
                || rendered_after.contains(" 0.0\n")
                || rendered_after.contains(" 0")
        );
    });
}
