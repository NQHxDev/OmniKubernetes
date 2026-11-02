//! Integration tests against live PostgreSQL instance.
//!
//! Gated by the `OMNI_TEST_DATABASE_URL` environment variable.
//! When not provided, these integration tests skip cleanly.

use std::env;

use omni_database::{Actor, DbPool, SessionContext, SpanId, TraceId, TxOptions, with_transaction};
use secrecy::SecretString;

#[tokio::test]
async fn test_live_postgres_connectivity_and_transactions() {
    let db_url = match env::var("OMNI_TEST_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!("SKIPPING live PostgreSQL test: OMNI_TEST_DATABASE_URL is not set.");
            return;
        }
    };

    let mut config = omni_common::config::DatabaseConfig {
        enabled: true,
        ..Default::default()
    };

    // Parse URL manually or via simple string slicing
    if let Some(rest) = db_url
        .strip_prefix("postgres://")
        .or_else(|| db_url.strip_prefix("postgresql://"))
        && let Some((user_pass, host_db)) = rest.split_once('@')
    {
        if let Some((user, pass)) = user_pass.split_once(':') {
            config.username = user.to_string();
            config.password = SecretString::from(pass);
        } else {
            config.username = user_pass.to_string();
        }

        if let Some((host_port, db_part)) = host_db.split_once('/') {
            if let Some((host, port_str)) = host_port.split_once(':') {
                config.host = host.to_string();
                config.port = port_str.parse().unwrap_or(5432);
            } else {
                config.host = host_port.to_string();
            }

            let db_name = db_part.split('?').next().unwrap_or("postgres");
            config.database_name = db_name.to_string();
        }
    }

    let pool = DbPool::create_pool_from_config(config, "integration-test")
        .await
        .expect("connection pool to test DB must succeed");

    let session = SessionContext {
        trace_id: TraceId::parse("4bf92f3577b34da6a3ce929d0e0e4736"),
        span_id: SpanId::parse("00f067aa0ba902b7"),
        actor: Actor::service("integration-test"),
        tenant_id: None,
    };

    // 1. Test ping through sentinel tick
    pool.sentinel_tick().await;
    assert!(pool.health().evaluate(false).is_ok());

    // 2. Test Transaction & rollback on error
    let res: Result<(), omni_database::DbError> =
        with_transaction(&pool, &session, &TxOptions::read_write(), |conn| {
            Box::pin(async move {
                sqlx::query("SELECT 1")
                    .execute(&mut *conn)
                    .await
                    .map_err(omni_database::DbError::from)?;
                // Intentionally fail to trigger rollback
                Err(omni_database::DbError::NotFound)
            })
        })
        .await;

    assert!(matches!(res, Err(omni_database::DbError::NotFound)));

    // 3. Graceful close
    pool.close(std::time::Duration::from_secs(5))
        .await
        .expect("pool close must succeed");
}
