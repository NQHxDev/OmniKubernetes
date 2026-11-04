//! Fault tolerance, rate limiting and circuit breaking configuration.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Cấu hình khả năng phục hồi mạng và điều phối lưu lượng.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResilienceConfig {
    /// Kích hoạt bộ giới hạn tần suất request (Rate Limiting).
    pub rate_limit_enabled: bool,

    /// Số request tối đa mỗi giây cho mỗi client IP (Requests Per Second).
    #[validate(range(min = 1, max = 100_000))]
    pub rate_limit_rps: u32,

    /// Dung lượng burst cho phép vượt đỉnh tức thời.
    #[validate(range(min = 1, max = 100_000))]
    pub rate_limit_burst: u32,

    /// Kích hoạt giới hạn tần suất phân tán qua Redis cluster-wide.
    pub distributed_rate_limit_enabled: bool,

    /// Kích hoạt cơ chế chủ động trút tải khi bão hòa (Load Shedding).
    pub load_shed_enabled: bool,

    /// Số lượng request đồng thời tối đa toàn hệ thống xử lý trước khi kích hoạt 503 Load Shed.
    #[validate(range(min = 1, max = 65_536))]
    pub max_concurrent_requests: usize,

    /// Kích hoạt bộ ngắt mạch bảo vệ downstream (Circuit Breaker).
    pub circuit_breaker_enabled: bool,

    /// Tỷ lệ lỗi kích hoạt Circuit Breaker mở mạch (%).
    #[validate(range(min = 1, max = 100))]
    pub circuit_breaker_error_threshold_pct: u8,

    /// Số lượng request tối thiểu trong cửa sổ trượt trước khi cho phép ngắt mạch.
    #[validate(range(min = 5, max = 10_000))]
    pub circuit_breaker_min_requests: u32,

    /// Thời gian chờ ở trạng thái OPEN trước khi chuyển sang `HALF_OPEN` để thử thăm dò (giây).
    #[validate(range(min = 1, max = 600))]
    pub circuit_breaker_reset_timeout_secs: u64,

    /// Số lượng request thăm dò trong trạng thái `HALF_OPEN`.
    #[validate(range(min = 1, max = 100))]
    pub circuit_breaker_half_open_probes: u32,

    /// Số lần thử lại tối đa cho các request hợp lệ (Idempotent only).
    #[validate(range(min = 0, max = 10))]
    pub max_retry_attempts: u32,

    /// Thời gian chờ khởi điểm cho thuật toán Exponential Backoff (mili-giây).
    #[validate(range(min = 10, max = 10_000))]
    pub retry_base_delay_ms: u64,

    /// Thời gian chờ tối đa cho thuật toán Exponential Backoff (mili-giây).
    #[validate(range(min = 100, max = 60_000))]
    pub retry_max_delay_ms: u64,

    /// Cửa sổ thời gian ngủ chờ Ingress/EndpointSlice đồng bộ sau khi nhận SIGTERM (giây).
    #[validate(range(min = 0, max = 30))]
    pub shutdown_quiescence_delay_secs: u64,

    /// Thời gian tối đa chờ xử lý dứt điểm các HTTP In-Flight requests đang chạy (giây).
    #[validate(range(min = 1, max = 120))]
    pub shutdown_grace_period_secs: u64,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            rate_limit_enabled: true,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
            distributed_rate_limit_enabled: false,

            load_shed_enabled: true,
            max_concurrent_requests: 1024,

            circuit_breaker_enabled: true,
            circuit_breaker_error_threshold_pct: 50,
            circuit_breaker_min_requests: 20,
            circuit_breaker_reset_timeout_secs: 30,
            circuit_breaker_half_open_probes: 5,

            max_retry_attempts: 3,
            retry_base_delay_ms: 100,
            retry_max_delay_ms: 5000,

            shutdown_quiescence_delay_secs: 5,
            shutdown_grace_period_secs: 15,
        }
    }
}
