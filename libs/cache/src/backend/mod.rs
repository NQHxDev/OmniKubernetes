//! Cache Storage Backend Abstraction (`backend/mod.rs`).
//!
//! Provides the [`CacheBackend`] trait implemented by [`RedisBackend`] (production)
//! and [`MemoryBackend`] (standalone offline unit testing).

pub mod memory;
pub mod redis;

use std::fmt::Debug;

use async_trait::async_trait;

pub use memory::MemoryBackend;
pub use redis::RedisBackend;

use crate::error::CacheError;

/// Trait trừu tượng hóa các thao tác giao tiếp với tầng lưu trữ Redis.
#[async_trait]
pub trait CacheBackend: Debug + Send + Sync + 'static {
    /// Đọc một khóa cache (`GET`).
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError>;

    /// Đọc hàng loạt khóa cache (`MGET`).
    async fn get_many(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, CacheError>;

    /// Ghi một khóa cache kèm TTL theo mili-giây (`SET key value PX ttl_ms`).
    async fn set(&self, key: &str, value: &[u8], ttl_ms: u64) -> Result<(), CacheError>;

    /// Ghi khóa nếu chưa tồn tại (`SET key value NX PX ttl_ms`).
    async fn set_nx(&self, key: &str, value: &[u8], ttl_ms: u64) -> Result<bool, CacheError>;

    /// Xóa một khóa (`DEL`).
    async fn del(&self, key: &str) -> Result<bool, CacheError>;

    /// Xóa hàng loạt khóa (`DEL key1 key2 ...`).
    async fn del_many(&self, keys: &[&str]) -> Result<usize, CacheError>;

    /// Kiểm tra kết nối máy chủ (`PING`).
    async fn ping(&self) -> Result<(), CacheError>;

    /// Chiếm khóa phân tán và cấp phát Fencing Token qua Lua script `acquire_with_fence.lua`.
    async fn acquire_with_fence(
        &self,
        lock_key: &str,
        fence_key: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<(bool, u64), CacheError>;

    /// Giải phóng khóa phân tán an toàn qua Lua script `release.lua` (Compare-and-Delete).
    async fn release_lock(&self, lock_key: &str, owner_token: &str) -> Result<bool, CacheError>;

    /// Gia hạn khóa phân tán qua Lua script `extend.lua` (Compare-and-Pexpire).
    async fn extend_lock(
        &self,
        lock_key: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<bool, CacheError>;

    /// Ghi dữ liệu kèm kiểm tra Distributed Lease qua Lua script `write_with_lease.lua`.
    async fn write_with_lease(
        &self,
        data_key: &str,
        lease_key: &str,
        token: &str,
        payload: &[u8],
        ttl_ms: u64,
    ) -> Result<bool, CacheError>;
}
