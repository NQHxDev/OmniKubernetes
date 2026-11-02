//! Transactional Outbox Engine Module (`outbox`).
//!
//! Provides the core Transactional Outbox Pattern implementation:
//! - [`OutboxRepository`]: Atomic insertion within [`omni_database::transaction::UnitOfWork`].
//! - [`OutboxRecord`]: State machine tracking (`PENDING`, `PROCESSING`, `PUBLISHED`, `FAILED`, `DEAD`).
//! - [`relay::run_relay_loop`]: Hybrid relay combining `LISTEN/NOTIFY` with `FOR UPDATE SKIP LOCKED`.
//! - [`cleanup::OutboxRetentionWorker`]: Chunked retention pruning to prevent table bloat.

pub mod cleanup;
pub mod listener;
pub mod model;
pub mod relay;
pub mod repository;

#[cfg(feature = "postgres")]
pub use cleanup::OutboxRetentionWorker;
pub use model::{OutboxRecord, OutboxStatus};
#[cfg(feature = "postgres")]
pub use relay::{dispatch_batch, run_relay_loop};
pub use repository::{OUTBOX_TABLE_DDL, OutboxRepository};
