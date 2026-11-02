//! Prometheus Exporter initialization, isolated recorder creation, and exposition router.

use std::sync::OnceLock;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use metrics::gauge;
use metrics_exporter_prometheus::{
    Matcher, PrometheusBuilder, PrometheusHandle, PrometheusRecorder,
};

use crate::{
    config::MetricsConfig, error::MetricsError, middleware::METRIC_HTTP_REQUEST_DURATION_SECONDS,
    probes::METRIC_K8S_READINESS_CHECK_DURATION_SECONDS, system::METRIC_OMNI_BUILD_INFO,
};

/// Content-Type chuẩn theo quy chuẩn Prometheus / `OpenMetrics` Exposition Format 0.0.4.
pub const PROMETHEUS_MEDIA_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Dải bucket chuẩn (đơn vị: giây) cho độ trễ HTTP/gRPC Request (từ 5ms đến 10s).
pub const HTTP_DURATION_BUCKETS: [f64; 11] = [
    0.005, // 5ms   - In-memory / Redis cache hit
    0.010, // 10ms  - Truy vấn SQL đơn giản theo Primary Key
    0.025, // 25ms  - p50 chuẩn cho API nội bộ
    0.050, // 50ms  - Xử lý giao dịch có 2-3 câu lệnh SQL
    0.100, // 100ms - p95 mục tiêu cho API nghiệp vụ
    0.250, // 250ms - Ngưỡng cảnh báo truy vấn chậm
    0.500, // 500ms - p99 tối đa theo cam kết SLA
    1.000, // 1.0s  - Tác vụ nặng
    2.500, // 2.5s  - Suy giảm hiệu năng nghiêm trọng
    5.000, // 5.0s  - Chạm trần acquire_timeout_secs
    10.00, // 10.0s - Tiệm cận request timeout
];

/// Dải bucket tinh gọn cho các kiểm tra sức khỏe hạ tầng (/readyz ping DB/Redis: 1ms đến 2s).
pub const PROBE_DURATION_BUCKETS: [f64; 8] = [
    0.001, // 1ms
    0.005, // 5ms
    0.010, // 10ms
    0.025, // 25ms
    0.050, // 50ms
    0.100, // 100ms
    0.500, // 500ms
    2.000, // 2.0s
];

static GLOBAL_PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Khởi tạo và cài đặt Global Prometheus Recorder theo mô hình Idempotent Singleton.
///
/// Thiết lập chuẩn các dải Histogram Buckets, Global Labels (`service`, `environment`),
/// và phát hành metric thông tin phiên bản `omni_build_info`.
///
/// # Errors
/// Trả về [`MetricsError`] nếu việc cài đặt global recorder thất bại.
pub fn init_prometheus(config: &MetricsConfig) -> Result<PrometheusHandle, MetricsError> {
    if let Some(existing) = GLOBAL_PROMETHEUS_HANDLE.get() {
        return Ok(existing.clone());
    }

    let builder = create_configured_builder(config)?;
    let handle = builder
        .install_recorder()
        .map_err(|e| MetricsError::GlobalInstall(e.to_string()))?;

    // Thiết lập thông tin build ban đầu
    gauge!(
        METRIC_OMNI_BUILD_INFO,
        "version" => config.service_version.clone(),
        "rust_version" => option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("1.85+")
    )
    .set(1.0);

    let _ = GLOBAL_PROMETHEUS_HANDLE.set(handle.clone());
    Ok(handle)
}

/// Tạo một bộ `(PrometheusRecorder, PrometheusHandle)` cô lập phục vụ kiểm thử song song không xung đột.
///
/// Sử dụng kết hợp với `metrics::with_local_recorder` trong các integration/unit tests.
///
/// # Errors
/// Trả về [`MetricsError`] nếu cấu hình builder thất bại.
pub fn create_isolated_recorder(
    config: &MetricsConfig,
) -> Result<(PrometheusRecorder, PrometheusHandle), MetricsError> {
    let builder = create_configured_builder(config)?;
    let recorder = builder.build_recorder();
    let handle = recorder.handle();
    Ok((recorder, handle))
}

fn create_configured_builder(config: &MetricsConfig) -> Result<PrometheusBuilder, MetricsError> {
    let mut builder = PrometheusBuilder::new();

    // 1. Cấu hình Histograms buckets
    builder = builder
        .set_buckets_for_metric(
            Matcher::Full(METRIC_HTTP_REQUEST_DURATION_SECONDS.to_string()),
            &HTTP_DURATION_BUCKETS,
        )
        .map_err(|e| MetricsError::RecorderInit(e.to_string()))?
        .set_buckets_for_metric(
            Matcher::Full(METRIC_K8S_READINESS_CHECK_DURATION_SECONDS.to_string()),
            &PROBE_DURATION_BUCKETS,
        )
        .map_err(|e| MetricsError::RecorderInit(e.to_string()))?;

    // 2. Đính kèm Global Labels bắt buộc
    builder = builder
        .add_global_label("service", config.service_name.clone())
        .add_global_label("environment", config.environment.clone());

    Ok(builder)
}

#[derive(Clone)]
struct MetricsRouteState {
    handle: PrometheusHandle,
    enabled: bool,
}

async fn metrics_scrape_handler(State(state): State<MetricsRouteState>) -> Response {
    if !state.enabled {
        return StatusCode::NOT_FOUND.into_response();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, PROMETHEUS_MEDIA_TYPE)
        .body(Body::from(state.handle.render()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Tạo Axum Router phục vụ Prometheus scraping endpoint (ví dụ: `GET /metrics`).
pub fn metrics_router(handle: PrometheusHandle, config: &MetricsConfig) -> Router {
    let state = MetricsRouteState {
        handle,
        enabled: config.enabled,
    };

    let path = if config.metrics_path.starts_with('/') {
        config.metrics_path.clone()
    } else {
        format!("/{}", config.metrics_path)
    };

    Router::new()
        .route(&path, get(metrics_scrape_handler))
        .with_state(state)
}
