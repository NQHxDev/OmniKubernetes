//! High-Level `CacheClient` Coordinator & Lifecycle Management (`client.rs`).
//!
//! Coordinates multiplexed connections, bounded connect retry with criticality degradation,
//! Cache Sentinel background health checks, and graceful shutdown.

use std::{sync::Arc, time::Duration};

use omni_common::config::{AppConfig, redis::RedisCriticality};
use rand::Rng;
use redis::aio::ConnectionManager;
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::watch;

use crate::{
    backend::{
        CacheBackend,
        redis::{RedisBackend, create_manager_config},
    },
    config::{CacheSettings, RedisEndpoint},
    error::CacheError,
    health::CacheHealthState,
    instrument::CacheMetricsRecorder,
    invalidation::InvalidationReplayBuffer,
    key::{CacheKey, KeyHasher, KeySpace},
    lock::DistributedLock,
    sentinel::spawn_cache_sentinel,
    typed::{CachePolicy, TypedCache},
};

/// Client điều phối chính của thư viện `omni-cache`.
#[derive(Clone)]
pub struct CacheClient {
    settings: Arc<CacheSettings>,
    backend: Arc<RedisBackend>,
    coordination_backend: Arc<RedisBackend>,
    health: Arc<CacheHealthState>,
    replay_buffer: Arc<InvalidationReplayBuffer>,
    shutdown_tx: watch::Sender<bool>,
}

impl std::fmt::Debug for CacheClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheClient")
            .field("service_name", &self.settings.service_name)
            .field("backend", &self.backend)
            .field("coordination_backend", &self.coordination_backend)
            .finish_non_exhaustive()
    }
}

impl CacheClient {
    /// Kết nối tới Redis cluster/instance với cơ chế Bounded Retry và khởi tạo Sentinel.
    pub async fn connect<C: Serialize + validator::Validate + Send + Sync + 'static>(
        app_config: &AppConfig<C>,
    ) -> Result<Self, CacheError> {
        let settings = Arc::new(CacheSettings::from_app_config(app_config)?);

        let cache_ep = RedisEndpoint::from_cache_config(&settings.config);
        let coord_ep = RedisEndpoint::from_coordination_config(&settings.config);

        let (cache_conns, coord_conns) = if settings.config.enabled {
            let cache_connections = connect_endpoint_with_retry(&settings, &cache_ep).await?;
            let coord_connections = if settings.config.coordination.is_some() {
                connect_endpoint_with_retry(&settings, &coord_ep).await?
            } else {
                cache_connections.clone()
            };
            (cache_connections, coord_connections)
        } else {
            tracing::info!(
                "Redis is disabled in configuration, CacheClient operating in no-op/bypass mode"
            );
            (Vec::new(), Vec::new())
        };

        let command_timeout = Duration::from_millis(settings.config.command_timeout_ms);
        let backend = Arc::new(RedisBackend::new(
            cache_conns,
            command_timeout,
            cache_ep.host.clone(),
            cache_ep.port,
        ));

        let coordination_backend = Arc::new(RedisBackend::new(
            coord_conns,
            command_timeout,
            coord_ep.host.clone(),
            coord_ep.port,
        ));

        let health = Arc::new(CacheHealthState::new(
            settings.config.sentinel.failure_threshold,
            settings.config.sentinel.interval_secs,
            settings.config.bypass_on_unhealthy,
        ));

        let replay_buffer = Arc::new(InvalidationReplayBuffer::new(
            settings.config.cache.invalidation_replay_capacity,
        ));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        // Khởi chạy Sentinel nếu Redis được bật
        if settings.config.enabled {
            spawn_cache_sentinel(
                backend.clone(),
                health.clone(),
                replay_buffer.clone(),
                Duration::from_secs(settings.config.sentinel.interval_secs),
                Duration::from_millis(settings.config.sentinel.ping_timeout_ms),
                shutdown_rx,
            );
        }

        Ok(Self {
            settings,
            backend,
            coordination_backend,
            health,
            replay_buffer,
            shutdown_tx,
        })
    }

    /// Tạo một phiên bản [`TypedCache<T>`] cho một domain resource và policy cụ thể.
    pub fn cache<T>(
        &self,
        resource: &'static str,
        policy: CachePolicy,
    ) -> Result<TypedCache<T, RedisBackend>, CacheError>
    where
        T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    {
        let hasher = if let Some(ref sec) = self.settings.config.key_hash_secret {
            Some(KeyHasher::new(sec.clone())?)
        } else {
            None
        };

        let keyspace = KeySpace::new(
            &self.settings.config.key_prefix,
            self.settings.service_name.as_ref(),
            resource,
            policy.schema_version,
            hasher,
        )?;

        let metrics = CacheMetricsRecorder::new(
            resource,
            self.backend.server_address(),
            self.backend.server_port(),
        );

        Ok(TypedCache::new(
            keyspace,
            self.backend.clone(),
            policy,
            self.health.clone(),
            self.replay_buffer.clone(),
            metrics,
        ))
    }

    /// Khởi tạo một [`DistributedLock`] cho một tài nguyên nghiệp vụ cụ thể.
    pub fn lock(
        &self,
        namespace: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Result<DistributedLock<RedisBackend>, CacheError> {
        let lock_cfg = &self.settings.config.lock;
        self.lock_with_ttl(
            namespace,
            resource_id,
            Duration::from_millis(lock_cfg.default_ttl_ms),
        )
    }

    /// Khởi tạo một [`DistributedLock`] kèm TTL tùy biến.
    pub fn lock_with_ttl(
        &self,
        namespace: impl Into<String>,
        resource_id: impl Into<String>,
        ttl: Duration,
    ) -> Result<DistributedLock<RedisBackend>, CacheError> {
        let lock_cfg = &self.settings.config.lock;
        DistributedLock::new(
            &self.settings.config.key_prefix,
            &self.settings.service_name,
            namespace,
            resource_id,
            self.coordination_backend.clone(),
            ttl,
            lock_cfg.renew_interval_ratio,
            Duration::from_millis(lock_cfg.retry_min_delay_ms),
            Duration::from_millis(lock_cfg.retry_max_delay_ms),
        )
    }

    /// Trạng thái sức khỏe kết nối nguyên tử (`CacheHealthState`).
    #[must_use]
    pub fn health(&self) -> Arc<CacheHealthState> {
        self.health.clone()
    }

    /// Bộ đệm Invalidation Replay Buffer.
    #[must_use]
    pub fn replay_buffer(&self) -> Arc<InvalidationReplayBuffer> {
        self.replay_buffer.clone()
    }

    /// Tắt máy khách có thứ tự và dừng Sentinel (Graceful Shutdown).
    pub async fn shutdown(&self, timeout: Duration) -> Result<(), CacheError> {
        tracing::info!("Beginning CacheClient graceful shutdown");
        let _ = self.shutdown_tx.send(true);

        // Best-effort replay các key còn tồn đọng trong replay buffer trước khi thoát
        if !self.replay_buffer.is_empty() {
            let keys = self.replay_buffer.drain_all();
            let keys_ref: Vec<&str> = keys.iter().map(CacheKey::as_str).collect();
            let _ = tokio::time::timeout(timeout, self.backend.del_many(&keys_ref)).await;
        }

        tracing::info!("CacheClient graceful shutdown completed");
        Ok(())
    }
}

