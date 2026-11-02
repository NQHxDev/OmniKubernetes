//! Invalidation Replay Buffer & Delayed Double Deletion Helpers (`invalidation.rs`).
//!
//! Provides bounded, non-blocking buffering for failed cache invalidations during network partitions
//! and background delayed double deletion for read-replica setups.

use std::{collections::VecDeque, sync::Mutex, time::Duration};

use crate::key::CacheKey;

/// Bộ đệm Invalidation Replay Buffer lưu các key xóa thất bại khi Redis tạm thời gặp sự cố.
#[derive(Debug)]
pub struct InvalidationReplayBuffer {
    capacity: usize,
    queue: Mutex<VecDeque<CacheKey>>,
}

impl InvalidationReplayBuffer {
    /// Khởi tạo [`InvalidationReplayBuffer`] mới với dung lượng giới hạn.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(100),
            queue: Mutex::new(VecDeque::with_capacity(capacity.min(1024))),
        }
    }

    /// Đẩy một key vào buffer. Nếu buffer đã đầy, ghi log ERROR và drop key cũ nhất (hoặc key hiện tại)
    /// để chặn rò rỉ bộ nhớ Pod.
    pub fn push(&self, key: CacheKey) {
        let mut lock = self.queue.lock().unwrap();
        if lock.len() >= self.capacity {
            let dropped = lock.pop_front();
            tracing::error!(
                dropped_key = ?dropped.as_ref().map(CacheKey::as_str),
                new_key = %key.as_str(),
                capacity = self.capacity,
                "Invalidation Replay Buffer overflow: dropped stale invalidation key"
            );
        }
        lock.push_back(key);
    }

    /// Rút toàn bộ các key đang tồn đọng trong buffer để thực thi xóa hàng loạt khi Redis phục hồi.
    #[must_use]
    pub fn drain_all(&self) -> Vec<CacheKey> {
        let mut lock = self.queue.lock().unwrap();
        lock.drain(..).collect()
    }

    /// Số lượng key đang chờ phát lại trong buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    /// Kiểm tra buffer có đang rỗng hay không.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }
}

/// Tiện ích hỗ trợ Delayed Double Deletion cho các môi trường có PostgreSQL Read Replica.
pub fn spawn_delayed_double_delete<F>(key: CacheKey, delay: Duration, deleter: F)
where
    F: FnOnce(CacheKey) -> futures::future::BoxFuture<'static, ()> + Send + 'static,
{
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        deleter(key).await;
    });
}
