//! Fault tolerance, rate limiting and circuit breaking configuration.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Cấu hình khả năng phục hồi mạng và điều phối lưu lượng.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct ResilienceConfig {
    /// Kích hoạt bộ giới hạn tần suất request (Rate Limiting).
    pub rate_limit_enabled: bool,

    /// Số request tối đa mỗi giây cho mỗi client IP (Requests Per Second).
    #[validate(range(min = 1, max = 100_000))]
    pub rate_limit_rps: u32,

    /// Dung lượng burst cho phép vượt đỉnh tức thời.
    #[validate(range(min = 1, max = 100_000))]
    pub rate_limit_burst: u32,

    /// Số lượng request đồng thời tối đa toàn hệ thống xử lý.
    #[validate(range(min = 1, max = 65536))]
    pub max_concurrent_requests: usize,

    /// Tỷ lệ lỗi kích hoạt Circuit Breaker mở mạch (%).
    #[validate(range(min = 1, max = 100))]
    pub circuit_breaker_error_threshold_pct: u8,

    /// Thời gian chờ trước khi thử đóng lại Circuit Breaker (giây).
    #[validate(range(min = 1, max = 600))]
    pub circuit_breaker_reset_timeout_secs: u64,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            rate_limit_enabled: true,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
            max_concurrent_requests: 1024,
            circuit_breaker_error_threshold_pct: 50,
            circuit_breaker_reset_timeout_secs: 30,
        }
    }
}