async fn connect_endpoint_with_retry(
    settings: &CacheSettings,
    endpoint: &RedisEndpoint,
) -> Result<Vec<ConnectionManager>, CacheError> {
    let conn_info = settings.build_connection_info(endpoint)?;
    let client = redis::Client::open(conn_info)
        .map_err(|e| CacheError::Unavailable(format!("Failed to open Redis client: {e}")))?;

    let manager_cfg = create_manager_config(settings);
    let num_conns = settings.config.connections.clamp(1, 16);

    let retry = &settings.config.connect_retry;
    let mut rng = rand::rng();

    for attempt in 1..=retry.max_attempts {
        tracing::debug!(
            attempt = attempt,
            max_attempts = retry.max_attempts,
            host = %endpoint.host,
            port = endpoint.port,
            "Connecting to Redis endpoint"
        );

        let mut conns = Vec::with_capacity(num_conns);
        let mut failed = false;

        for _ in 0..num_conns {
            match ConnectionManager::new_with_config(client.clone(), manager_cfg.clone()).await {
                Ok(mgr) => conns.push(mgr),
                Err(err) => {
                    // Kiểm tra lỗi vĩnh viễn không thể khắc phục (Fail-Fast)
                    if is_fatal_redis_error(&err) {
                        return Err(CacheError::Unavailable(format!(
                            "Fatal Redis configuration error: {err}"
                        )));
                    }
                    tracing::warn!(
                        attempt = attempt,
                        error = %err,
                        "Redis ConnectionManager creation failed"
                    );
                    failed = true;
                    break;
                }
            }
        }

        if !failed && conns.len() == num_conns {
            tracing::info!(
                connections = num_conns,
                host = %endpoint.host,
                port = endpoint.port,
                "Successfully connected to Redis"
            );
            return Ok(conns);
        }

        if attempt < retry.max_attempts {
            let min_ms = retry.initial_backoff_ms;
            let max_ms = retry.max_backoff_ms;
            let exp_ms = min_ms.saturating_mul(1 << (attempt - 1).min(6));
            let capped_ms = exp_ms.min(max_ms);

            let sleep_ms = rng.random_range(min_ms..=capped_ms.max(min_ms));
            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        }
    }

    // Khi hết số lần thử kết nối lại
    match settings.criticality() {
        RedisCriticality::Critical => Err(CacheError::Unavailable(format!(
            "Redis connection failed after {} attempts on critical endpoint {}:{}",
            retry.max_attempts, endpoint.host, endpoint.port
        ))),
        RedisCriticality::NonCritical => {
            tracing::warn!(
                host = %endpoint.host,
                port = endpoint.port,
                "Redis connection failed after {} attempts; non-critical endpoint starting in degraded/lazy mode",
                retry.max_attempts
            );
            // Tạo lazy connection manager
            let mut conns = Vec::with_capacity(num_conns);
            for _ in 0..num_conns {
                // ConnectionManager::new_with_config creates a connection manager that will connect in background
                if let Ok(mgr) =
                    ConnectionManager::new_with_config(client.clone(), manager_cfg.clone()).await
                {
                    conns.push(mgr);
                }
            }
            Ok(conns)
        }
    }
}

fn is_fatal_redis_error(err: &redis::RedisError) -> bool {
    let s = err.to_string().to_lowercase();
    s.contains("authentication")
        || s.contains("invalid password")
        || s.contains("noperm")
        || s.contains("wrongpass")
        || s.contains("resp3 not supported")
}
