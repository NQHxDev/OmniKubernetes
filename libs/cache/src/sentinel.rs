//! Cache Sentinel Background Health Probe & Bypass Coordinator (`sentinel.rs`).
//!
//! Decoupled background task performing periodic PING probes with hysteresis,
//! managing automatic Bypass mode and replaying the Invalidation Replay Buffer on recovery.

use std::{sync::Arc, time::Duration};

use tokio::sync::watch;

use crate::{
    backend::CacheBackend,
    health::CacheHealthState,
    instrument::{update_redis_bypass_status, update_redis_connection_status},
    invalidation::InvalidationReplayBuffer,
    key::CacheKey,
};

/// Khởi chạy tác vụ nền Sentinel duy nhất của Pod để giám sát Redis.
pub fn spawn_cache_sentinel<B: CacheBackend>(
    backend: Arc<B>,
    health: Arc<CacheHealthState>,
    replay_buffer: Arc<InvalidationReplayBuffer>,
    interval: Duration,
    ping_timeout: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = interval.as_secs(),
            ping_timeout_ms = ping_timeout.as_millis(),
            "Cache Sentinel background loop started"
        );

        let mut ticker = tokio::time::interval(interval);
        // Bỏ qua tick đầu tiên ngay lập tức nếu cần
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("Cache Sentinel received shutdown signal, exiting loop");
                        break;
                    }
                }
                _ = ticker.tick() => {
                    perform_sentinel_check(
                        backend.as_ref(),
                        health.as_ref(),
                        replay_buffer.as_ref(),
                        ping_timeout,
                    ).await;
                }
            }
        }
    })
}

async fn perform_sentinel_check<B: CacheBackend>(
    backend: &B,
    health: &CacheHealthState,
    replay_buffer: &InvalidationReplayBuffer,
    ping_timeout: Duration,
) {
    let ping_res = tokio::time::timeout(ping_timeout, backend.ping()).await;

    match ping_res {
        Ok(Ok(())) => {
            let was_bypassed = health.record_success();
            update_redis_connection_status("cache", true);
            update_redis_bypass_status("cache", false);

            // Nếu vừa phục hồi từ sự cố hoặc buffer có key tồn đọng -> Replay invalidations
            if (was_bypassed || !replay_buffer.is_empty()) && !replay_buffer.is_empty() {
                let keys = replay_buffer.drain_all();
                let keys_ref: Vec<&str> = keys.iter().map(CacheKey::as_str).collect();
                let count = keys_ref.len();

                tracing::info!(
                    count = count,
                    "Replaying deferred cache invalidations after Redis health recovery"
                );
                match backend.del_many(&keys_ref).await {
                    Ok(deleted) => {
                        tracing::info!(
                            deleted = deleted,
                            requested = count,
                            "Deferred cache invalidations replayed successfully"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to replay deferred invalidations, keys will expire via TTL");
                    }
                }
            }
        }
        Ok(Err(e)) => {
            let reason = format!("PING error: {e}");
            health.record_failure(&reason);
            update_redis_connection_status("cache", false);
            update_redis_bypass_status("cache", health.is_bypassed());
        }
        Err(_) => {
            let reason = format!("PING timed out after {}ms", ping_timeout.as_millis());
            health.record_failure(&reason);
            update_redis_connection_status("cache", false);
            update_redis_bypass_status("cache", health.is_bypassed());
        }
    }
}
