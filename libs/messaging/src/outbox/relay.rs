//! High-Throughput Hybrid Outbox Relay Worker (`relay.rs`).
//!
//! Combines PostgreSQL `LISTEN/NOTIFY` for zero-latency reactive wakeups with
//! `SELECT ... FOR UPDATE SKIP LOCKED` micro-batching and exponential backoff retry.

#[cfg(feature = "postgres")]
use std::{collections::HashMap, sync::Arc, time::Instant};

#[cfg(feature = "postgres")]
use omni_database::DbPool;
#[cfg(feature = "postgres")]
use tokio::sync::broadcast;

#[cfg(feature = "postgres")]
use crate::{
    broker::{RawBrokerMessage, publisher::MessagePublisher},
    config::OutboxConfig,
    consumer::backoff::BackoffPolicy,
    error::MessagingError,
    instrument::{
        record_outbox_failed, record_outbox_publish_duration, record_outbox_published,
        record_outbox_queue_depth, record_outbox_retry,
    },
    outbox::{listener::setup_listener, repository::OutboxRepository},
};

/// Quét và gửi một mẻ bản tin Outbox sang Message Broker.
#[cfg(feature = "postgres")]
pub async fn dispatch_batch(
    pool: &DbPool,
    broker: &dyn MessagePublisher,
    config: &OutboxConfig,
) -> Result<usize, MessagingError> {
    let mut conn = pool
        .pool()
        .acquire()
        .await
        .map_err(|e| MessagingError::DatabaseError(omni_database::DbError::from(e)))?;

    // Cập nhật chỉ số độ sâu hàng đợi Outbox
    if let Ok(pending) = OutboxRepository::count_pending(&mut conn).await {
        record_outbox_queue_depth(pending as f64);
    }

    // Khóa và lấy lô bản tin PENDING bằng FOR UPDATE SKIP LOCKED
    let records = OutboxRepository::fetch_and_lock_pending(
        &mut conn,
        config.batch_size,
        config.lock_duration,
    )
    .await
    .map_err(MessagingError::from)?;

    if records.is_empty() {
        return Ok(0);
    }

    let count = records.len();
    let backoff = BackoffPolicy::default();

    for record in records {
        let topic = &record.destination_topic;
        let mut headers = HashMap::new();

        if let Some(headers_obj) = record.headers.as_object() {
            for (k, v) in headers_obj {
                if let Some(s) = v.as_str() {
                    headers.insert(k.clone(), s.to_string());
                } else {
                    headers.insert(k.clone(), v.to_string());
                }
            }
        }

        // Tự động bảo đảm ce-id và ce-type luôn có trong headers cho Binary Mode
        headers
            .entry("ce-id".to_string())
            .or_insert_with(|| record.id.to_string());
        headers
            .entry("ce-type".to_string())
            .or_insert_with(|| record.event_type.clone());

        let payload_bytes = serde_json::to_vec(&record.payload)
            .map_err(|e| MessagingError::SerializationFailed(e.to_string()))?;

        let message = RawBrokerMessage {
            topic: topic.clone(),
            partition_key: record.partition_key.clone(),
            payload: payload_bytes,
            headers,
        };

        let start_time = Instant::now();
        match broker.publish(message).await {
            Ok(()) => {
                let duration = start_time.elapsed();
                record_outbox_publish_duration(topic, duration);
                record_outbox_published(topic);

                // Đánh dấu thành công theo chiến lược dọn dẹp
                OutboxRepository::mark_published(&mut conn, &[record.id], config.cleanup_strategy)
                    .await
                    .map_err(MessagingError::from)?;
            }
            Err(e) => {
                tracing::warn!(
                    outbox_id = %record.id,
                    topic = %topic,
                    error = %e,
                    "Failed to publish outbox record to broker"
                );
                record_outbox_retry(topic);

                if record.retry_count + 1 >= record.max_retries {
                    record_outbox_failed(topic);
                    OutboxRepository::mark_dead(&mut conn, record.id, &e.to_string())
                        .await
                        .map_err(MessagingError::from)?;
                } else {
                    let delay = backoff.calculate_delay((record.retry_count + 1) as u32);
                    OutboxRepository::mark_failed(&mut conn, record.id, &e.to_string(), delay)
                        .await
                        .map_err(MessagingError::from)?;
                }
            }
        }
    }

    Ok(count)
}

/// Chạy vòng lặp nền Outbox Relay kết hợp Ticker và PostgreSQL `LISTEN/NOTIFY`.
#[cfg(feature = "postgres")]
pub async fn run_relay_loop(
    pool: DbPool,
    broker: Arc<dyn MessagePublisher>,
    config: OutboxConfig,
    mut shutdown: broadcast::Receiver<()>,
) {
    let mut ticker = tokio::time::interval(config.poll_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut listener = match setup_listener(&pool, &config.notify_channel).await {
        Ok(l) => Some(l),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Failed to establish PgListener for Outbox notifications. Falling back to pure polling."
            );
            None
        }
    };

    tracing::info!(
        poll_interval_ms = config.poll_interval.as_millis(),
        batch_size = config.batch_size,
        "Outbox Relay worker started"
    );

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                tracing::info!("Outbox relay received shutdown signal. Exiting loop.");
                break;
            }
            _ = ticker.tick() => {
                if let Err(e) = dispatch_batch(&pool, broker.as_ref(), &config).await {
                    tracing::error!("Error dispatching outbox batch on tick: {e:?}");
                }
            }
            notification = async {
                match listener.as_mut() {
                    Some(l) => l.recv().await,
                    None => futures::future::pending().await,
                }
            } => {
                match notification {
                    Ok(_notif) => {
                        // Thức tỉnh ngay tức thì khi có transaction vừa commit event mới!
                        if let Err(e) = dispatch_batch(&pool, broker.as_ref(), &config).await {
                            tracing::error!("Error dispatching outbox batch on notification: {e:?}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("PgListener connection lost: {e}. Worker will continue on ticker.");
                    }
                }
            }
        }
    }
}
