//! OpenTelemetry Database Semantic Conventions & Prometheus Metrics Instrumentation (`instrument.rs`).
//!
//! Emits standardized spans and counters while enforcing strict low-cardinality guardrails
//! (never emitting dynamic IDs or values into labels or span names).

use std::time::Instant;

use metrics::{counter, gauge, histogram};

/// Ngưỡng thời gian phát hiện lệnh Redis thực thi chậm (50ms).
pub const SLOW_COMMAND_THRESHOLD_MS: u64 = 50;

/// Bộ ghi nhận metrics chuẩn cho một resource cache cụ thể.
#[derive(Debug, Clone)]
pub struct CacheMetricsRecorder {
    resource: &'static str,
    server_address: String,
    server_port: u16,
}

impl CacheMetricsRecorder {
    /// Khởi tạo [`CacheMetricsRecorder`] mới.
    #[must_use]
    pub fn new(
        resource: &'static str,
        server_address: impl Into<String>,
        server_port: u16,
    ) -> Self {
        Self {
            resource,
            server_address: server_address.into(),
            server_port,
        }
    }

    /// Tên resource tĩnh gắn với recorder này.
    #[must_use]
    pub fn resource(&self) -> &'static str {
        self.resource
    }

    /// Ghi nhận kết quả thao tác cache (`hit`, `miss`, `stale`, `bypass`, `negative_hit`).
    pub fn record_operation(&self, op: &'static str, outcome: &'static str, duration_secs: f64) {
        counter!(
            "cache_operations_total",
            "resource" => self.resource,
            "op" => op,
            "outcome" => outcome,
        )
        .increment(1);

        histogram!(
            "cache_operation_duration_seconds",
            "resource" => self.resource,
            "op" => op,
        )
        .record(duration_secs);

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let elapsed_ms = (duration_secs * 1000.0) as u64;
        if elapsed_ms >= SLOW_COMMAND_THRESHOLD_MS {
            tracing::warn!(
                target: "omni_cache::instrument",
                db_system = "redis",
                db_operation = op,
                db_namespace = self.resource,
                elapsed_ms = elapsed_ms,
                threshold_ms = SLOW_COMMAND_THRESHOLD_MS,
                server_address = %self.server_address,
                server_port = self.server_port,
                "Slow Redis command detected"
            );
        }
    }

    /// Ghi nhận kích thước payload sau khi đóng gói envelope.
    #[allow(clippy::cast_precision_loss)]
    pub fn record_payload_size(&self, compression: &'static str, bytes_len: usize) {
        histogram!(
            "cache_payload_bytes",
            "resource" => self.resource,
            "compression" => compression,
        )
        .record(bytes_len as f64);
    }

    /// Ghi nhận số lượng request được gộp thành công qua Single-Flight.
    pub fn record_singleflight_deduplicated(&self) {
        counter!(
            "cache_singleflight_deduplicated_total",
            "resource" => self.resource,
        )
        .increment(1);
    }

    /// Ghi nhận sự kiện thuật toán `XFetch` kích hoạt nạp lại trước khi hết hạn.
    pub fn record_xfetch_recomputed(&self) {
        counter!(
            "cache_xfetch_early_recomputed_total",
            "resource" => self.resource,
        )
        .increment(1);
    }

    /// Ghi nhận kết quả phân tán lease (`acquired`, `waited_hit`, `waited_miss`).
    pub fn record_lease(&self, outcome: &'static str) {
        counter!(
            "cache_leases_total",
            "resource" => self.resource,
            "outcome" => outcome,
        )
        .increment(1);
    }

    /// Ghi nhận kết quả invalidation (`immediate`, `replayed`, `dropped`).
    pub fn record_invalidation(&self, outcome: &'static str) {
        counter!(
            "cache_invalidations_total",
            "resource" => self.resource,
            "outcome" => outcome,
        )
        .increment(1);
    }
}

/// Ghi nhận metrics của tầng Distributed Lock.
pub fn record_lock_acquisition(namespace: &str, outcome: &'static str) {
    counter!(
        "lock_acquisitions_total",
        "namespace" => namespace.to_string(),
        "outcome" => outcome,
    )
    .increment(1);
}

/// Ghi nhận thời gian giữ khóa phân tán (tính từ lúc acquire thành công đến khi drop).
pub fn record_lock_hold_duration(namespace: &str, duration_secs: f64) {
    histogram!(
        "lock_hold_duration_seconds",
        "namespace" => namespace.to_string(),
    )
    .record(duration_secs);
}

/// Ghi nhận thời gian chờ đợi acquire khóa phân tán.
pub fn record_lock_wait_duration(namespace: &str, duration_secs: f64) {
    histogram!(
        "lock_wait_duration_seconds",
        "namespace" => namespace.to_string(),
    )
    .record(duration_secs);
}

/// Ghi nhận số lần Watchdog gia hạn khóa thất bại.
pub fn record_lock_renewal_failure(namespace: &str, reason: &'static str) {
    counter!(
        "lock_renewal_failures_total",
        "namespace" => namespace.to_string(),
        "reason" => reason,
    )
    .increment(1);
}

/// Cập nhật trạng thái kết nối máy khách Redis.
pub fn update_redis_connection_status(endpoint_role: &'static str, connected: bool) {
    gauge!(
        "redis_client_connected",
        "endpoint_role" => endpoint_role,
    )
    .set(if connected { 1.0 } else { 0.0 });
}

/// Cập nhật trạng thái Bypass của máy khách Redis.
pub fn update_redis_bypass_status(endpoint_role: &'static str, active: bool) {
    gauge!(
        "redis_client_bypass_active",
        "endpoint_role" => endpoint_role,
    )
    .set(if active { 1.0 } else { 0.0 });
}

/// Tiện ích đo lường thời gian thực thi (Latency Timer).
#[derive(Debug)]
pub struct OperationTimer {
    start: Instant,
}

impl OperationTimer {
    /// Bắt đầu bấm giờ thao tác.
    #[must_use]
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Trả về thời gian đã trôi qua tính theo giây (f64).
    #[must_use]
    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}
