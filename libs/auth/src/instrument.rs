//! Prometheus metrics and OpenTelemetry security audit instrumentation.

use std::time::Instant;

/// Đếm tổng số lần thẩm định token với nhãn trạng thái và thuật toán.
pub fn record_token_validation(status: &'static str, algorithm: &'static str) {
    metrics::counter!(
        "auth_tokens_validated_total",
        "status" => status,
        "algorithm" => algorithm
    )
    .increment(1);
}

/// Đếm số lần từ chối xác thực / phân quyền với lý do chuẩn hóa (Low-Cardinality).
pub fn record_auth_failure(reason: &'static str) {
    metrics::counter!(
        "auth_failures_total",
        "reason" => reason
    )
    .increment(1);
}

/// Ghi nhận thời gian thực thi mật mã (Argon2, JWT sign/verify).
pub fn record_crypto_duration(operation: &'static str, duration_secs: f64) {
    metrics::histogram!(
        "auth_crypto_duration_seconds",
        "operation" => operation
    )
    .record(duration_secs);
}

/// Đo lường thời lượng thao tác mật mã bằng RAII guard.
#[derive(Debug)]
pub struct CryptoTimer {
    operation: &'static str,
    start: Instant,
}

impl CryptoTimer {
    #[must_use]
    pub fn start(operation: &'static str) -> Self {
        Self {
            operation,
            start: Instant::now(),
        }
    }

    pub fn complete(self) {
        let elapsed = self.start.elapsed().as_secs_f64();
        record_crypto_duration(self.operation, elapsed);
    }
}
