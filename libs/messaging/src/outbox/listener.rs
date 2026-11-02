//! PostgreSQL LISTEN/NOTIFY Listener Helper (`listener.rs`).
//!
//! Provides the listener client for reactive wakeup when transactions commit new outbox events.

#[cfg(feature = "postgres")]
use omni_database::{DbError, DbPool};
#[cfg(feature = "postgres")]
use sqlx::postgres::PgListener;

/// Khởi tạo và đăng ký lắng nghe kênh PostgreSQL `LISTEN/NOTIFY`.
#[cfg(feature = "postgres")]
pub async fn setup_listener(pool: &DbPool, channel_name: &str) -> Result<PgListener, DbError> {
    let mut listener = PgListener::connect_with(pool.pool())
        .await
        .map_err(DbError::from)?;

    listener.listen(channel_name).await.map_err(DbError::from)?;

    tracing::info!(
        channel = %channel_name,
        "Established PgListener for outbox reactive wakeup"
    );

    Ok(listener)
}
