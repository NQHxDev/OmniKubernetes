//! Kubernetes Tri-Probe Architecture (`/livez`, `/readyz`, `/startupz`, `/healthz`).
//!
//! Provides health indicators, concurrent probe orchestration with strict timeouts,
//! watchdog heartbeat validation, and graceful shutdown draining.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use metrics::{gauge, histogram};
use serde::{Deserialize, Serialize};

use crate::config::MetricsConfig;

/// Metric names for Kubernetes readiness probe monitoring.
pub const METRIC_K8S_READINESS_DEPENDENCY_STATUS: &str = "k8s_readiness_dependency_status";
pub const METRIC_K8S_READINESS_CHECK_DURATION_SECONDS: &str =
    "k8s_readiness_check_duration_seconds";

/// Cache control header value strictly prohibiting caching of health probe responses.
const CACHE_CONTROL_VALUE: &str = "no-store, no-cache, must-revalidate";

/// Mức độ trọng yếu của một phụ thuộc hạ tầng đối với khả năng phục vụ của Pod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCriticality {
    /// Phụ thuộc sống còn (ví dụ: Primary PostgreSQL, Core Redis). Nếu lỗi -> `/readyz` trả về `503`.
    Critical,
    /// Phụ thuộc thứ cấp (ví dụ: dịch vụ gợi ý phụ trợ). Nếu lỗi -> `/readyz` trả về `200` với trạng thái `"degraded"`.
    NonCritical,
}

impl DependencyCriticality {
    /// Trả về chuỗi định danh mức độ trọng yếu cho metric labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::NonCritical => "non_critical",
        }
    }
}

/// Trạng thái sức khỏe tổng thể hoặc của từng thành phần.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Starting,
    Draining,
}

/// Trait chuẩn cho mọi bộ kiểm tra kết nối hạ tầng (`omni-database`, `omni-cache`, `omni-messaging`).
#[async_trait]
pub trait HealthIndicator: std::fmt::Debug + Send + Sync + 'static {
    /// Tên định danh của thành phần hạ tầng (ví dụ: `"postgres"`, `"redis"`).
    fn name(&self) -> &'static str;

    /// Mức độ trọng yếu (mặc định là `Critical`).
    fn criticality(&self) -> DependencyCriticality {
        DependencyCriticality::Critical
    }

    /// Thực thi kiểm tra kết nối thực tế (ví dụ: `SELECT 1` hoặc `PING`).
    async fn check(&self) -> Result<(), String>;
}

/// Kết quả kiểm tra chi tiết của từng phụ thuộc hạ tầng trong `/readyz`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealthReport {
    pub status: HealthStatus,
    pub criticality: DependencyCriticality,
    pub latency_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Cấu trúc phản hồi JSON chuẩn cho các endpoints `/livez`, `/readyz`, `/startupz`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResponse {
    pub status: HealthStatus,
    pub service: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub checks: BTreeMap<String, ComponentHealthReport>,
}

/// Quản lý trạng thái vòng đời khởi động, hạ cánh an toàn (Drain) và nhịp tim Watchdog của Pod.
#[derive(Debug)]
pub struct ProbeState {
    startup_complete: AtomicBool,
    shutting_down: AtomicBool,
    last_watchdog_heartbeat_ms: AtomicU64,
    started_at: Instant,
    liveness_stall_threshold: Duration,
}

impl ProbeState {
    /// Khởi tạo `ProbeState`. Mặc định `startup_complete = true` cho các service không cần pha khởi tạo trễ.
    #[must_use]
    pub fn new(startup_complete: bool, liveness_stall_threshold: Duration) -> Self {
        Self {
            startup_complete: AtomicBool::new(startup_complete),
            shutting_down: AtomicBool::new(false),
            last_watchdog_heartbeat_ms: AtomicU64::new(current_epoch_ms()),
            started_at: Instant::now(),
            liveness_stall_threshold,
        }
    }

    /// Đánh dấu tiến trình đã hoàn tất pha khởi động (Migrations / Cache Warmup).
    pub fn mark_startup_complete(&self) {
        self.startup_complete.store(true, Ordering::Release);
    }

