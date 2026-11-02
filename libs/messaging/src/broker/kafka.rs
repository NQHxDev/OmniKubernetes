//! Apache Kafka / Redpanda Broker Driver Stub (`kafka.rs`).
//!
//! Provides the Kafka adapter interface conforming to [`MessagePublisher`] and [`MessageConsumer`].

use crate::error::MessagingError;

/// Cấu hình và điều phối kết nối Kafka / Redpanda.
#[derive(Debug, Clone, Default)]
pub struct KafkaBroker;

impl KafkaBroker {
    /// Khởi tạo [`KafkaBroker`] mới.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Kiểm tra tính khả dụng của Kafka Driver.
    pub fn ensure_supported() -> Result<(), MessagingError> {
        Err(MessagingError::Configuration(
            "Kafka driver is not enabled in this build. Compile with feature 'kafka'.".into(),
        ))
    }
}
