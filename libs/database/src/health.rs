//! Kubernetes Readiness Probe caching and pool health state.
//!
//! Provides lock-free atomic tracking of PostgreSQL connectivity health,
//! decoupled from Kubelet `/readyz` scrape frequency to prevent cascading probe timeouts.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pool::DbPool;

/// Trạng thái sức khỏe kết nối cơ sở dữ liệu được đồng bộ hóa nguyên tử (Lock-free).
#[derive(Debug)]
pub struct PoolHealthState {
    consecutive_failures: AtomicU32,
    last_success_millis: AtomicU64,
    last_error: RwLock<Option<String>>,
    failure_threshold: u32,
    interval_secs: u64,
}

fn current_timestamp_millis() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

impl PoolHealthState {
    /// Khởi tạo [`PoolHealthState`] mới với ngưỡng số lần lỗi và chu kỳ ping.
    #[must_use]
    pub fn new(failure_threshold: u32, interval_secs: u64) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            last_success_millis: AtomicU64::new(current_timestamp_millis()),
            last_error: RwLock::new(None),
            failure_threshold: failure_threshold.max(1),
            interval_secs: interval_secs.max(1),
        }
    }

    /// Ghi nhận một lượt kiểm tra nhịp tim thành công.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Release);
        self.last_success_millis
            .store(current_timestamp_millis(), Ordering::Release);
        if let Ok(mut lock) = self.last_error.write() {
            *lock = None;
        }
    }

    /// Ghi nhận một lượt kiểm tra nhịp tim thất bại kèm nguyên nhân.
    pub fn record_failure(&self, reason: String) {
        self.consecutive_failures.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut lock) = self.last_error.write() {
            *lock = Some(reason);
        }
    }

    /// Số lần thất bại liên tiếp hiện tại.
    #[must_use]
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Acquire)
    }

    /// Đánh giá trạng thái sẵn sàng phục vụ của PostgreSQL.
    ///
    /// # Errors
    /// Trả về `Err(String)` nếu pool đã bị đóng, số lần lỗi liên tiếp vượt ngưỡng,
    /// hoặc dữ liệu kiểm tra bị cũ quá 3 chu kỳ (Sentinel bị treo).
    pub fn evaluate(&self, is_closed: bool) -> Result<(), String> {
        if is_closed {
            return Err("PostgreSQL connection pool is closed".to_string());
        }

        let failures = self.consecutive_failures.load(Ordering::Acquire);
        if failures >= self.failure_threshold {
            let reason = self
                .last_error
                .read()
                .ok()
                .and_then(|r| r.clone())
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(format!(
                "PostgreSQL connection check failed {failures} consecutive times (threshold: {}): {reason}",
                self.failure_threshold
            ));
        }

        let last_success = self.last_success_millis.load(Ordering::Acquire);
        let now = current_timestamp_millis();
        let max_stale_millis = self.interval_secs * 3 * 1000;
        if now.saturating_sub(last_success) > max_stale_millis {
            return Err(format!(
                "PostgreSQL health check is stale (last success {}s ago, max allowed {}s)",
                now.saturating_sub(last_success) / 1000,
                max_stale_millis / 1000
            ));
        }

        Ok(())
    }
}

/// Bộ kiểm tra sức khỏe cơ sở dữ liệu tích hợp với Kubernetes Readiness Probe (`omni-metrics`).
#[cfg(feature = "probes")]
#[derive(Debug, Clone)]
pub struct PgHealthIndicator {
    db: DbPool,
}

#[cfg(feature = "probes")]
impl PgHealthIndicator {
    /// Khởi tạo [`PgHealthIndicator`] mới từ [`DbPool`].
    #[must_use]
    pub fn new(db: DbPool) -> Self {
        Self { db }
    }
}

#[cfg(feature = "probes")]
#[async_trait::async_trait]
impl omni_metrics::HealthIndicator for PgHealthIndicator {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn criticality(&self) -> omni_metrics::DependencyCriticality {
        omni_metrics::DependencyCriticality::Critical
    }

    async fn check(&self) -> Result<(), String> {
        self.db.health().evaluate(self.db.pool().is_closed())
    }
}
