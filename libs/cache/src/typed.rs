//! High-Level `TypedCache<T>` Abstraction & Read/Write Patterns (`typed.rs`).
//!
//! Provides the primary caching interface with fail-open read semantics,
//! 6-layer stampede protection, post-commit invalidation, and negative caching.

use std::{fmt, sync::Arc, time::Duration};

use omni_common::config::redis::{RedisCacheDefaultsConfig, RedisCompression};
use omni_errors::AppError;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    backend::CacheBackend,
    codec::{decode_envelope, encode_envelope},
    health::CacheHealthState,
    instrument::{CacheMetricsRecorder, OperationTimer},
    invalidation::{InvalidationReplayBuffer, spawn_delayed_double_delete},
    key::{CacheKey, KeySpace},
    stampede::{
        DistributedLease, SingleFlight, calculate_jittered_ttl_ms, should_recompute_xfetch,
    },
};

/// Cấu hình chính sách lưu đệm cho một [`TypedCache`].
#[derive(Debug, Clone)]
pub struct CachePolicy {
    pub schema_version: u8,
    pub default_ttl: Duration,
    pub max_ttl: Duration,
    pub ttl_jitter_pct: u8,
    pub xfetch_beta: f64,
    pub stale_ttl: Duration,
    pub negative_ttl: Duration,
    pub loader_timeout: Duration,
    pub lease_ttl_ms: u64,
    pub lease_wait_ms: u64,
    pub compression_threshold_bytes: usize,
    pub max_value_bytes: usize,
    pub prefer_compression: RedisCompression,
    pub enable_lease: bool,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            default_ttl: Duration::from_secs(300),
            max_ttl: Duration::from_hours(24),
            ttl_jitter_pct: 10,
            xfetch_beta: 1.0,
            stale_ttl: Duration::from_secs(60),
            negative_ttl: Duration::from_secs(30),
            loader_timeout: Duration::from_millis(3000),
            lease_ttl_ms: 4000,
            lease_wait_ms: 200,
            compression_threshold_bytes: 8192,
            max_value_bytes: 1_048_576,
            prefer_compression: RedisCompression::Lz4,
            enable_lease: true,
        }
    }
}

impl CachePolicy {
    /// Khởi tạo [`CachePolicy`] kế thừa các giá trị mặc định từ [`RedisCacheDefaultsConfig`].
    #[must_use]
    pub fn from_config(cfg: &RedisCacheDefaultsConfig, schema_version: u8) -> Self {
        Self {
            schema_version,
            default_ttl: Duration::from_secs(cfg.default_ttl_secs),
            max_ttl: Duration::from_secs(cfg.max_ttl_secs),
            ttl_jitter_pct: cfg.ttl_jitter_pct,
            xfetch_beta: cfg.xfetch_beta,
            stale_ttl: Duration::from_secs(cfg.stale_ttl_secs),
            negative_ttl: Duration::from_secs(cfg.negative_ttl_secs),
            loader_timeout: Duration::from_millis(cfg.loader_timeout_ms),
            lease_ttl_ms: cfg.lease_ttl_ms,
            lease_wait_ms: cfg.lease_wait_ms,
            compression_threshold_bytes: 8192,
            max_value_bytes: 1_048_576,
            prefer_compression: RedisCompression::Lz4,
            enable_lease: true,
        }
    }

    /// Tùy biến schema version (dùng khi deploy phiên bản DTO mới).
    #[must_use]
    pub fn with_schema_version(mut self, schema_version: u8) -> Self {
        self.schema_version = schema_version;
        self
    }

    /// Tùy biến thời gian sống TTL.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl.min(self.max_ttl);
        self
    }
}

/// Giao diện cấp cao chính của `omni-cache` gắn với kiểu dữ liệu thực thể `T`.
pub struct TypedCache<T, B: CacheBackend> {
    keyspace: KeySpace,
    backend: Arc<B>,
    policy: CachePolicy,
    single_flight: Arc<SingleFlight<CacheKey, Result<Option<T>, AppError>>>,
    health: Arc<CacheHealthState>,
    replay_buffer: Arc<InvalidationReplayBuffer>,
    metrics: CacheMetricsRecorder,
}

