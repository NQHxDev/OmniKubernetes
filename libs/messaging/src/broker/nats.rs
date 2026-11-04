//! NATS JetStream Broker Driver Stub (`nats.rs`).
//!
//! Provides the NATS JetStream adapter interface conforming to [`crate::broker::MessagePublisher`] and [`crate::broker::MessageConsumer`].

use crate::error::MessagingError;

/// Cấu hình và điều phối kết nối NATS JetStream.
#[derive(Debug, Clone, Default)]
pub struct NatsBroker;

impl NatsBroker {
    /// Khởi tạo [`NatsBroker`] mới.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Kiểm tra tính khả dụng của NATS Driver.
    pub fn ensure_supported() -> Result<(), MessagingError> {
        Err(MessagingError::Configuration(
            "NATS driver is not enabled in this build. Compile with feature 'nats'.".into(),
        ))
    }
}
