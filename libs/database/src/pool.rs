//! Enterprise PostgreSQL Connection Pool Factory and Lifecycle Manager.
//!
//! Provides bounded exponential backoff on startup, non-retryable error fail-fast,
//! and graceful drain shutdown adhering to the Kubernetes Pod lifecycle.

use std::sync::Arc;
use std::time::Duration;

use omni_common::config::{AppConfig, DatabaseConfig, Validate};
use sqlx::Connection as _;

use crate::config::DatabaseSettings;
use crate::error::DbError;
use crate::health::PoolHealthState;
use crate::instrument::{
    METRIC_DB_CLIENT_CONNECTIONS_MAX, METRIC_DB_CLIENT_CONNECTIONS_MIN,
    METRIC_DB_CLIENT_CONNECTIONS_USAGE,
};
use crate::session::{SessionContext, build_begin_statement};
use crate::transaction::{TxOptions, UnitOfWork};

/// Đối tượng quản lý kết nối cơ sở dữ liệu dùng chung (`omni-database`).
#[derive(Debug, Clone)]
pub struct DbPool {
    pool: sqlx::PgPool,
    settings: Arc<DatabaseSettings>,
    health: Arc<PoolHealthState>,
}

impl DbPool {
    /// Khởi tạo [`DbPool`] từ [`AppConfig<C>`] với cơ chế thử lại có giới hạn (Bounded Backoff).
    ///
    /// # Errors
    /// Trả về [`DbError::Configuration`] nếu database chưa được bật trong cấu hình,
    /// hoặc [`DbError`] nếu kết nối thất bại sau số lần thử lại tối đa.
    pub async fn create_pool<C: Validate>(app_config: &AppConfig<C>) -> Result<Self, DbError> {
        Self::create_pool_from_config(app_config.database.clone(), &app_config.service.name).await
    }

    /// Khởi tạo [`DbPool`] trực tiếp từ [`DatabaseConfig`] và tên microservice.
    pub async fn create_pool_from_config(
        config: DatabaseConfig,
        service_name: &str,
    ) -> Result<Self, DbError> {
        if !config.enabled {
            return Err(DbError::Configuration(
                "Database is disabled in configuration (`database.enabled = false`)".to_string(),
            ));
        }

        let settings = DatabaseSettings::new(config, service_name);
        let max_attempts = settings.raw.connect_retry.max_attempts;
        let initial_backoff_ms = settings.raw.connect_retry.initial_backoff_ms;
        let max_backoff_ms = settings.raw.connect_retry.max_backoff_ms;

        let mut attempt = 1;
        let pool = loop {
            let connect_opts = settings.connect_options();
            let pool_opts = settings.pool_options();

            tracing::debug!(
                attempt = attempt,
                max_attempts = max_attempts,
                db.url = settings.raw.redacted_url(),
                "attempting database connection"
            );

            match pool_opts.connect_with(connect_opts).await {
                Ok(p) => {
                    tracing::info!(
                        attempt = attempt,
                        db.url = settings.raw.redacted_url(),
                        application_name = settings.application_name(),
                        "database connection pool established successfully"
                    );
                    break p;
                }
                Err(e) => {
                    // Kiểm tra các lỗi vĩnh viễn không nên retry
                    let is_permanent = match &e {
                        sqlx::Error::Database(db_err) => {
                            if let Some(code) = db_err.code() {
                                matches!(code.as_ref(), "28P01" | "28000" | "3D000")
                            } else {
                                false
                            }
                        }
                        sqlx::Error::Tls(_) => true,
                        _ => false,
                    };

                    if is_permanent {
                        tracing::error!(
                            error = %e,
                            db.url = settings.raw.redacted_url(),
                            "non-retryable database authentication/configuration error, failing fast"
                        );
                        return Err(DbError::from(e));
                    }

                    if attempt >= max_attempts {
                        tracing::error!(
                            error = %e,
                            attempts = attempt,
                            db.url = settings.raw.redacted_url(),
                            "exhausted all database connection retry attempts"
                        );
                        return Err(DbError::Unavailable(e));
                    }

                    let factor = 2u64.saturating_pow(attempt.saturating_sub(1));
                    let backoff_ms =
                        (initial_backoff_ms.saturating_mul(factor)).min(max_backoff_ms);

                    tracing::warn!(
                        error = %e,
                        attempt = attempt,
                        max_attempts = max_attempts,
                        retry_in_ms = backoff_ms,
                        db.url = settings.raw.redacted_url(),
                        "database connection attempt failed, scheduling backoff retry"
                    );

                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    attempt += 1;
                }
            }
        };

        let health = Arc::new(PoolHealthState::new(
            settings.raw.sentinel.failure_threshold,
            settings.raw.sentinel.interval_secs,
        ));

        Ok(Self {
            pool,
            settings: Arc::new(settings),
            health,
        })
    }

