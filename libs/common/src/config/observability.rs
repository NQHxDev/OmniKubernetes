//! Observability, metrics, `OpenAPI`, and logging configuration.

use omni_logging::LogFormat;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Cấu hình quan sát bổ sung (Prometheus metrics & `OpenAPI` specification).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// Kích hoạt Prometheus metrics endpoint.
    pub metrics_enabled: bool,

    /// Đường dẫn phục vụ Prometheus metrics scraping.
    #[validate(length(min = 1))]
    pub metrics_path: String,

    /// Kích hoạt `OpenAPI` JSON schema endpoint.
    pub openapi_enabled: bool,

    /// Đường dẫn tải `OpenAPI` JSON specification.
    #[validate(length(min = 1))]
    pub openapi_json_path: String,

    /// Kích hoạt Swagger UI HTML (Nghiêm cấm bật ở môi trường Prod).
    pub swagger_ui_enabled: bool,

    /// Tỷ lệ lấy mẫu APM Traces (0.0 đến 1.0).
    #[validate(range(min = 0.0, max = 1.0))]
    pub sampling_ratio: f64,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: true,
            metrics_path: "/metrics".to_string(),
            openapi_enabled: true,
            openapi_json_path: "/v3/api-docs/openapi.json".to_string(),
            swagger_ui_enabled: false,
            sampling_ratio: 1.0,
        }
    }
}

/// Cấu hình phân hệ Structured Logging & OpenTelemetry OTLP (`omni-logging`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct LoggingSectionConfig {
    /// Cấp độ lọc log mặc định (ví dụ: `"info"`, `"debug,hyper=warn"`).
    #[validate(length(min = 1, message = "logging.log_level must not be empty"))]
    pub log_level: String,

    /// Định dạng xuất log (`json` cho Staging/Prod, `pretty` cho Local Dev).
    pub log_format: LogFormat,

    /// Kích thước bộ đệm hàng đợi non-blocking (số dòng log tối đa).
    #[validate(range(min = 128, max = 1_000_000))]
    pub buffered_lines_limit: usize,

    /// Cho phép drop log mới khi bộ đệm đầy để bảo vệ Tokio reactor không bị treo.
    pub lossy: bool,

    /// Địa chỉ gRPC của OpenTelemetry Collector (ví dụ: `"http://otel-collector:4317"`).
    #[validate(url(message = "logging.otlp_endpoint must be a valid URL"))]
    pub otlp_endpoint: Option<String>,
}

impl Default for LoggingSectionConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            log_format: LogFormat::Json,
            buffered_lines_limit: 10000,
            lossy: true,
            otlp_endpoint: None,
        }
    }
}
