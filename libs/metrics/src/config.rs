//! Configuration structures and bridge to `omni-common` configuration.

use std::time::Duration;

use omni_common::config::{AppConfig, Validate};
use serde::{Deserialize, Serialize};

/// Cấu hình khởi tạo hệ thống Prometheus Metrics, Kubernetes Probes và System Telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Tên dịch vụ gắn vào nhãn toàn cục `service`.
    pub service_name: String,
    /// Phiên bản dịch vụ gắn vào metric `omni_build_info`.
    pub service_version: String,
    /// Môi trường vận hành (`dev`, `staging`, `prod`) gắn vào nhãn toàn cục `environment`.
    pub environment: String,
    /// Bật/tắt phục vụ endpoint `/metrics`.
    pub enabled: bool,
    /// Đường dẫn HTTP phục vụ Prometheus scraping (mặc định: `"/metrics"`).
    pub metrics_path: String,
    /// Chu kỳ chạy tác vụ nền `run_upkeep()` và thu thập chỉ số hệ thống (mặc định: 5 giây).
    pub upkeep_interval: Duration,
    /// Thời gian chờ tối đa cho mỗi `HealthIndicator` khi kiểm tra `/readyz` (mặc định: 1500ms).
    pub readiness_probe_timeout: Duration,
    /// Ngưỡng thời gian tối đa cho phép Tokio Watchdog không phản hồi trước khi `/livez` báo lỗi (mặc định: 15 giây).
    pub liveness_stall_threshold: Duration,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            service_name: "omni-service".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: "dev".to_string(),
            enabled: true,
            metrics_path: "/metrics".to_string(),
            upkeep_interval: Duration::from_secs(5),
            readiness_probe_timeout: Duration::from_millis(1500),
            liveness_stall_threshold: Duration::from_secs(15),
        }
    }
}

impl MetricsConfig {
    /// Khởi tạo `MetricsConfig` trực tiếp từ `AppConfig<C>` của `omni-common`.
    #[must_use]
    pub fn from_app_config<C: Validate>(app_config: &AppConfig<C>) -> Self {
        Self {
            service_name: app_config.service.name.clone(),
            service_version: app_config.service.version.clone(),
            environment: app_config.service.environment.as_str().to_string(),
            enabled: app_config.observability.metrics_enabled,
            metrics_path: app_config.observability.metrics_path.clone(),
            ..Self::default()
        }
    }
}
