//! Database Migration Runner and Schema Verification.
//!
//! Supports three execution modes:
//! - `Disabled`: Skips migration completely (e.g., read-only pods, local testing).
//! - `Apply`: Runs platform schema setup and applies all pending service migrations.
//! - `Verify`: Validates that all migrations defined in code have already been applied,
//!   rejecting pod startup if migrations are missing (Production standard).

use std::collections::HashSet;

use omni_common::config::MigrationMode;
use sqlx::migrate::Migrator;
use sqlx::{Connection as _, PgConnection, Row as _};

use crate::error::DbError;
use crate::pool::DbPool;

/// DDL khởi tạo schema và trigger kiểm toán nền tảng của `OmniKubernetes`.
const PLATFORM_MIGRATIONS_DDL: &str = r"
CREATE TABLE IF NOT EXISTS _omni_platform_migrations (
    id SERIAL PRIMARY KEY,
    version VARCHAR(64) NOT NULL UNIQUE,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE OR REPLACE FUNCTION omni_set_audit_timestamps()
RETURNS TRIGGER AS $$
BEGIN
    IF (TG_OP = 'INSERT') THEN
        NEW.created_at = COALESCE(NEW.created_at, CURRENT_TIMESTAMP);
        IF NEW.created_by IS NULL THEN
            BEGIN
                NEW.created_by = NULLIF(current_setting('omni.actor_id', true), '')::UUID;
            EXCEPTION WHEN OTHERS THEN
                NEW.created_by = NULL;
            END IF;
        END IF;
        NEW.updated_at = NEW.created_at;
        NEW.updated_by = NEW.created_by;
        NEW.version = 1;
    ELSIF (TG_OP = 'UPDATE') THEN
        NEW.updated_at = CURRENT_TIMESTAMP;
        BEGIN
            NEW.updated_by = NULLIF(current_setting('omni.actor_id', true), '')::UUID;
        EXCEPTION WHEN OTHERS THEN
            NEW.updated_by = OLD.updated_by;
        END;
        NEW.version = OLD.version + 1;
        NEW.created_at = OLD.created_at;
        NEW.created_by = OLD.created_by;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
";

/// Thực thi hoặc xác minh Database Migrations theo cấu hình [`MigrationMode`].
///
/// Sử dụng kết nối trực tiếp đến PostgreSQL (`migration_connect_options`) bỏ qua `PgBouncer`
/// để hỗ trợ đầy đủ Advisory Lock ở cấp phiên.
pub async fn run_migrations(db: &DbPool, migrator: &Migrator) -> Result<(), DbError> {
    let mode = db.settings().raw.effective_migration_mode();

    match mode {
        MigrationMode::Disabled => {
            tracing::info!(
                mode = "disabled",
                "database migrations are disabled in configuration, skipping execution"
            );
            Ok(())
        }
        MigrationMode::Apply => {
            tracing::info!(
                mode = "apply",
                "applying platform and service database migrations"
            );
            let mut conn = PgConnection::connect_with(&db.settings().migration_connect_options())
                .await
                .map_err(DbError::from)?;

            // 1. Áp dụng platform migrations DDL
            sqlx::raw_sql(PLATFORM_MIGRATIONS_DDL)
                .execute(&mut conn)
                .await
                .map_err(DbError::from)?;

            // 2. Thực thi migrations của microservice
            migrator.run(&mut conn).await.map_err(DbError::from)?;

            tracing::info!("database migrations applied successfully");
            Ok(())
        }
        MigrationMode::Verify => {
            tracing::info!(
                mode = "verify",
                "verifying database schema migrations status against code"
            );
            let mut conn = PgConnection::connect_with(&db.settings().migration_connect_options())
                .await
                .map_err(DbError::from)?;

            let applied_rows = match sqlx::query(
                "SELECT version FROM _sqlx_migrations WHERE success = TRUE",
            )
            .fetch_all(&mut conn)
            .await
            {
                Ok(rows) => rows,
                Err(err) => {
                    if let sqlx::Error::Database(ref db_err) = err
                        && db_err.code().as_deref() == Some("42P01")
                    {
                        // undefined_table: bảng migrations chưa hề tồn tại
                        return Err(DbError::Configuration(
                            "Database migration table `_sqlx_migrations` does not exist. Run migrations via Job before starting service.".to_string(),
                        ));
                    }
                    return Err(DbError::from(err));
                }
            };

            let applied_versions: HashSet<i64> = applied_rows
                .into_iter()
                .filter_map(|row| row.try_get::<i64, _>("version").ok())
                .collect();

            let mut missing = Vec::new();
            for migration in migrator.iter() {
                if !applied_versions.contains(&migration.version) {
                    missing.push(format!("{}_{}", migration.version, migration.description));
                }
            }

            if !missing.is_empty() {
                let err_msg = format!(
                    "Database schema verification failed: {} pending migration(s) not yet applied: [{}]. Pod startup rejected in verify mode.",
                    missing.len(),
                    missing.join(", ")
                );
                tracing::error!(error = %err_msg, "unapplied migrations detected");
                return Err(DbError::Configuration(err_msg));
            }

            tracing::info!("all database migrations are verified and up-to-date");
            Ok(())
        }
    }
}
