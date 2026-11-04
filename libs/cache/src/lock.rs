//! Distributed Concurrency Control, Atomic Lua Lock & Watchdog Fencing (`lock.rs`).
//!
//! Provides single-instance lease locking with monotonically increasing 64-bit Fencing Tokens,
//! background heartbeat Watchdog renewal, and lock-loss cancellation channels.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rand::Rng;
use tokio::sync::{oneshot, watch};
use uuid::Uuid;

use crate::{
    backend::CacheBackend,
    error::{CacheError, LockAcquireReason, LockLostReason},
    instrument::{
        record_lock_acquisition, record_lock_hold_duration, record_lock_renewal_failure,
        record_lock_wait_duration,
    },
    key::make_lock_keys,
};

/// Trình quản lý khóa phân tán cho một tài nguyên nghiệp vụ cụ thể.
#[derive(Debug, Clone)]
pub struct DistributedLock<B: CacheBackend> {
    namespace: String,
    resource_id: String,
    lock_key: String,
    fence_key: String,
    backend: Arc<B>,
    default_ttl: Duration,
    renew_interval_ratio: f64,
    retry_min_delay: Duration,
    retry_max_delay: Duration,
}

impl<B: CacheBackend> DistributedLock<B> {
    /// Khởi tạo [`DistributedLock`] mới.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        prefix: &str,
        service: &str,
        namespace: impl Into<String>,
        resource_id: impl Into<String>,
        backend: Arc<B>,
        default_ttl: Duration,
        renew_interval_ratio: f64,
        retry_min_delay: Duration,
        retry_max_delay: Duration,
    ) -> Result<Self, CacheError> {
        let ns = namespace.into();
        let res_id = resource_id.into();
        let (lock_key, fence_key) = make_lock_keys(prefix, service, &ns, &res_id)?;

        Ok(Self {
            namespace: ns,
            resource_id: res_id,
            lock_key,
            fence_key,
            backend,
            default_ttl,
            renew_interval_ratio: renew_interval_ratio.clamp(0.05, 0.5),
            retry_min_delay,
            retry_max_delay,
        })
    }

    /// Tên namespace của khóa phân tán này.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Định danh tài nguyên được bảo vệ bởi khóa này.
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    /// Thử chiếm khóa đúng 1 lần duy nhất (Non-blocking Try-Lock).
    ///
    /// Trả về ngay lập tức `Err(CacheError::LockNotAcquired)` nếu tài nguyên đang bị khóa.
    pub async fn try_lock(&self) -> Result<LockGuard<B>, CacheError> {
        let owner_token = Uuid::now_v7().to_string();
        let ttl_ms = u64::try_from(self.default_ttl.as_millis()).unwrap_or(u64::MAX);

        let (acquired, fence_token) = self
            .backend
            .acquire_with_fence(&self.lock_key, &self.fence_key, &owner_token, ttl_ms)
            .await?;

        if !acquired {
            record_lock_acquisition(&self.namespace, "contended");
            return Err(CacheError::LockNotAcquired {
                resource: self.resource_id.clone(),
                reason: LockAcquireReason::Contended,
            });
        }

        record_lock_acquisition(&self.namespace, "success");
        Ok(self.create_guard(owner_token, fence_token))
    }

    /// Chờ chiếm khóa với cơ chế Truncated Exponential Backoff và Full Jitter cho đến khi vượt quá `wait_timeout`.
    pub async fn lock_with_timeout(
        &self,
        wait_timeout: Duration,
    ) -> Result<LockGuard<B>, CacheError> {
        let start = Instant::now();
        let mut attempt = 0u32;
        let mut rng = rand::rng();

        loop {
            match self.try_lock().await {
                Ok(guard) => {
                    record_lock_wait_duration(&self.namespace, start.elapsed().as_secs_f64());
                    return Ok(guard);
                }
                Err(CacheError::LockNotAcquired {
                    reason: LockAcquireReason::Contended,
                    ..
                }) => {
                    if start.elapsed() >= wait_timeout {
                        record_lock_acquisition(&self.namespace, "timeout");
                        record_lock_wait_duration(&self.namespace, start.elapsed().as_secs_f64());
                        return Err(CacheError::LockNotAcquired {
                            resource: self.resource_id.clone(),
                            reason: LockAcquireReason::Timeout,
                        });
                    }

                    // Tính toán Exponential Backoff có Jitter
                    let min_ms =
                        u64::try_from(self.retry_min_delay.as_millis()).unwrap_or(u64::MAX);
                    let max_ms =
                        u64::try_from(self.retry_max_delay.as_millis()).unwrap_or(u64::MAX);
                    let exp_ms = min_ms.saturating_mul(1 << attempt.min(6));
                    let capped_ms = exp_ms.min(max_ms);

                    let sleep_ms = rng.random_range(min_ms..=capped_ms.max(min_ms));
                    let remaining = wait_timeout.saturating_sub(start.elapsed());
                    let actual_sleep = Duration::from_millis(sleep_ms).min(remaining);

                    tokio::time::sleep(actual_sleep).await;
                    attempt = attempt.saturating_add(1);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn create_guard(&self, owner_token: String, fencing_token: u64) -> LockGuard<B> {
        let (abort_tx, abort_rx) = oneshot::channel();
        let (loss_tx, loss_rx) = watch::channel(None);

        let renew_interval = self.default_ttl.mul_f64(self.renew_interval_ratio);

        // Kích hoạt Watchdog Background Task
        spawn_watchdog(
            self.backend.clone(),
            self.lock_key.clone(),
            owner_token.clone(),
            u64::try_from(self.default_ttl.as_millis()).unwrap_or(u64::MAX),
            renew_interval,
            self.namespace.clone(),
            abort_rx,
            loss_tx,
        );

        LockGuard {
            namespace: self.namespace.clone(),
            lock_key: self.lock_key.clone(),
            owner_token,
            fencing_token,
            backend: self.backend.clone(),
            watchdog_abort_tx: Some(abort_tx),
            loss_rx,
            acquired_at: Instant::now(),
            released: false,
        }
    }
}

/// RAII Guard bảo vệ quyền sở hữu khóa phân tán.
///
/// Tự động giải phóng khóa trên Redis và dừng Watchdog khi ra khỏi scope hoặc future bị hủy.
pub struct LockGuard<B: CacheBackend> {
    namespace: String,
    lock_key: String,
    owner_token: String,
    fencing_token: u64,
    backend: Arc<B>,
    watchdog_abort_tx: Option<oneshot::Sender<()>>,
    loss_rx: watch::Receiver<Option<LockLostReason>>,
    acquired_at: Instant,
    released: bool,
}

impl<B: CacheBackend> std::fmt::Debug for LockGuard<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LockGuard")
            .field("namespace", &self.namespace)
            .field("lock_key", &self.lock_key)
            .field("fencing_token", &self.fencing_token)
            .field("released", &self.released)
            .finish_non_exhaustive()
    }
}

impl<B: CacheBackend> LockGuard<B> {
    /// Fencing Token đơn điệu tăng được cấp phát bởi Redis Lua Script.
    ///
    /// Bắt buộc phải đính kèm token này vào câu lệnh `UPDATE ... WHERE fencing_token < $1` tại PostgreSQL.
    #[must_use]
    pub fn fencing_token(&self) -> u64 {
        self.fencing_token
    }

    /// Chuỗi định danh chủ sở hữu ngẫu nhiên `UUIDv7` của khóa này (chứa Unix timestamp ms để hỗ trợ quan sát/truy vết).
    #[must_use]
    pub fn owner_token(&self) -> &str {
        &self.owner_token
    }

    /// Kênh tín hiệu cảnh báo mất khóa phân tán (`lock_loss_rx`).
    ///
    /// Cho phép tác vụ nghiệp vụ dùng `tokio::select!` hủy ngang công việc nếu Watchdog mất khóa.
    #[must_use]
    pub fn loss_signal(&self) -> watch::Receiver<Option<LockLostReason>> {
        self.loss_rx.clone()
    }

    /// Giải phóng khóa tường minh và chờ kết quả từ Redis.
    pub async fn release(&mut self) -> Result<bool, CacheError> {
        if self.released {
            return Ok(false);
        }
        self.released = true;

        // Dừng Watchdog ngay
        if let Some(tx) = self.watchdog_abort_tx.take() {
            let _ = tx.send(());
        }

        record_lock_hold_duration(&self.namespace, self.acquired_at.elapsed().as_secs_f64());
        self.backend
            .release_lock(&self.lock_key, &self.owner_token)
            .await
    }
}

impl<B: CacheBackend> Drop for LockGuard<B> {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;

        // Dừng Watchdog
        if let Some(tx) = self.watchdog_abort_tx.take() {
            let _ = tx.send(());
        }

        record_lock_hold_duration(&self.namespace, self.acquired_at.elapsed().as_secs_f64());

        // Spawn detached task giải phóng khóa trên Redis mà không block drop() đồng bộ
        let backend = self.backend.clone();
        let lock_key = self.lock_key.clone();
        let owner_token = self.owner_token.clone();

        tokio::spawn(async move {
            let _ = backend.release_lock(&lock_key, &owner_token).await;
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_watchdog<B: CacheBackend>(
    backend: Arc<B>,
    lock_key: String,
    owner_token: String,
    ttl_ms: u64,
    interval: Duration,
    namespace: String,
    mut abort_rx: oneshot::Receiver<()>,
    loss_tx: watch::Sender<Option<LockLostReason>>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut consecutive_failures = 0u32;

        loop {
            tokio::select! {
                biased;
                _ = &mut abort_rx => {
                    // LockGuard được drop hoặc release tường minh -> Watchdog dừng êm ái
                    break;
                }
                _ = ticker.tick() => {
                    match backend.extend_lock(&lock_key, &owner_token, ttl_ms).await {
                        Ok(true) => {
                            consecutive_failures = 0;
                        }
                        Ok(false) => {
                            // Khóa đã bị mất quyền sở hữu (bị cướp bởi client khác do quá hạn TTL)
                            record_lock_renewal_failure(&namespace, "preempted");
                            tracing::error!(
                                lock_key = %lock_key,
                                namespace = %namespace,
                                "Watchdog renewal rejected: Distributed lock was preempted or expired"
                            );
                            let _ = loss_tx.send(Some(LockLostReason::Preempted));
                            break;
                        }
                        Err(e) => {
                            consecutive_failures += 1;
                            record_lock_renewal_failure(&namespace, "backend_error");
                            tracing::warn!(
                                lock_key = %lock_key,
                                failures = consecutive_failures,
                                error = %e,
                                "Watchdog heartbeat renewal encountered error"
                            );

                            if consecutive_failures >= 2 {
                                tracing::error!(
                                    lock_key = %lock_key,
                                    "Watchdog failed 2 consecutive heartbeats: Signaling lock loss to worker"
                                );
                                let _ = loss_tx.send(Some(LockLostReason::RenewalFailed(e.to_string())));
                                break;
                            }
                        }
                    }
                }
            }
        }
    });
}
