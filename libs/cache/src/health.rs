//! Kubernetes Readiness Probe Caching & Lock-Free Health State (`health.rs`).
//!
//! Provides atomic, decoupled health tracking between the background Sentinel ping loop
//! and the Kubelet `/readyz` scrape frequency.

use std::{
    sync::RwLock,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn current_timestamp_millis() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

/// Trạng thái sức khỏe kết nối Redis được đồng bộ hóa nguyên tử (Lock-Free).
#[derive(Debug)]
pub struct CacheHealthState {
    consecutive_failures: AtomicU32,
    last_success_millis: AtomicU64,
    last_error: RwLock<Option<String>>,
    bypassed: AtomicBool,
    failure_threshold: u32,
    interval_secs: u64,
    bypass_on_unhealthy: bool,
}

impl CacheHealthState {
    /// Khởi tạo [`CacheHealthState`] mới.
    #[must_use]
    pub fn new(failure_threshold: u32, interval_secs: u64, bypass_on_unhealthy: bool) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            last_success_millis: AtomicU64::new(current_timestamp_millis()),
            last_error: RwLock::new(None),
            bypassed: AtomicBool::new(false),
            failure_threshold: failure_threshold.max(1),
            interval_secs: interval_secs.max(1),
            bypass_on_unhealthy,
        }
    }

    /// Kiểm tra xem Redis có đang ở chế độ Bypass hay không (tốn ~1ns).
    #[inline]
    #[must_use]
    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Relaxed)
    }

    /// Thiết lập trạng thái Bypass thủ công (cho kiểm thử hoặc can thiệp khẩn cấp).
    pub fn set_bypassed(&self, bypassed: bool) {
        self.bypassed.store(bypassed, Ordering::Release);
    }

    /// Ghi nhận kiểm tra PING thành công, tự động tắt chế độ Bypass nếu trước đó đang bật.
    pub fn record_success(&self) -> bool {
        self.consecutive_failures.store(0, Ordering::Release);
        self.last_success_millis
            .store(current_timestamp_millis(), Ordering::Release);
        if let Ok(mut lock) = self.last_error.write() {
            *lock = None;
        }

        let was_bypassed = self.bypassed.swap(false, Ordering::AcqRel);
        if was_bypassed {
            tracing::info!("Redis health restored: Exiting Bypass mode");
        }
        was_bypassed
    }

    /// Ghi nhận kiểm tra PING thất bại kèm nguyên nhân, tự động kích hoạt Bypass nếu vượt ngưỡng.
    pub fn record_failure(&self, reason: &str) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut lock) = self.last_error.write() {
            *lock = Some(reason.to_string());
        }

        if self.bypass_on_unhealthy && failures >= self.failure_threshold {
            let was_bypassed = self.bypassed.swap(true, Ordering::AcqRel);
            if !was_bypassed {
                tracing::warn!(
                    failures = failures,
                    threshold = self.failure_threshold,
                    reason = %reason,
                    "Redis failure threshold reached: Activating Bypass mode"
                );
            }
        }
    }

    /// Số lần thất bại liên tiếp hiện tại.
    #[must_use]
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Acquire)
    }

    /// Đánh giá trạng thái sẵn sàng phục vụ của Redis.
    ///
    /// # Errors
    /// Trả về `Err(String)` nếu số lần lỗi liên tiếp vượt ngưỡng hoặc dữ liệu bị cũ quá 3 chu kỳ.
    pub fn evaluate(&self) -> Result<(), String> {
        let failures = self.consecutive_failures.load(Ordering::Acquire);
        if failures >= self.failure_threshold {
            let reason = self
                .last_error
                .read()
                .ok()
                .and_then(|r| r.clone())
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(format!(
                "Redis connection check failed {failures} consecutive times (threshold: {}): {reason}",
                self.failure_threshold
            ));
        }

        let last_success = self.last_success_millis.load(Ordering::Acquire);
        let now = current_timestamp_millis();
        let max_stale_millis = self.interval_secs * 3 * 1000;
        if now.saturating_sub(last_success) > max_stale_millis {
            return Err(format!(
                "Redis health check is stale (last success {}s ago, max allowed {}s)",
                now.saturating_sub(last_success) / 1000,
                max_stale_millis / 1000
            ));
        }

        Ok(())
    }
}

/// Bộ kiểm tra sức khỏe Redis tích hợp với Kubernetes Readiness Probe (`omni-metrics`).
#[cfg(feature = "probes")]
#[derive(Debug, Clone)]
pub struct RedisHealthIndicator {
    health: std::sync::Arc<CacheHealthState>,
    criticality: omni_metrics::DependencyCriticality,
}

#[cfg(feature = "probes")]
impl RedisHealthIndicator {
    /// Khởi tạo [`RedisHealthIndicator`] mới.
    #[must_use]
    pub fn new(
        health: std::sync::Arc<CacheHealthState>,
        criticality: omni_metrics::DependencyCriticality,
    ) -> Self {
        Self {
            health,
            criticality,
        }
    }
}

#[cfg(feature = "probes")]
#[async_trait::async_trait]
impl omni_metrics::HealthIndicator for RedisHealthIndicator {
    fn name(&self) -> &'static str {
        "redis"
    }

    fn criticality(&self) -> omni_metrics::DependencyCriticality {
        self.criticality
    }

    async fn check(&self) -> Result<(), String> {
        self.health.evaluate()
    }
}
