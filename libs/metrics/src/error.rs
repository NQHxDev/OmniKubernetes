//! Metrics and telemetry error definitions.

use thiserror::Error;

/// Lỗi phát sinh trong hệ thống metrics, exporter hoặc bộ thu thập dữ liệu hệ thống.
#[derive(Debug, Error)]
pub enum MetricsError {
    /// Lỗi cấu hình Prometheus recorder hoặc matcher.
    #[error("Lỗi cấu hình Prometheus Recorder: {0}")]
    RecorderInit(String),

    /// Lỗi cài đặt Prometheus global recorder.
    #[error("Lỗi cài đặt Global Recorder: {0}")]
    GlobalInstall(String),

    /// Lỗi I/O khi đọc thông số hệ thống từ `/proc/self`.
    #[error("Lỗi I/O khi đọc thông số hệ thống: {0}")]
    Io(#[from] std::io::Error),

    /// Lỗi cấu hình không hợp lệ.
    #[error("Lỗi cấu hình metrics: {0}")]
    Configuration(String),
}