    /// Kích hoạt chế độ hạ cánh an toàn (khi nhận `SIGTERM`), buộc `/readyz` trả về `503` ngay lập tức.
    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
    }

    /// Cập nhật nhịp tim từ Tokio Watchdog background task.
    pub fn record_watchdog_heartbeat(&self) {
        self.last_watchdog_heartbeat_ms
            .store(current_epoch_ms(), Ordering::Release);
    }

    /// Kiểm tra nhịp tim Watchdog có còn nằm trong ngưỡng an toàn hay không.
    #[must_use]
    pub fn is_watchdog_healthy(&self) -> bool {
        let last = self.last_watchdog_heartbeat_ms.load(Ordering::Acquire);
        let now = current_epoch_ms();
        let elapsed_ms = now.saturating_sub(last);
        elapsed_ms <= u64::try_from(self.liveness_stall_threshold.as_millis()).unwrap_or(15_000)
    }

    /// Kiểm tra tiến trình đã hoàn tất khởi động hay chưa.
    #[must_use]
    pub fn is_startup_complete(&self) -> bool {
        self.startup_complete.load(Ordering::Acquire)
    }

    /// Kiểm tra tiến trình có đang trong trạng thái draining để tắt hay không.
    #[must_use]
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    /// Thời gian tiến trình đã sống (giây).
    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}

#[inline]
fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Bộ đăng ký và điều phối kiểm tra sức khỏe phụ thuộc hạ tầng song song.
#[derive(Clone, Default)]
pub struct HealthRegistry {
    indicators: Arc<RwLock<Vec<Arc<dyn HealthIndicator>>>>,
}

impl std::fmt::Debug for HealthRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.indicators.read().map_or(0, |g| g.len());
        f.debug_struct("HealthRegistry")
            .field("registered_indicators_count", &count)
            .finish()
    }
}

impl HealthRegistry {
    /// Khởi tạo registry mới rỗng.
    #[must_use]
    pub fn new() -> Self {
        Self {
            indicators: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Đăng ký một bộ kiểm tra sức khỏe hạ tầng.
    pub fn register<I: HealthIndicator>(&self, indicator: I) {
        self.register_arc(Arc::new(indicator));
    }

    /// Đăng ký một bộ kiểm tra sức khỏe đã được bọc `Arc`.
    pub fn register_arc(&self, indicator: Arc<dyn HealthIndicator>) {
        if let Ok(mut guard) = self.indicators.write() {
            guard.push(indicator);
        }
    }

    /// Thực thi kiểm tra song song toàn bộ các phụ thuộc với thời gian chờ giới hạn cho từng phép kiểm tra.
    pub async fn check_all(
        &self,
        per_check_timeout: Duration,
    ) -> BTreeMap<String, ComponentHealthReport> {
        let indicators: Vec<Arc<dyn HealthIndicator>> = self
            .indicators
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();

        if indicators.is_empty() {
            return BTreeMap::new();
        }

        let futures = indicators.into_iter().map(|indicator| {
            let timeout = per_check_timeout;
            async move {
                let name = indicator.name();
                let criticality = indicator.criticality();
                let start = Instant::now();

                let check_res = tokio::time::timeout(timeout, indicator.check()).await;
                let duration_secs = start.elapsed().as_secs_f64();
                let latency_ms = duration_secs * 1000.0;

                histogram!(METRIC_K8S_READINESS_CHECK_DURATION_SECONDS, "dependency" => name)
                    .record(duration_secs);

                let (status, message, status_val) = match check_res {
                    Ok(Ok(())) => (HealthStatus::Healthy, None, 1.0),
                    Ok(Err(err)) => (HealthStatus::Unhealthy, Some(err), 0.0),
                    Err(_) => (
                        HealthStatus::Unhealthy,
                        Some(format!(
                            "Health check timed out after {}ms",
                            timeout.as_millis()
                        )),
                        0.0,
                    ),
                };

                gauge!(
                    METRIC_K8S_READINESS_DEPENDENCY_STATUS,
                    "dependency" => name,
                    "criticality" => criticality.as_str()
                )
                .set(status_val);

                (
                    name.to_string(),
                    ComponentHealthReport {
                        status,
                        criticality,
                        latency_ms,
                        message,
                    },
                )
            }
        });

        let results = join_all(futures).await;
        results.into_iter().collect()
    }
}

/// Ngữ cảnh dùng chung cho các bộ điều khiển Kubernetes Probes.
#[derive(Clone, Debug)]
pub struct ProbeContext {
    pub state: Arc<ProbeState>,
    pub registry: HealthRegistry,
    pub config: MetricsConfig,
}

impl ProbeContext {
    /// Khởi tạo ngữ cảnh probe mới.
    #[must_use]
    pub fn new(state: Arc<ProbeState>, registry: HealthRegistry, config: MetricsConfig) -> Self {
        Self {
            state,
            registry,
            config,
        }
    }
}

/// Tạo phản hồi Probe kèm header `Cache-Control: no-store, no-cache, must-revalidate`.
fn build_probe_response(status_code: StatusCode, body: ProbeResponse) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE),
    );
    (status_code, headers, Json(body)).into_response()
}

