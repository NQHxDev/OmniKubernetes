//! Dual-Layer Idempotent Inbox Engine (`deduplicator.rs`).
//!
//! Implements two-tier deduplication defense:
//! - **Tier 1 (Fast Path):** Sub-millisecond Redis `SET NX EX` atomic lock.
//! - **Tier 2 (Strong Consistency):** PostgreSQL `inbox_events` atomic check within local transaction.

use std::{sync::Arc, time::Duration};

#[cfg(feature = "cache-dedup")]
use omni_cache::CacheBackend;
#[cfg(feature = "postgres")]
use omni_database::DbPool;
use uuid::Uuid;

use crate::error::MessagingError;

/// Bộ điều phối khử trùng lặp 2 tầng (Dual-Layer Deduplicator).
#[derive(Clone)]
pub struct DualLayerDeduplicator {
    #[cfg(feature = "cache-dedup")]
    cache_backend: Option<Arc<dyn CacheBackend>>,
    #[cfg(feature = "postgres")]
    db_pool: Option<DbPool>,
}

impl std::fmt::Debug for DualLayerDeduplicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DualLayerDeduplicator")
            .finish_non_exhaustive()
    }
}

impl DualLayerDeduplicator {
    /// Khởi tạo [`DualLayerDeduplicator`] mới.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "cache-dedup")]
            cache_backend: None,
            #[cfg(feature = "postgres")]
            db_pool: None,
        }
    }

    /// Đính kèm Cache Backend cho Tier 1 Fast Path.
    #[cfg(feature = "cache-dedup")]
    #[must_use]
    pub fn with_cache_backend(mut self, backend: Arc<dyn CacheBackend>) -> Self {
        self.cache_backend = Some(backend);
        self
    }

    /// Đính kèm Database Pool cho Tier 2 Strong Consistency Path.
    #[cfg(feature = "postgres")]
    #[must_use]
    pub fn with_db_pool(mut self, pool: DbPool) -> Self {
        self.db_pool = Some(pool);
        self
    }

    /// Sinh khóa Redis cho bộ lọc trùng lặp Tầng 1.
    #[must_use]
    pub fn format_cache_key(consumer_group: &str, idempotency_key: &str) -> String {
        format!("inbox:{consumer_group}:{idempotency_key}")
    }

    /// Kiểm tra và chiếm khóa nhanh Tầng 1 qua Redis (`SET key "1" NX PX ttl_ms`).
    ///
    /// Trả về `Ok(true)` nếu đây là bản tin mới (chưa từng thấy).
    /// Trả về `Ok(false)` nếu phát hiện bản tin trùng lặp (khóa đã tồn tại).
    pub async fn check_and_lock_fast(
        &self,
        consumer_group: &str,
        idempotency_key: &str,
        ttl: Duration,
    ) -> Result<bool, MessagingError> {
        #[cfg(feature = "cache-dedup")]
        if let Some(ref cache) = self.cache_backend {
            let key = Self::format_cache_key(consumer_group, idempotency_key);
            let ttl_ms = ttl.as_millis() as u64;

            match cache.set_nx(&key, b"1", ttl_ms).await {
                Ok(is_new) => return Ok(is_new),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Cache deduplication fast-path failed. Degrading to DB layer."
                    );
                    // Fail-open cho tầng cache để chuyển sang tầng DB
                    return Ok(true);
                }
            }
        }

        let _ = (consumer_group, idempotency_key, ttl);
        Ok(true)
    }

    /// Giải phóng khóa nhanh Tầng 1 khi xử lý nghiệp vụ gặp lỗi tạm thời (Transient Error),
    /// cho phép lần thử lại tiếp theo không bị nhận định nhầm là bản tin trùng lặp.
    pub async fn release_fast(
        &self,
        consumer_group: &str,
        idempotency_key: &str,
    ) -> Result<(), MessagingError> {
        #[cfg(feature = "cache-dedup")]
        if let Some(ref cache) = self.cache_backend {
            let key = Self::format_cache_key(consumer_group, idempotency_key);
            let _ = cache.del(&key).await;
        }

        let _ = (consumer_group, idempotency_key);
        Ok(())
    }

    /// Kiểm tra và ghi nhận bản tin vào cơ sở dữ liệu Tầng 2 (PostgreSQL `inbox_events`).
    #[cfg(feature = "postgres")]
    pub async fn check_and_record_db(
        &self,
        id: Uuid,
        event_type: &str,
        source: &str,
        consumer_group: &str,
    ) -> Result<bool, MessagingError> {
        if let Some(ref pool) = self.db_pool {
            let mut conn = pool
                .pool()
                .acquire()
                .await
                .map_err(|e| MessagingError::DatabaseError(omni_database::DbError::from(e)))?;

            crate::inbox::repository::InboxRepository::record_if_not_exists(
                &mut conn,
                id,
                event_type,
                source,
                consumer_group,
            )
            .await
            .map_err(MessagingError::from)
        } else {
            Ok(true)
        }
    }
}

impl Default for DualLayerDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}