    /// Khởi tạo một giao dịch mới gắn kèm ngữ cảnh phiên [`SessionContext`].
    pub async fn begin(
        &self,
        session: &SessionContext,
        opts: &TxOptions,
    ) -> Result<UnitOfWork, DbError> {
        let sql = build_begin_statement(opts, session, &self.settings);
        let tx = self
            .pool
            .begin_with(sqlx::AssertSqlSafe(sql))
            .await
            .map_err(DbError::from)?;
        Ok(UnitOfWork::new(tx))
    }

    /// Đóng connection pool một cách an toàn kèm thời gian timeout.
    pub async fn close(&self, timeout: Duration) -> Result<(), DbError> {
        tracing::info!("gracefully closing database connection pool");
        let close_fut = self.pool.close();
        tokio::time::timeout(timeout, close_fut)
            .await
            .map_err(|_| {
                DbError::Configuration("database pool graceful close timed out".to_string())
            })?;
        Ok(())
    }

    /// Tham chiếu đến [`sqlx::PgPool`] nền tảng.
    #[must_use]
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Tham chiếu đến thiết lập cấu hình [`DatabaseSettings`].
    #[must_use]
    pub fn settings(&self) -> &DatabaseSettings {
        &self.settings
    }

    /// Tham chiếu đến bộ trạng thái sức khỏe [`PoolHealthState`].
    #[must_use]
    pub fn health(&self) -> &Arc<PoolHealthState> {
        &self.health
    }

    /// Thực thi một chu kỳ kiểm tra nhịp tim (Pool Sentinel Tick).
    pub async fn sentinel_tick(&self) {
        let size = f64::from(self.pool.size());
        let idle = f64::from(u32::try_from(self.pool.num_idle()).unwrap_or(0));
        let used = (size - idle).max(0.0);

        metrics::gauge!(METRIC_DB_CLIENT_CONNECTIONS_USAGE, "state" => "idle").set(idle);
        metrics::gauge!(METRIC_DB_CLIENT_CONNECTIONS_USAGE, "state" => "used").set(used);
        metrics::gauge!(METRIC_DB_CLIENT_CONNECTIONS_MAX)
            .set(f64::from(self.settings.raw.max_connections));
        metrics::gauge!(METRIC_DB_CLIENT_CONNECTIONS_MIN)
            .set(f64::from(self.settings.raw.min_connections));

        let ping_timeout = Duration::from_millis(self.settings.raw.sentinel.ping_timeout_ms);
        match tokio::time::timeout(ping_timeout, self.pool.acquire()).await {
            Ok(Ok(mut conn)) => match tokio::time::timeout(ping_timeout, conn.ping()).await {
                Ok(Ok(())) => {
                    self.health.record_success();
                }
                Ok(Err(e)) => {
                    self.health.record_failure(format!("ping error: {e}"));
                }
                Err(_) => {
                    self.health
                        .record_failure("ping timeout elapsed".to_string());
                }
            },
            Ok(Err(e)) => {
                self.health
                    .record_failure(format!("pool acquire error: {e}"));
            }
            Err(_) => {
                self.health
                    .record_failure("pool acquire timeout elapsed".to_string());
            }
        }
    }
}
