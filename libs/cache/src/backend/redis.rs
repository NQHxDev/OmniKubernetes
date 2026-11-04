//! High-Performance Multiplexed Redis Storage Backend (`backend/redis.rs`).
//!
//! Manages K round-robin [`redis::aio::ConnectionManager`] instances with external timeout bounds,
//! pipeline queuing, and atomic Lua script execution.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};

use crate::{backend::CacheBackend, config::CacheSettings, error::CacheError, scripts::LuaScripts};

/// Cấu hình và tạo một [`ConnectionManagerConfig`] từ [`CacheSettings`].
#[must_use]
pub fn create_manager_config(settings: &CacheSettings) -> ConnectionManagerConfig {
    let reconnect = &settings.config.reconnect;
    let mut cfg = ConnectionManagerConfig::new();
    cfg = cfg
        .set_number_of_retries(reconnect.max_retries as usize)
        .set_min_delay(Duration::from_millis(reconnect.min_delay_ms))
        .set_max_delay(Duration::from_millis(reconnect.max_delay_ms))
        .set_connection_timeout(Some(Duration::from_millis(
            settings.config.connection_timeout_ms,
        )))
        .set_response_timeout(Some(Duration::from_millis(
            settings.config.command_timeout_ms,
        )));

    cfg
}

/// Tầng triển khai thực tế trên `redis-rs` với K kết nối multiplexed round-robin.
#[derive(Clone)]
pub struct RedisBackend {
    conns: Arc<[ConnectionManager]>,
    next: Arc<AtomicUsize>,
    command_timeout: Duration,
    scripts: LuaScripts,
    server_address: String,
    server_port: u16,
}

impl std::fmt::Debug for RedisBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisBackend")
            .field("connections_count", &self.conns.len())
            .field("command_timeout", &self.command_timeout)
            .field("server_address", &self.server_address)
            .field("server_port", &self.server_port)
            .finish_non_exhaustive()
    }
}

impl RedisBackend {
    /// Khởi tạo [`RedisBackend`] mới từ danh sách kết nối đã thiết lập.
    #[must_use]
    pub fn new(
        conns: Vec<ConnectionManager>,
        command_timeout: Duration,
        server_address: impl Into<String>,
        server_port: u16,
    ) -> Self {
        Self {
            conns: conns.into(),
            next: Arc::new(AtomicUsize::new(0)),
            command_timeout,
            scripts: LuaScripts::new(),
            server_address: server_address.into(),
            server_port,
        }
    }

    /// Chọn một kết nối `ConnectionManager` theo thuật toán round-robin (Zero contention).
    #[inline]
    pub fn conn(&self) -> ConnectionManager {
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.conns.len();
        self.conns[idx].clone()
    }

    /// Địa chỉ máy chủ Redis.
    #[must_use]
    pub fn server_address(&self) -> &str {
        &self.server_address
    }

    /// Cổng kết nối Redis.
    #[must_use]
    pub fn server_port(&self) -> u16 {
        self.server_port
    }

    /// Thực thi lệnh Redis bọc trong timeout ngoài cứng (Outer Timeout Envelope).
    pub async fn run<T, F>(
        &self,
        op: &'static str,
        f: impl FnOnce(ConnectionManager) -> F,
    ) -> Result<T, CacheError>
    where
        F: Future<Output = redis::RedisResult<T>>,
    {
        let conn = self.conn();
        match tokio::time::timeout(self.command_timeout, f(conn)).await {
            Ok(Ok(res)) => Ok(res),
            Ok(Err(e)) => Err(CacheError::from_redis(op, e)),
            Err(_) => Err(CacheError::Timeout { op }),
        }
    }
}

