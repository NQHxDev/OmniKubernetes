//! Pluggable Message Broker Transport Layer (`broker`).
//!
//! Provides the core traits [`MessagePublisher`], [`MessageConsumer`], and [`RawBrokerMessage`],
//! along with pluggable driver adapters:
//! - [`InMemoryBroker`]: In-process channel for tests and standalone environments.
//! - [`RabbitMqBroker`]: AMQP 0-9-1 Topic Exchange & Quorum Queues (lapin).
//! - [`KafkaBroker`]: Apache Kafka / Redpanda Driver adapter.
//! - [`NatsBroker`]: NATS JetStream Driver adapter.

pub mod consumer;
pub mod kafka;
pub mod memory;
pub mod message;
pub mod nats;
pub mod publisher;
pub mod rabbitmq;

pub use consumer::{MessageConsumer, RawMessageHandler};
pub use kafka::KafkaBroker;
pub use memory::InMemoryBroker;
pub use message::RawBrokerMessage;
pub use nats::NatsBroker;
pub use publisher::{BatchPublishResult, MessagePublisher};
#[cfg(feature = "rabbitmq")]
pub use rabbitmq::RabbitMqBroker;
