//! Idempotent Inbox Engine Module (`inbox`).
//!
//! Provides the dual-layer deduplication defense mechanism:
//! - Sub-millisecond Redis fast-path caching (`omni-cache`)
//! - ACID PostgreSQL `inbox_events` strong consistency (`omni-database`)

pub mod deduplicator;
pub mod model;
pub mod repository;

pub use deduplicator::DualLayerDeduplicator;
pub use model::InboxRecord;
pub use repository::{INBOX_TABLE_DDL, InboxRepository};