#[async_trait]
impl CacheBackend for RedisBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        let key_owned = key.to_string();
        self.run("GET", |mut c| async move {
            redis::cmd("GET").arg(key_owned).query_async(&mut c).await
        })
        .await
    }

    async fn get_many(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let keys_owned: Vec<String> = keys.iter().map(|k| (*k).to_string()).collect();
        self.run("MGET", |mut c| async move {
            redis::cmd("MGET")
                .arg(&keys_owned)
                .query_async(&mut c)
                .await
        })
        .await
    }

    async fn set(&self, key: &str, value: &[u8], ttl_ms: u64) -> Result<(), CacheError> {
        let key_owned = key.to_string();
        let val_owned = value.to_vec();
        self.run("SET", |mut c| async move {
            redis::cmd("SET")
                .arg(key_owned)
                .arg(val_owned)
                .arg("PX")
                .arg(ttl_ms)
                .query_async(&mut c)
                .await
        })
        .await
    }

    async fn set_nx(&self, key: &str, value: &[u8], ttl_ms: u64) -> Result<bool, CacheError> {
        let key_owned = key.to_string();
        let val_owned = value.to_vec();
        let res: Option<String> = self
            .run("SET_NX", |mut c| async move {
                redis::cmd("SET")
                    .arg(key_owned)
                    .arg(val_owned)
                    .arg("NX")
                    .arg("PX")
                    .arg(ttl_ms)
                    .query_async(&mut c)
                    .await
            })
            .await?;
        Ok(res.is_some())
    }

    async fn del(&self, key: &str) -> Result<bool, CacheError> {
        let key_owned = key.to_string();
        let count: usize = self
            .run("DEL", |mut c| async move {
                redis::cmd("DEL").arg(key_owned).query_async(&mut c).await
            })
            .await?;
        Ok(count > 0)
    }

    async fn del_many(&self, keys: &[&str]) -> Result<usize, CacheError> {
        if keys.is_empty() {
            return Ok(0);
        }
        let keys_owned: Vec<String> = keys.iter().map(|k| (*k).to_string()).collect();
        self.run("DEL", |mut c| async move {
            redis::cmd("DEL").arg(&keys_owned).query_async(&mut c).await
        })
        .await
    }

    async fn ping(&self) -> Result<(), CacheError> {
        let _: String = self
            .run("PING", |mut c| async move {
                redis::cmd("PING").query_async(&mut c).await
            })
            .await?;
        Ok(())
    }

    async fn acquire_with_fence(
        &self,
        lock_key: &str,
        fence_key: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<(bool, u64), CacheError> {
        let l_key = lock_key.to_string();
        let f_key = fence_key.to_string();
        let token = owner_token.to_string();
        let script = self.scripts.acquire_fence.clone();

        let (status, fence): (i64, u64) = self
            .run("EVALSHA_ACQUIRE", |mut c| async move {
                script
                    .key(l_key)
                    .key(f_key)
                    .arg(token)
                    .arg(ttl_ms)
                    .invoke_async(&mut c)
                    .await
            })
            .await?;

        Ok((status == 1, fence))
    }

    async fn release_lock(&self, lock_key: &str, owner_token: &str) -> Result<bool, CacheError> {
        let l_key = lock_key.to_string();
        let token = owner_token.to_string();
        let script = self.scripts.release.clone();

        let res: i64 = self
            .run("EVALSHA_RELEASE", |mut c| async move {
                script.key(l_key).arg(token).invoke_async(&mut c).await
            })
            .await?;

        Ok(res == 1)
    }

    async fn extend_lock(
        &self,
        lock_key: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<bool, CacheError> {
        let l_key = lock_key.to_string();
        let token = owner_token.to_string();
        let script = self.scripts.extend.clone();

        let res: i64 = self
            .run("EVALSHA_EXTEND", |mut c| async move {
                script
                    .key(l_key)
                    .arg(token)
                    .arg(ttl_ms)
                    .invoke_async(&mut c)
                    .await
            })
            .await?;

        Ok(res == 1)
    }

    async fn write_with_lease(
        &self,
        data_key: &str,
        lease_key: &str,
        token: &str,
        payload: &[u8],
        ttl_ms: u64,
    ) -> Result<bool, CacheError> {
        let d_key = data_key.to_string();
        let l_key = lease_key.to_string();
        let tok = token.to_string();
        let data = payload.to_vec();
        let script = self.scripts.write_lease.clone();

        let res: i64 = self
            .run("EVALSHA_WRITE_LEASE", |mut c| async move {
                script
                    .key(d_key)
                    .key(l_key)
                    .arg(tok)
                    .arg(data)
                    .arg(ttl_ms)
                    .invoke_async(&mut c)
                    .await
            })
            .await?;

        Ok(res == 1)
    }
}