/// Handler xử lý `GET /livez` (Liveness Probe).
///
/// Chỉ xác nhận khả năng lập lịch của Tokio Reactor và nhịp tim Watchdog, tuyệt đối không kiểm tra DB/Redis.
pub async fn livez_handler(State(ctx): State<ProbeContext>) -> Response {
    // 1. Buộc nhường CPU để kiểm tra Tokio Reactor không bị nghẽn
    tokio::task::yield_now().await;

    // 2. Xác thực nhịp tim Watchdog
    let is_healthy = ctx.state.is_watchdog_healthy();
    let status = if is_healthy {
        HealthStatus::Healthy
    } else {
        HealthStatus::Unhealthy
    };
    let status_code = if is_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let response = ProbeResponse {
        status,
        service: ctx.config.service_name.clone(),
        version: ctx.config.service_version.clone(),
        uptime_seconds: ctx.state.uptime_seconds(),
        timestamp: Utc::now(),
        checks: BTreeMap::new(),
    };

    build_probe_response(status_code, response)
}

/// Handler xử lý `GET /readyz` (Readiness Probe).
///
/// Kiểm tra thực tế kết nối tới toàn bộ các phụ thuộc hạ tầng đã đăng ký và cổng chặn Graceful Shutdown.
pub async fn readyz_handler(State(ctx): State<ProbeContext>) -> Response {
    // 1. Kiểm tra trạng thái Khởi động
    if !ctx.state.is_startup_complete() {
        let response = ProbeResponse {
            status: HealthStatus::Starting,
            service: ctx.config.service_name.clone(),
            version: ctx.config.service_version.clone(),
            uptime_seconds: ctx.state.uptime_seconds(),
            timestamp: Utc::now(),
            checks: BTreeMap::new(),
        };
        return build_probe_response(StatusCode::SERVICE_UNAVAILABLE, response);
    }

    // 2. Kiểm tra cổng chặn Graceful Shutdown (Drain Gate)
    if ctx.state.is_shutting_down() {
        let response = ProbeResponse {
            status: HealthStatus::Draining,
            service: ctx.config.service_name.clone(),
            version: ctx.config.service_version.clone(),
            uptime_seconds: ctx.state.uptime_seconds(),
            timestamp: Utc::now(),
            checks: BTreeMap::new(),
        };
        return build_probe_response(StatusCode::SERVICE_UNAVAILABLE, response);
    }

    // 3. Thực thi kiểm tra song song các phụ thuộc hạ tầng
    let checks = ctx
        .registry
        .check_all(ctx.config.readiness_probe_timeout)
        .await;

    let has_critical_failure = checks.values().any(|c| {
        c.criticality == DependencyCriticality::Critical && c.status != HealthStatus::Healthy
    });

    let has_non_critical_failure = checks.values().any(|c| {
        c.criticality == DependencyCriticality::NonCritical && c.status != HealthStatus::Healthy
    });

    let (status, status_code) = if has_critical_failure {
        (HealthStatus::Unhealthy, StatusCode::SERVICE_UNAVAILABLE)
    } else if has_non_critical_failure {
        (HealthStatus::Degraded, StatusCode::OK)
    } else {
        (HealthStatus::Healthy, StatusCode::OK)
    };

    let response = ProbeResponse {
        status,
        service: ctx.config.service_name.clone(),
        version: ctx.config.service_version.clone(),
        uptime_seconds: ctx.state.uptime_seconds(),
        timestamp: Utc::now(),
        checks,
    };

    build_probe_response(status_code, response)
}

/// Handler xử lý `GET /startupz` (Startup Probe).
///
/// Phục vụ kiểm tra tiến trình đã hoàn thành pha nạp ban đầu (Migrations, Cache Warming).
pub async fn startupz_handler(State(ctx): State<ProbeContext>) -> Response {
    let is_complete = ctx.state.is_startup_complete();
    let status = if is_complete {
        HealthStatus::Healthy
    } else {
        HealthStatus::Starting
    };
    let status_code = if is_complete {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let response = ProbeResponse {
        status,
        service: ctx.config.service_name.clone(),
        version: ctx.config.service_version.clone(),
        uptime_seconds: ctx.state.uptime_seconds(),
        timestamp: Utc::now(),
        checks: BTreeMap::new(),
    };

    build_probe_response(status_code, response)
}

/// Tạo Axum Router phục vụ toàn bộ các endpoints Kubernetes Probes chuẩn (`/livez`, `/readyz`, `/startupz`, `/healthz`, `/health`).
pub fn probe_router(ctx: ProbeContext) -> Router {
    Router::new()
        .route("/livez", get(livez_handler))
        .route("/readyz", get(readyz_handler))
        .route("/startupz", get(startupz_handler))
        .route("/healthz", get(livez_handler))
        .route("/health", get(livez_handler))
        .with_state(ctx)
}

impl omni_common::resilience::ShutdownDrainGate for ProbeState {
    fn begin_shutdown(&self) {
        Self::begin_shutdown(self);
    }
}
