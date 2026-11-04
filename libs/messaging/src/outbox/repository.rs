//! Transactional Outbox Database Repository (`repository.rs`).
//!
//! Provides atomic event recording within [`UnitOfWork`], high-throughput polling via
//! `SELECT ... FOR UPDATE SKIP LOCKED`, and lifecycle state transitions.

#[cfg(feature = "postgres")]
use std::time::Duration;

#[cfg(feature = "postgres")]
use omni_database::{DbError, transaction::UnitOfWork};
use serde::Serialize;
#[cfg(feature = "postgres")]
use sqlx::{PgConnection, Row as _};
use uuid::Uuid;

use crate::{
    cloudevent::CloudEvent,
    config::CleanupStrategy,
    outbox::model::{OutboxRecord, OutboxStatus},
};

/// DDL khởi tạo schema bảng `outbox_events` và partial indexes tối ưu SKIP LOCKED.
pub const OUTBOX_TABLE_DDL: &str = r"
CREATE TABLE IF NOT EXISTS outbox_events (
    id UUID PRIMARY KEY,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(128) NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    destination_topic VARCHAR(256) NOT NULL,
    partition_key VARCHAR(128),
    payload JSONB NOT NULL,
    headers JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    retry_count INT NOT NULL DEFAULT 0,
    max_retries INT NOT NULL DEFAULT 5,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    locked_until TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_outbox_pending_relay
ON outbox_events (next_retry_at, created_at)
WHERE status = 'PENDING';

CREATE INDEX IF NOT EXISTS idx_outbox_aggregate
ON outbox_events (aggregate_type, aggregate_id);

CREATE INDEX IF NOT EXISTS idx_outbox_cleanup
ON outbox_events (published_at)
WHERE status = 'PUBLISHED';
";

/// Repository quản lý vòng đời bản ghi `outbox_events`.
#[derive(Debug)]
pub struct OutboxRepository;

impl OutboxRepository {
    /// Khởi tạo bảng `outbox_events` và chỉ mục nếu chưa tồn tại.
    #[cfg(feature = "postgres")]
    pub async fn create_tables(conn: &mut PgConnection) -> Result<(), DbError> {
        sqlx::raw_sql(OUTBOX_TABLE_DDL)
            .execute(conn)
            .await
            .map_err(DbError::from)?;
        Ok(())
    }

    /// Ghi nhận CloudEvent vào bảng `outbox_events` bên trong [`UnitOfWork`] hiện tại.
    #[cfg(feature = "postgres")]
    pub async fn record<T: Serialize>(
        uow: &mut UnitOfWork,
        topic: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        event: CloudEvent<T>,
    ) -> Result<Uuid, DbError> {
        Self::record_with_conn(uow.conn(), topic, aggregate_type, aggregate_id, event).await
    }

    /// Ghi nhận CloudEvent trực tiếp trên một kết nối PostgreSQL đang mở.
    #[cfg(feature = "postgres")]
    pub async fn record_with_conn<T: Serialize>(
        conn: &mut PgConnection,
        topic: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        event: CloudEvent<T>,
    ) -> Result<Uuid, DbError> {
        let topic = topic.into();
        let aggregate_type = aggregate_type.into();
        let aggregate_id = aggregate_id.into();
        let partition_key = event.extensions.partition_key.clone();

        let payload_json = serde_json::to_value(&event)
            .map_err(|e| DbError::Configuration(format!("Serialization error: {e}")))?;
        let headers_json = serde_json::to_value(&event.extensions)
            .map_err(|e| DbError::Configuration(format!("Serialization error: {e}")))?;

        let event_id = event.id;

        sqlx::query(
            r"
            INSERT INTO outbox_events (
                id, aggregate_type, aggregate_id, event_type,
                destination_topic, partition_key, payload, headers,
                status, created_at, next_retry_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'PENDING', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ",
        )
        .bind(event_id)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&event.event_type)
        .bind(topic)
        .bind(partition_key)
        .bind(payload_json)
        .bind(headers_json)
        .execute(conn)
        .await
        .map_err(DbError::from)?;

        Ok(event_id)
    }

    /// Khóa và lấy một mẻ bản tin `PENDING` sử dụng `SELECT ... FOR UPDATE SKIP LOCKED`.
    #[cfg(feature = "postgres")]
    pub async fn fetch_and_lock_pending(
        conn: &mut PgConnection,
        batch_size: i64,
        lock_duration: Duration,
    ) -> Result<Vec<OutboxRecord>, DbError> {
        let lock_ms = lock_duration.as_millis() as i64;

        let query = r"
            WITH cte AS (
                SELECT id FROM outbox_events
                WHERE (status = 'PENDING' OR (status = 'PROCESSING' AND locked_until < CURRENT_TIMESTAMP))
                  AND next_retry_at <= CURRENT_TIMESTAMP
                ORDER BY created_at ASC
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE outbox_events o
            SET status = 'PROCESSING',
                locked_until = CURRENT_TIMESTAMP + ($2 * INTERVAL '1 millisecond')
            FROM cte
            WHERE o.id = cte.id
            RETURNING o.id, o.aggregate_type, o.aggregate_id, o.event_type,
                      o.destination_topic, o.partition_key, o.payload, o.headers,
                      o.status, o.retry_count, o.max_retries, o.next_retry_at,
                      o.locked_until, o.last_error, o.created_at, o.published_at
        ";

        let rows = sqlx::query(query)
            .bind(batch_size)
            .bind(lock_ms)
            .fetch_all(conn)
            .await
            .map_err(DbError::from)?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let status_str: String = row.try_get("status").map_err(DbError::from)?;
            let record = OutboxRecord {
                id: row.try_get("id").map_err(DbError::from)?,
                aggregate_type: row.try_get("aggregate_type").map_err(DbError::from)?,
                aggregate_id: row.try_get("aggregate_id").map_err(DbError::from)?,
                event_type: row.try_get("event_type").map_err(DbError::from)?,
                destination_topic: row.try_get("destination_topic").map_err(DbError::from)?,
                partition_key: row.try_get("partition_key").map_err(DbError::from)?,
                payload: row.try_get("payload").map_err(DbError::from)?,
                headers: row.try_get("headers").map_err(DbError::from)?,
                status: OutboxStatus::from(status_str.as_str()),
                retry_count: row.try_get("retry_count").map_err(DbError::from)?,
                max_retries: row.try_get("max_retries").map_err(DbError::from)?,
                next_retry_at: row.try_get("next_retry_at").map_err(DbError::from)?,
                locked_until: row.try_get("locked_until").map_err(DbError::from)?,
                last_error: row.try_get("last_error").map_err(DbError::from)?,
                created_at: row.try_get("created_at").map_err(DbError::from)?,
                published_at: row.try_get("published_at").map_err(DbError::from)?,
            };
            records.push(record);
        }

        Ok(records)
    }

    /// Đánh dấu danh sách bản tin đã xuất bản thành công theo chiến lược dọn dẹp chỉ định.
    #[cfg(feature = "postgres")]
    pub async fn mark_published(
        conn: &mut PgConnection,
        ids: &[Uuid],
        strategy: CleanupStrategy,
    ) -> Result<(), DbError> {
        if ids.is_empty() {
            return Ok(());
        }

        match strategy {
            CleanupStrategy::HardDelete => {
                sqlx::query("DELETE FROM outbox_events WHERE id = ANY($1)")
                    .bind(ids)
                    .execute(conn)
                    .await
                    .map_err(DbError::from)?;
            }
            CleanupStrategy::SoftDelete | CleanupStrategy::RetentionPruning => {
                sqlx::query(
                    r"
                    UPDATE outbox_events
                    SET status = 'PUBLISHED',
                        published_at = CURRENT_TIMESTAMP,
                        locked_until = NULL
                    WHERE id = ANY($1)
                    ",
                )
                .bind(ids)
                .execute(conn)
                .await
                .map_err(DbError::from)?;
            }
        }

        Ok(())
    }

    /// Đánh dấu một bản tin gửi thất bại và lên lịch thử lại tiếp theo.
    #[cfg(feature = "postgres")]
    pub async fn mark_failed(
        conn: &mut PgConnection,
        id: Uuid,
        error_msg: &str,
        retry_delay: Duration,
    ) -> Result<(), DbError> {
        let delay_ms = retry_delay.as_millis() as i64;
        sqlx::query(
            r"
            UPDATE outbox_events
            SET status = 'PENDING',
                retry_count = retry_count + 1,
                next_retry_at = CURRENT_TIMESTAMP + ($1 * INTERVAL '1 millisecond'),
                locked_until = NULL,
                last_error = $2
            WHERE id = $3
            ",
        )
        .bind(delay_ms)
        .bind(error_msg)
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    /// Đánh dấu một bản tin là `DEAD` khi đã vượt quá số lần thử lại tối đa.
    #[cfg(feature = "postgres")]
    pub async fn mark_dead(
        conn: &mut PgConnection,
        id: Uuid,
        error_msg: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            r"
            UPDATE outbox_events
            SET status = 'DEAD',
                locked_until = NULL,
                last_error = $1
            WHERE id = $2
            ",
        )
        .bind(error_msg)
        .bind(id)
        .execute(conn)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    /// Đếm số lượng bản tin đang ở trạng thái `PENDING`.
    #[cfg(feature = "postgres")]
    pub async fn count_pending(conn: &mut PgConnection) -> Result<i64, DbError> {
        let row =
            sqlx::query("SELECT count(*) as count FROM outbox_events WHERE status = 'PENDING'")
                .fetch_one(conn)
                .await
                .map_err(DbError::from)?;

        let count: i64 = row.try_get("count").map_err(DbError::from)?;
        Ok(count)
    }

    /// Dọn dẹp các bản ghi Outbox đã gửi thành công vượt quá thời hạn retention theo mẻ.
    #[cfg(feature = "postgres")]
    pub async fn prune_retention(
        conn: &mut PgConnection,
        retention_duration: Duration,
        limit: i64,
    ) -> Result<u64, DbError> {
        let retention_secs = retention_duration.as_secs() as i64;
        let query = r"
            DELETE FROM outbox_events
            WHERE id IN (
                SELECT id FROM outbox_events
                WHERE status = 'PUBLISHED'
                  AND published_at < CURRENT_TIMESTAMP - ($1 * INTERVAL '1 second')
                LIMIT $2
            )
        ";

        let result = sqlx::query(query)
            .bind(retention_secs)
            .bind(limit)
            .execute(conn)
            .await
            .map_err(DbError::from)?;

        Ok(result.rows_affected())
    }
}
