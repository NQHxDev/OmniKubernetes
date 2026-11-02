//! `OmniKubernetes` Enterprise Relational Persistence Foundation (`omni-database`).
//!
//! Provides a production-grade, compile-time verified, observable persistence layer
//! on top of PostgreSQL and `SQLx`:
//! - **Enterprise `PgPool` Factory:** Bounded backoff retry, keepalive ping, and graceful drain.
//! - **Kubernetes Pool Sentinel:** Cached readiness health reporting without probe query thundering herd.
//! - **Zero-Round-Trip Unit of Work:** Multi-statement `BEGIN` with session context injection.
//! - **Strict SQLSTATE Error Taxonomy:** Maps PostgreSQL errors to RFC 9457 `AppError` without leaking database internals.
//! - **Telemetry & OpenTelemetry Semantic Conventions:** Metrics histograms, tracing spans, and slow query logging.
//! - **Entity Auditing & Optimistic Concurrency:** Standard audit triggers and `Id<T>` `UUIDv7` types.
//! - **Keyset Cursor Pagination:** High-performance pagination bridge to `CursorPaginationMeta`.
//! - **Database Migrations:** Schema apply and verification runners.

pub mod audit;
pub mod commenter;
pub mod config;
pub mod error;
pub mod health;
pub mod id;
pub mod instrument;
pub mod migrate;
pub mod pagination;
pub mod pool;
pub mod sentinel;
pub mod session;
pub mod transaction;

pub use audit::{AuditColumns, check_optimistic_lock};
pub use commenter::{append_sqlcomment, format_sqlcomment};
pub use config::DatabaseSettings;
pub use error::{DbError, Retryable, SqlState};
#[cfg(feature = "probes")]
pub use health::PgHealthIndicator;
pub use health::PoolHealthState;
pub use id::Id;
pub use instrument::{
    DB_SYSTEM_NAME, DEFAULT_SLOW_QUERY_THRESHOLD_MS, DbFutureExt, DbOperation,
    METRIC_DB_CLIENT_CONNECTIONS_MAX, METRIC_DB_CLIENT_CONNECTIONS_MIN,
    METRIC_DB_CLIENT_CONNECTIONS_USAGE, METRIC_DB_CLIENT_ERRORS_TOTAL,
    METRIC_DB_CLIENT_OPERATION_DURATION_SECONDS, METRIC_DB_CLIENT_TRANSACTION_DURATION_SECONDS,
    METRIC_DB_CLIENT_TRANSACTION_RETRIES_TOTAL, METRIC_DB_CLIENT_TRANSACTIONS_TOTAL,
};
pub use migrate::run_migrations;
pub use pagination::{CursorError, KeysetCursor, paginate_keyset};
pub use pool::DbPool;
pub use sentinel::spawn_pool_sentinel;
pub use session::{Actor, SessionContext, SpanId, TraceId, build_begin_statement};
pub use transaction::{
    IsolationLevel, RetryPolicy, TxOptions, TxOutcome, UnitOfWork, with_retryable_transaction,
    with_transaction,
};
