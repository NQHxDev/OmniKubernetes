//! Unified Message Consumer Abstraction (`consumer.rs`).
//!
//! Provides the broker-agnostic [`MessageConsumer`] and [`RawMessageHandler`] traits.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{broker::RawBrokerMessage, error::MessagingError};

/// Handler xử lý thông điệp thô tiếp nhận từ Broker trước khi chuyển sang Typed Handler.
#[async_trait]
pub trait RawMessageHandler: Send + Sync {
    /// Xử lý một bản tin thô nhận được từ Broker.
    async fn handle_raw(&self, message: RawBrokerMessage) -> Result<(), MessagingError>;
}

/// Trait nhận thông điệp từ Message Broker.
#[async_trait]
pub trait MessageConsumer: Send + Sync {
    /// Khởi động việc đăng ký lắng nghe (Subscribe) trên topic chỉ định với một consumer group.
    async fn subscribe(
        &self,
        topic: &str,
        consumer_group: &str,
        handler: Arc<dyn RawMessageHandler>,
    ) -> Result<(), MessagingError>;
}
