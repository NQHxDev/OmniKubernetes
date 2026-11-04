//! Background Pool Sentinel for periodic keepalive and health metrics.
//!
//! Executes asynchronous pings, tracks active/idle connection gauges,
//! and updates atomic health state for Kubernetes probes.

use std::time::Duration;

use crate::pool::DbPool;

/// Khởi chạy tác vụ nền Pool Sentinel giám sát sức khỏe kết nối PostgreSQL.
///
/// Tác vụ tự động kết thúc an toàn khi connection pool nhận sự kiện đóng (`close_event`).
pub fn spawn_pool_sentinel(db: DbPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval_secs = db.settings().raw.sentinel.interval_secs;
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let close = db.pool().close_event();
        tokio::pin!(close);

        tracing::debug!(
            target: "omni_database::sentinel",
            interval_secs = interval_secs,
            "PostgreSQL pool sentinel started"
        );

        loop {
            tokio::select! {
                () = &mut close => {
                    tracing::debug!(target: "omni_database::sentinel", "pool close event received, terminating sentinel");
                    break;
                }
                _ = ticker.tick() => {
                    db.sentinel_tick().await;
                }
            }
        }

        tracing::debug!(target: "omni_database::sentinel", "PostgreSQL pool sentinel stopped");
    })
}
