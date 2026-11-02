//! `OmniKubernetes` Enterprise Metrics Telemetry, RED Middleware & Kubernetes Probes Standard (`omni-metrics`).
//!
//! # Overview
//! `omni-metrics` hoàn thiện trụ cột quan sát thứ ba (The Observability Triad) cho hệ sinh thái `OmniKubernetes`, bao gồm:
//! - **Standardized RED Telemetry:** Thu thập bộ chỉ số Rate - Errors - Duration và In-Flight Requests cho Axum services.
//! - **Strict Low-Cardinality Guardrails:** Bộ chuẩn hóa đường dẫn Zero-Regex Byte Scanner chống bùng nổ số chiều Prometheus.
//! - **Zero-Copy Error Code Attribution:** Tích hợp trực tiếp với `omni-errors` qua `ErrorCodeExtension`.
//! - **Kubernetes Tri-Probe Architecture:** Phân định rạch ròi `/livez`, `/readyz`, `/startupz`, `/healthz` kèm cổng Graceful Shutdown Drain.
//! - **Tokio Runtime & Process Telemetry:** Giám sát độ bão hòa Tokio Reactor, nhịp tim Watchdog và tài nguyên tiến trình OS (Zero-`unsafe`).

pub mod config;
pub mod error;
pub mod exporter;
pub mod middleware;
pub mod probes;
pub mod sanitizer;
pub mod system;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;

pub use config::MetricsConfig;
pub use error::MetricsError;
pub use exporter::{
    DB_DURATION_BUCKETS, HTTP_DURATION_BUCKETS, PROBE_DURATION_BUCKETS, PROMETHEUS_MEDIA_TYPE,
    create_isolated_recorder, init_prometheus, metrics_router,
};
pub use middleware::{
    InFlightGuard, METRIC_HTTP_REQUEST_DURATION_SECONDS, METRIC_HTTP_REQUEST_ERRORS_TOTAL,
    METRIC_HTTP_REQUESTS_IN_FLIGHT, METRIC_HTTP_REQUESTS_TOTAL, fallback_error_code_for_status,
    red_metrics_middleware,
};
pub use probes::{
    ComponentHealthReport, DependencyCriticality, HealthIndicator, HealthRegistry, HealthStatus,
    METRIC_K8S_READINESS_CHECK_DURATION_SECONDS, METRIC_K8S_READINESS_DEPENDENCY_STATUS,
    ProbeContext, ProbeResponse, ProbeState, livez_handler, probe_router, readyz_handler,
    startupz_handler,
};
pub use sanitizer::{UNMATCHED_ROUTE_LABEL, normalize_matched_path, sanitize_uri_path};
pub use system::{
    METRIC_OMNI_BUILD_INFO, METRIC_PROCESS_CPU_SECONDS_TOTAL, METRIC_PROCESS_MAX_FDS,
    METRIC_PROCESS_OPEN_FDS, METRIC_PROCESS_RESIDENT_MEMORY_BYTES, METRIC_PROCESS_THREADS,
    METRIC_PROCESS_UPTIME_SECONDS, METRIC_PROCESS_VIRTUAL_MEMORY_BYTES,
    METRIC_TOKIO_RUNTIME_ALIVE_TASKS, METRIC_TOKIO_RUNTIME_GLOBAL_QUEUE_DEPTH,
    METRIC_TOKIO_RUNTIME_WORKER_BUSY_DURATION_SECONDS,
    METRIC_TOKIO_RUNTIME_WORKER_PARK_COUNT_TOTAL, METRIC_TOKIO_RUNTIME_WORKERS_COUNT,
    METRIC_TOKIO_WATCHDOG_CHECK_LAG_SECONDS, collect_process_metrics, collect_tokio_metrics,
    spawn_system_metrics_collector,
};

/// Tiện ích kết hợp tạo toàn bộ router phục vụ Kubernetes Probes và Prometheus Scraping endpoint.
pub fn observability_routes(probe_ctx: ProbeContext, handle: PrometheusHandle) -> Router {
    let metrics_r = metrics_router(handle, &probe_ctx.config);
    probe_router(probe_ctx).merge(metrics_r)
}
