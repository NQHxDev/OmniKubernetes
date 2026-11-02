//! # `OmniKubernetes` Event-Driven Messaging & Transactional Outbox Pattern (`omni-messaging`).
//!
//! Provides enterprise event-driven architecture standards adhering to:
//! - **CNCF CloudEvents v1.0.2**: Structured & Binary Content Modes with UUIDv7 identifiers.
//! - **Transactional Outbox Pattern**: Local ACID transactions eliminating dual-write dilemmas.
//! - **Hybrid Relay Worker**: Zero-latency wakeups via `LISTEN/NOTIFY` combined with `SELECT ... FOR UPDATE SKIP LOCKED`.
//! - **Idempotent Dual-Layer Inbox**: Redis fast-path filter paired with PostgreSQL strong consistency.
//! - **Resilient Consumer Pipeline**: Poison pill isolation, Dead Letter Queue (DLQ), and exponential backoff with full jitter.
//! - **W3C Distributed Tracing**: Continuous APM span propagation across network boundaries.

#![allow(
    clippy::doc_markdown,
    clippy::implicit_hasher,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

pub mod broker;
pub mod cloudevent;
pub mod config;
pub mod consumer;
pub mod error;
pub mod inbox;
pub mod instrument;
pub mod outbox;
pub mod test_utils;
pub mod tracing;

// Re-exports for primary public API
#[cfg(feature = "rabbitmq")]
pub use broker::RabbitMqBroker;
pub use broker::{
    BatchPublishResult, InMemoryBroker, MessageConsumer, MessagePublisher, RawBrokerMessage,
    RawMessageHandler,
};
pub use cloudevent::{
    CloudEvent, CloudEventBuilder, CloudEventExtensions, from_binary_message, from_raw_message,
    from_structured_bytes, to_binary_message, to_structured_bytes,
};
pub use config::{
    BrokerConnectionConfig, BrokerType, CleanupStrategy, ConsumerConfig, MessagingConfig,
    OutboxConfig,
};
pub use consumer::{
    BackoffPolicy, ConsumerPipeline, DeadLetterHeaders, MessageContext, MessageHandler,
    route_to_dlq,
};
pub use error::{MessageProcessingError, MessagingError};
pub use inbox::{DualLayerDeduplicator, INBOX_TABLE_DDL, InboxRecord, InboxRepository};
pub use outbox::{OUTBOX_TABLE_DDL, OutboxRecord, OutboxRepository, OutboxStatus};
#[cfg(feature = "postgres")]
pub use outbox::{OutboxRetentionWorker, dispatch_batch, run_relay_loop};
pub use tracing::{create_consumer_span, extract_trace_context, inject_trace_context};
