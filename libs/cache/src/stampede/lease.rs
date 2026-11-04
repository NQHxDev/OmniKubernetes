//! Distributed Lease Coordination (Facebook Memcache Model, NSDI 2013).
//!
//! Prevents cluster-wide thundering herds across multiple Kubernetes Pod replicas
//! and eliminates stale cache overwrites after delayed database transactions.

use uuid::Uuid;

/// Kết quả của một lần thử nhận Distributed Lease trên Redis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseOutcome {
    /// Trúng cử làm Cluster Leader: nhận được lease token ngẫu nhiên.
    Acquired { token: String },
    /// Đã có Pod khác nắm giữ lease: caller trở thành Follower.
    Denied,
}

/// Trình quản lý Distributed Lease cho một lượt nạp dữ liệu.
#[derive(Debug, Clone)]
pub struct DistributedLease {
    lease_ttl_ms: u64,
    lease_wait_ms: u64,
}

impl DistributedLease {
    /// Khởi tạo [`DistributedLease`] mới.
    #[must_use]
    pub fn new(lease_ttl_ms: u64, lease_wait_ms: u64) -> Self {
        Self {
            lease_ttl_ms: lease_ttl_ms.max(100),
            lease_wait_ms: lease_wait_ms.max(10),
        }
    }

    /// Tạo một lease token ngẫu nhiên duy nhất (`UUIDv7`).
    #[must_use]
    pub fn generate_token() -> String {
        Uuid::now_v7().to_string()
    }

    /// Thời gian sống của lease (mili-giây).
    #[must_use]
    pub fn lease_ttl_ms(&self) -> u64 {
        self.lease_ttl_ms
    }

    /// Thời gian Follower chờ Cluster Leader nạp xong trước khi tự nạp lại (mili-giây).
    #[must_use]
    pub fn lease_wait_ms(&self) -> u64 {
        self.lease_wait_ms
    }
}