impl<T, B: CacheBackend> fmt::Debug for TypedCache<T, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedCache")
            .field("keyspace", &self.keyspace)
            .field("backend", &self.backend)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl<T, B: CacheBackend> TypedCache<T, B>
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// Khởi tạo [`TypedCache`] mới.
    pub fn new(
        keyspace: KeySpace,
        backend: Arc<B>,
        policy: CachePolicy,
        health: Arc<CacheHealthState>,
        replay_buffer: Arc<InvalidationReplayBuffer>,
        metrics: CacheMetricsRecorder,
    ) -> Self {
        Self {
            keyspace,
            backend,
            policy,
            single_flight: Arc::new(SingleFlight::<CacheKey, Result<Option<T>, AppError>>::new()),
            health,
            replay_buffer,
            metrics,
        }
    }

    /// Đọc trực tiếp từ cache (không tự động nạp).
    ///
    /// Áp dụng Fail-Open: nếu Redis timeout hoặc sập, trả về `Ok(None)`.
    pub async fn get(&self, id: impl fmt::Display) -> Result<Option<T>, AppError> {
        let key = self.keyspace.key(id).map_err(AppError::from)?;
        let timer = OperationTimer::start();

        if self.health.is_bypassed() {
            self.metrics
                .record_operation("GET", "bypass", timer.elapsed_secs());
            return Ok(None);
        }

        match self.backend.get(key.as_str()).await {
            Ok(Some(bytes)) => {
                match decode_envelope::<T>(
                    &bytes,
                    self.policy.max_value_bytes,
                    self.policy.schema_version,
                ) {
                    Ok(Some(val)) => {
                        self.metrics
                            .record_operation("GET", "hit", timer.elapsed_secs());
                        Ok(Some(val))
                    }
                    Ok(None) => {
                        // Negative cache hit
                        self.metrics
                            .record_operation("GET", "negative_hit", timer.elapsed_secs());
                        Ok(None)
                    }
                    Err(e) => {
                        e.log();
                        self.metrics
                            .record_operation("GET", "decode_error", timer.elapsed_secs());
                        // Fail-open: lỗi giải mã coi như cache miss
                        Ok(None)
                    }
                }
            }
            Ok(None) => {
                self.metrics
                    .record_operation("GET", "miss", timer.elapsed_secs());
                Ok(None)
            }
            Err(e) => {
                e.log();
                self.metrics
                    .record_operation("GET", "error", timer.elapsed_secs());
                // Fail-open: lỗi Redis coi như cache miss
                Ok(None)
            }
        }
    }

    /// Ghi dữ liệu vào cache kèm TTL và nén tự động.
    pub async fn set(&self, id: impl fmt::Display, value: Option<&T>) -> Result<(), AppError> {
        let key = self.keyspace.key(id).map_err(AppError::from)?;
        let timer = OperationTimer::start();

        if self.health.is_bypassed() {
            return Ok(());
        }

        let envelope = encode_envelope(
            value,
            self.policy.compression_threshold_bytes,
            self.policy.max_value_bytes,
            self.policy.schema_version,
            self.policy.prefer_compression,
        )
        .map_err(AppError::from)?;

        let ttl_secs = if value.is_some() {
            self.policy.default_ttl.as_secs()
        } else {
            self.policy.negative_ttl.as_secs()
        };

        let ttl_ms = calculate_jittered_ttl_ms(ttl_secs * 1000, self.policy.ttl_jitter_pct);

        if let Err(e) = self.backend.set(key.as_str(), &envelope, ttl_ms).await {
            e.log();
            self.metrics
                .record_operation("SET", "error", timer.elapsed_secs());
        } else {
            self.metrics
                .record_payload_size(self.policy.prefer_compression.as_str(), envelope.len());
            self.metrics
                .record_operation("SET", "success", timer.elapsed_secs());
        }

        Ok(())
    }

    /// Vô hiệu hóa (xóa) một key khỏi cache (Post-Commit Invalidation).
    pub async fn invalidate(&self, id: impl fmt::Display) -> Result<(), AppError> {
        let key = self.keyspace.key(id).map_err(AppError::from)?;
        let timer = OperationTimer::start();

        match self.backend.del(key.as_str()).await {
            Ok(_) => {
                self.metrics.record_invalidation("immediate");
                self.metrics
                    .record_operation("DEL", "success", timer.elapsed_secs());
                Ok(())
            }
            Err(e) => {
                e.log();
                // Đưa vào Invalidation Replay Buffer để Sentinel phát lại sau
                self.replay_buffer.push(key);
                self.metrics.record_invalidation("deferred");
                self.metrics
                    .record_operation("DEL", "error", timer.elapsed_secs());
                Ok(())
            }
        }
    }

    /// Vô hiệu hóa kèm cơ chế Delayed Double Deletion cho PostgreSQL Read Replica.
    pub async fn invalidate_with_double_delete(
        &self,
        id: impl fmt::Display,
        delay: Duration,
    ) -> Result<(), AppError> {
        let key = self.keyspace.key(&id).map_err(AppError::from)?;
        self.invalidate(id).await?;

        let backend = self.backend.clone();

        spawn_delayed_double_delete(key, delay, move |k| {
            Box::pin(async move {
                let _ = backend.del(k.as_str()).await;
            })
        });

        Ok(())
    }

    /// Quy trình đọc và nạp toàn diện 6 lớp bảo vệ (`get_or_load`).
    ///
    /// 1. Kiểm tra Sentinel Bypass (~1ns).
    /// 2. Đọc Redis; nếu Cache HIT kiểm tra `XFetch` để nạp ngầm.
    /// 3. Nếu Cache MISS, đưa qua Single-Flight cục bộ (gom 10,000 req trong Pod thành 1).
    /// 4. Leader trúng cử dùng Distributed Lease để gom nhiều Pod thành 1.
    /// 5. Gọi `loader` nạp dữ liệu từ PostgreSQL.
    /// 6. Ghi Redis an toàn chống Stale Set kèm Jittered TTL hoặc Negative Sentinel.
    #[allow(clippy::too_many_lines)]
    pub async fn get_or_load<F, Fut>(
        &self,
        id: impl fmt::Display,
        loader: F,
    ) -> Result<Option<T>, AppError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Option<T>, AppError>> + Send + 'static,
    {
        let key = self.keyspace.key(id).map_err(AppError::from)?;
        let timer = OperationTimer::start();

        // Lớp 1: Kiểm tra Sentinel Bypass
        if self.health.is_bypassed() {
            self.metrics
                .record_operation("GET_OR_LOAD", "bypass", timer.elapsed_secs());
            return self.single_flight.execute_or_join(key, loader).await;
        }

        // Lớp 2: Thử đọc Redis (Cache Hit / Miss)
        match self.backend.get(key.as_str()).await {
            Ok(Some(bytes)) => {
                match decode_envelope::<T>(
                    &bytes,
                    self.policy.max_value_bytes,
                    self.policy.schema_version,
                ) {
                    Ok(Some(val)) => {
                        self.metrics
                            .record_operation("GET_OR_LOAD", "hit", timer.elapsed_secs());

                        // Lớp 3: Kiểm tra XFetch kích hoạt nạp ngầm sớm
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let delta_ms = (timer.elapsed_secs() * 1000.0) as u64;
                        let now_ms =
                            u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0);
                        let expiry_ms = now_ms
                            + u64::try_from(self.policy.default_ttl.as_millis()).unwrap_or(0);

                        if should_recompute_xfetch(
                            now_ms,
                            expiry_ms,
                            delta_ms,
                            self.policy.xfetch_beta,
                        ) {
                            self.metrics.record_xfetch_recomputed();
                            let bg_self_backend = self.backend.clone();
                            let bg_key = key.clone();
                            let bg_policy = self.policy.clone();
                            tokio::spawn(async move {
                                if let Ok(Some(fresh_data)) = loader().await
                                    && let Ok(env) = encode_envelope(
                                        Some(&fresh_data),
                                        bg_policy.compression_threshold_bytes,
                                        bg_policy.max_value_bytes,
                                        bg_policy.schema_version,
                                        bg_policy.prefer_compression,
                                    )
                                {
                                    let ttl_ms = calculate_jittered_ttl_ms(
                                        u64::try_from(bg_policy.default_ttl.as_millis())
                                            .unwrap_or(u64::MAX),
                                        bg_policy.ttl_jitter_pct,
                                    );
                                    let _ =
                                        bg_self_backend.set(bg_key.as_str(), &env, ttl_ms).await;
                                }
                            });
                        }

                        return Ok(Some(val));
                    }
                    Ok(None) => {
                        // Negative Cache Hit
                        self.metrics.record_operation(
                            "GET_OR_LOAD",
                            "negative_hit",
                            timer.elapsed_secs(),
                        );
                        return Ok(None);
                    }
                    Err(e) => {
                        e.log();
                        // Envelope lỗi, tiếp tục xuống Single-Flight để nạp lại từ DB
                    }
                }
            }
            Ok(None) => {
                // Cache Miss bình thường
            }
            Err(e) => {
                e.log();
                // Redis lỗi mạng, Fail-Open tiếp tục xuống Single-Flight
            }
        }

        self.metrics
            .record_operation("GET_OR_LOAD", "miss", timer.elapsed_secs());

        // Lớp 4: In-Process Single-Flight
        let backend = self.backend.clone();
        let policy = self.policy.clone();
        let metrics = self.metrics.clone();
        let flight_key = key.clone();

        self.single_flight
            .execute_or_join(flight_key, move || async move {
                // Lớp 5: Distributed Lease (nếu được bật)
                if policy.enable_lease {
                    let lease_token = DistributedLease::generate_token();
                    let lease_key = key.lease_key();

                    match backend
                        .set_nx(
                            lease_key.as_str(),
                            lease_token.as_bytes(),
                            policy.lease_ttl_ms,
                        )
                        .await
                    {
                        Ok(true) => {
                            // Trúng cử làm Cluster Leader
                            metrics.record_lease("acquired");
                            let load_res = loader().await;

                            match &load_res {
                                Ok(Some(data)) => {
                                    if let Ok(env) = encode_envelope(
                                        Some(data),
                                        policy.compression_threshold_bytes,
                                        policy.max_value_bytes,
                                        policy.schema_version,
                                        policy.prefer_compression,
                                    ) {
                                        let ttl_ms = calculate_jittered_ttl_ms(
                                            u64::try_from(policy.default_ttl.as_millis())
                                                .unwrap_or(u64::MAX),
                                            policy.ttl_jitter_pct,
                                        );
                                        let _ = backend
                                            .write_with_lease(
                                                key.as_str(),
                                                lease_key.as_str(),
                                                &lease_token,
                                                &env,
                                                ttl_ms,
                                            )
                                            .await;
                                    }
                                }
                                Ok(None) => {
                                    // Negative Caching (Lớp 6)
                                    if let Ok(env) = encode_envelope::<T>(
                                        None,
                                        policy.compression_threshold_bytes,
                                        policy.max_value_bytes,
                                        policy.schema_version,
                                        policy.prefer_compression,
                                    ) {
                                        let ttl_ms = calculate_jittered_ttl_ms(
                                            u64::try_from(policy.negative_ttl.as_millis())
                                                .unwrap_or(u64::MAX),
                                            policy.ttl_jitter_pct,
                                        );
                                        let _ = backend
                                            .write_with_lease(
                                                key.as_str(),
                                                lease_key.as_str(),
                                                &lease_token,
                                                &env,
                                                ttl_ms,
                                            )
                                            .await;
                                    }
                                }
                                Err(_) => {
                                    // Loader lỗi, giải phóng lease ngay
                                    let _ = backend.del(lease_key.as_str()).await;
                                }
                            }
                            load_res
                        }
                        Ok(false) => {
                            // Không giành được lease -> Trở thành Follower
                            tokio::time::sleep(Duration::from_millis(policy.lease_wait_ms)).await;

                            // Thử đọc lại Redis sau khi Leader nạp xong
                            if let Ok(Some(bytes)) = backend.get(key.as_str()).await
                                && let Ok(val) = decode_envelope::<T>(
                                    &bytes,
                                    policy.max_value_bytes,
                                    policy.schema_version,
                                )
                            {
                                metrics.record_lease("waited_hit");
                                return Ok(val);
                            }

                            // Nếu Leader thất bại hoặc timeout -> Tự động nạp trực tiếp
                            metrics.record_lease("waited_miss");
                            loader().await
                        }
                        Err(_) => {
                            // Lỗi Redis khi xin lease -> Fallback nạp trực tiếp
                            loader().await
                        }
                    }
                } else {
                    // Lease không bật: nạp trực tiếp và ghi Redis
                    let load_res = loader().await;
                    if let Ok(Some(ref data)) = load_res
                        && let Ok(env) = encode_envelope(
                            Some(data),
                            policy.compression_threshold_bytes,
                            policy.max_value_bytes,
                            policy.schema_version,
                            policy.prefer_compression,
                        )
                    {
                        let ttl_ms = calculate_jittered_ttl_ms(
                            u64::try_from(policy.default_ttl.as_millis()).unwrap_or(u64::MAX),
                            policy.ttl_jitter_pct,
                        );
                        let _ = backend.set(key.as_str(), &env, ttl_ms).await;
                    } else if let Ok(None) = load_res
                        && let Ok(env) = encode_envelope::<T>(
                            None,
                            policy.compression_threshold_bytes,
                            policy.max_value_bytes,
                            policy.schema_version,
                            policy.prefer_compression,
                        )
                    {
                        let ttl_ms = calculate_jittered_ttl_ms(
                            u64::try_from(policy.negative_ttl.as_millis()).unwrap_or(u64::MAX),
                            policy.ttl_jitter_pct,
                        );
                        let _ = backend.set(key.as_str(), &env, ttl_ms).await;
                    }
                    load_res
                }
            })
            .await
    }
}
