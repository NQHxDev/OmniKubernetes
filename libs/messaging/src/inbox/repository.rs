//! Transactional Inbox Database Repository (`repository.rs`).
//!
//! Provides DDL schema initialization, atomic `ON CONFLICT DO NOTHING` idempotency recording,
//! and batch retention pruning on PostgreSQL.

#[cfg(feature = "postgres")]
use std::time::Duration;

#[cfg(feature = "postgres")]
use omni_database::DbError;
#[cfg(feature = "postgres")]
use sqlx::PgConnection;
#[cfg(feature = "postgres")]
use uuid::Uuid;

/// DDL khởi tạo schema bảng `inbox_events` và chỉ mục dọn dẹp.
pub const INBOX_TABLE_DDL: &str = r"
CREATE TABLE IF NOT EXISTS inbox_events (
    id UUID PRIMARY KEY,
    event_type VARCHAR(128) NOT NULL,
    source VARCHAR(256) NOT NULL,
    consumer_group VARCHAR(128) NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(20) NOT NULL DEFAULT 'SUCCESS',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_inbox_cleanup
ON inbox_events (created_at);
";

/// Repository quản lý vòng đời bảng `inbox_events`.
#[derive(Debug)]
pub struct InboxRepository;

impl InboxRepository {
    /// Khởi tạo bảng `inbox_events` và chỉ mục nếu chưa tồn tại.
    #[cfg(feature = "postgres")]
    pub async fn create_tables(conn: &mut PgConnection) -> Result<(), DbError> {
        sqlx::raw_sql(INBOX_TABLE_DDL)
            .execute(conn)
            .await
            .map_err(DbError::from)?;
        Ok(())
    }

    /// Ghi nhận bản tin vào `inbox_events` nếu chưa tồn tại.
    ///
    /// Trả về `Ok(true)` nếu bản tin mới được ghi thành công.
    /// Trả về `Ok(false)` nếu bản tin đã tồn tại (phát hiện Duplicate Message).
    #[cfg(feature = "postgres")]
    pub async fn record_if_not_exists(
        conn: &mut PgConnection,
        id: Uuid,
        event_type: &str,
        source: &str,
        consumer_group: &str,
    ) -> Result<bool, DbError> {
        let result = sqlx::query(
            r"
            INSERT INTO inbox_events (
                id, event_type, source, consumer_group,
                processed_at, status, created_at
            )
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP, 'SUCCESS', CURRENT_TIMESTAMP)
            ON CONFLICT (id) DO NOTHING
            ",
        )
        .bind(id)
        .bind(event_type)
        .bind(source)
        .bind(consumer_group)
        .execute(conn)
        .await
        .map_err(DbError::from)?;

        Ok(result.rows_affected() > 0)
    }

    /// Dọn dẹp các bản ghi inbox cũ vượt quá khoảng thời gian retention theo mẻ.
    #[cfg(feature = "postgres")]
    pub async fn prune_retention(
        conn: &mut PgConnection,
        retention_duration: Duration,
        limit: i64,
    ) -> Result<u64, DbError> {
        let retention_secs = retention_duration.as_secs() as i64;
        let query = r"
            DELETE FROM inbox_events
            WHERE id IN (
                SELECT id FROM inbox_events
                WHERE created_at < CURRENT_TIMESTAMP - ($1 * INTERVAL '1 second')
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
