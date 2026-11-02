//! In-Process Sharded Single-Flight Future Sharing (`stampede/single_flight.rs`).
//!
//! Deduplicates concurrent in-flight requests for the same key within a single Pod,
//! ensuring exactly one loader runs while other callers await the result via cancellation-safe
//! [`tokio::sync::watch`] channels.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use parking_lot::Mutex;
use tokio::sync::watch;

const NUM_SHARDS: usize = 16;

/// RAII Guard đảm bảo dọn sạch entry khỏi map khi Leader hoàn thành hoặc bị drop/hủy giữa chừng.
struct CleanupGuard<'a, K, V>
where
    K: Hash + Eq + Clone,
{
    flight: &'a SingleFlight<K, V>,
    key: K,
    shard_idx: usize,
}

impl<K, V> Drop for CleanupGuard<'_, K, V>
where
    K: Hash + Eq + Clone,
{
    fn drop(&mut self) {
        let mut map = self.flight.shards[self.shard_idx].lock();
        map.remove(&self.key);
    }
}

/// Trình điều phối Single-Flight 16 phân vùng (shards) giảm tối đa tranh chấp Mutex.
pub struct SingleFlight<K, V> {
    shards: [Mutex<HashMap<K, watch::Receiver<Option<V>>>>; NUM_SHARDS],
}

impl<K, V> SingleFlight<K, V> {
    /// Khởi tạo [`SingleFlight`] mới.
    #[must_use]
    pub fn new() -> Self {
        // Khởi tạo mảng 16 shards
        Self {
            shards: std::array::from_fn(|_| Mutex::new(HashMap::new())),
        }
    }

    /// Trả về số lượng công việc nạp đang chạy đồng thời trên toàn bộ 16 shards.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.shards.iter().map(|s| s.lock().len()).sum()
    }
}

impl<K, V> Default for SingleFlight<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> std::fmt::Debug for SingleFlight<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleFlight")
            .field("active_count", &self.active_count())
            .finish()
    }
}

impl<K, V> SingleFlight<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    #[inline]
    fn shard_index(key: &K) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        usize::try_from(hasher.finish() % (NUM_SHARDS as u64)).unwrap_or(0)
    }

    /// Thực thi loader nếu là Leader trúng cử, hoặc chờ kết quả từ Leader nếu là Follower.
    ///
    /// # An toàn Hủy bỏ (Cancellation Safety)
    /// - Nếu client hủy request của Follower: Leader vẫn tiếp tục hoàn tất để điền cache.
    /// - Nếu chính Leader bị client hủy kết nối: kênh `watch` bị drop, các Follower thức dậy
    ///   an toàn và một Follower tiếp theo sẽ tự động kế thừa quyền Leader mà không bị deadlock.
    pub async fn execute_or_join<F, Fut>(&self, key: K, loader: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        enum Participant<V> {
            Leader(watch::Sender<Option<V>>),
            Follower(watch::Receiver<Option<V>>),
        }

        let shard_idx = Self::shard_index(&key);

        let participant = {
            let mut map = self.shards[shard_idx].lock();
            if let Some(existing_rx) = map.get(&key) {
                Participant::Follower(existing_rx.clone())
            } else {
                let (tx, rx) = watch::channel(None);
                map.insert(key.clone(), rx);
                Participant::Leader(tx)
            }
        };

        match participant {
            Participant::Leader(tx) => {
                let cleanup_guard = CleanupGuard {
                    flight: self,
                    key: key.clone(),
                    shard_idx,
                };

                let result = loader().await;
                let _ = tx.send(Some(result.clone()));
                drop(cleanup_guard);
                result
            }
            Participant::Follower(mut rx) => loop {
                if let Some(ref val) = *rx.borrow() {
                    return val.clone();
                }

                if rx.changed().await.is_err() {
                    // Leader bị drop/hủy hoặc tiến trình nạp gặp sự cố -> Tự động thử lại để trở thành Leader mới
                    return Box::pin(self.execute_or_join(key, loader)).await;
                }
            },
        }
    }
}
