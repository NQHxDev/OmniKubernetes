//! Distributed Redis Sliding Window Rate Limiter implementation.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use omni_common::resilience::{
    DistributedRateLimiter, REDIS_SLIDING_WINDOW_LUA, RateLimitDecision, RateLimitKey,
};
use redis::{Script, aio::ConnectionManager};

/// Distributed rate limiter backed by Redis using an atomic sliding window Lua script.
#[derive(Clone)]
pub struct RedisRateLimiter {
    connection: ConnectionManager,
    script: Script,
    key_prefix: String,
}

impl std::fmt::Debug for RedisRateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisRateLimiter")
            .field("key_prefix", &self.key_prefix)
            .finish_non_exhaustive()
    }
}

impl RedisRateLimiter {
    /// Creates a new `RedisRateLimiter` with the given connection manager and key prefix.
    #[must_use]
    pub fn new(connection: ConnectionManager, key_prefix: impl Into<String>) -> Self {
        Self {
            connection,
            script: Script::new(REDIS_SLIDING_WINDOW_LUA),
            key_prefix: key_prefix.into(),
        }
    }
}

#[async_trait]
impl DistributedRateLimiter for RedisRateLimiter {
    async fn check_distributed(
        &self,
        key: &RateLimitKey,
        limit: u64,
        window: Duration,
    ) -> Result<RateLimitDecision, String> {
        let now_ms = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        )
        .unwrap_or(u64::MAX);

        let window_secs = window.as_secs().max(1);
        let window_ms = window_secs * 1000;
        let current_window_id = now_ms / window_ms;
        let prev_window_id = current_window_id.saturating_sub(1);

        let curr_key = format!(
            "{}:rl:{}:{}",
            self.key_prefix,
            key.as_key_str(),
            current_window_id
        );
        let prev_key = format!(
            "{}:rl:{}:{}",
            self.key_prefix,
            key.as_key_str(),
            prev_window_id
        );

        let mut conn = self.connection.clone();
        let result: (i64, i64, i64) = self
            .script
            .key(&curr_key)
            .key(&prev_key)
            .arg(limit)
            .arg(window_secs)
            .arg(now_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| format!("Redis sliding window evaluation failed: {e}"))?;

        let allowed = result.0 == 1;
        let remaining = if allowed {
            result.1.max(0).cast_unsigned()
        } else {
            0
        };
        let reset_secs = result.2.max(1).cast_unsigned();

        Ok(RateLimitDecision {
            allowed,
            limit,
            remaining,
            reset_secs,
            retry_after_secs: if allowed { 0 } else { reset_secs },
        })
    }
}
