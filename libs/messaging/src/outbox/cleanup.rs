//! Outbox Table Retention & Bloat Prevention Worker (`cleanup.rs`).
//!
//! Provides chunked background deletion of published outbox records older than
//! retention thresholds to prevent PostgreSQL table bloat.

#[cfg(feature = "postgres")]
use std::time::Duration;

#[cfg(feature = "postgres")]
use omni_database::DbPool;
#[cfg(feature = "postgres")]
use tokio::sync::broadcast;

#[cfg(feature = "postgres")]
use crate::outbox::repository::OutboxRepository;

/// Worker chạy nền dọn dẹp các bản ghi Outbox và Inbox cũ định kỳ.
#[cfg(feature = "postgres")]
#[derive(Debug)]
pub struct OutboxRetentionWorker;

#[cfg(feature = "postgres")]
impl OutboxRetentionWorker {
    /// Khởi chạy tiến trình dọn dẹp định kỳ cho tới khi nhận tín hiệu shutdown.
    pub async fn run(
        pool: DbPool,
        retention_hours: i64,
        check_interval: Duration,
        mut shutdown: broadcast::Receiver<()>,
    ) {
        let mut interval = tokio::time::interval(check_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let retention_duration = Duration::from_secs((retention_hours.max(1) * 3600) as u64);
        let batch_delete_limit = 5000;

        tracing::info!(
            retention_hours = retention_hours,
            check_interval_secs = check_interval.as_secs(),
            "Outbox retention housekeeping worker started"
        );

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Outbox retention worker received shutdown signal. Exiting.");
                    break;
                }
                _ = interval.tick() => {
                    match pool.pool().acquire().await {
                        Ok(mut conn) => {
                            match OutboxRepository::prune_retention(
                                &mut conn,
                                retention_duration,
                                batch_delete_limit,
                            )
                            .await
                            {
                                Ok(pruned) if pruned > 0 => {
                                    tracing::info!(
                                        pruned_count = pruned,
                                        "Pruned published outbox records"
                                    );
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Failed to prune outbox records"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to acquire connection for retention prune: {e}");
                        }
                    }
                }
            }
        }
    }
}
