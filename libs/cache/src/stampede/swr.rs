//! Stale-While-Revalidate (SWR) & Stale-If-Error (SIE) Models (RFC 5861).
//!
//! Divides cache lifespan into soft (fresh) and stale windows, serving cached content
//! immediately during revalidation or backend database failures.

use std::time::Duration;

/// Trạng thái độ tươi mới của dữ liệu cache theo chuẩn SWR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFreshness {
    /// Dữ liệu còn tươi mới hoàn toàn, phục vụ trực tiếp.
    Fresh,
    /// Dữ liệu đã vượt qua `soft_ttl` nhưng còn trong cửa sổ `stale_ttl`.
    /// Cho phép trả ngay dữ liệu stale và kích hoạt background task revalidate.
    Stale,
    /// Dữ liệu đã vượt quá `soft_ttl + stale_ttl`, bắt buộc nạp lại trước khi trả về.
    Expired,
}

/// Chính sách SWR kiểm soát thời hạn mềm và thời hạn cứng của cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwrPolicy {
    pub soft_ttl: Duration,
    pub stale_ttl: Duration,
}

impl SwrPolicy {
    /// Khởi tạo [`SwrPolicy`] mới từ thời hạn mềm và thời hạn stale (giây).
    #[must_use]
    pub fn new(soft_ttl_secs: u64, stale_ttl_secs: u64) -> Self {
        Self {
            soft_ttl: Duration::from_secs(soft_ttl_secs),
            stale_ttl: Duration::from_secs(stale_ttl_secs),
        }
    }

    /// Thời hạn cứng (hard TTL) được đặt trên Redis: `soft_ttl + stale_ttl`.
    #[must_use]
    pub fn hard_ttl(&self) -> Duration {
        self.soft_ttl + self.stale_ttl
    }

    /// Đánh giá độ tươi mới dựa trên thời gian đã trôi qua kể từ khi nạp cache.
    #[must_use]
    pub fn evaluate_freshness(&self, age: Duration) -> CacheFreshness {
        if age <= self.soft_ttl {
            CacheFreshness::Fresh
        } else if age <= self.hard_ttl() {
            CacheFreshness::Stale
        } else {
            CacheFreshness::Expired
        }
    }
}
