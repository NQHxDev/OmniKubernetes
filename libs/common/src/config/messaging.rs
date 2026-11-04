//! Event-driven messaging and outbox pattern configuration.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::{redact_connection_url, serialize_redacted_secret};

/// Cấu hình truyền thông hướng sự kiện & Transactional Outbox (`omni-messaging`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct MessagingConfig {
    /// Cờ kích hoạt hệ thống hàng đợi tin nhắn / sự kiện.
    pub enabled: bool,

    /// Chuỗi kết nối Message Broker `RabbitMQ / Kafka` chứa thông tin xác thực.
    #[serde(serialize_with = "serialize_redacted_secret")]
    pub broker_url: SecretString,

    /// Kích thước batch lấy tin nhắn từ Transactional Outbox.
    #[validate(range(min = 1, max = 5000))]
    pub outbox_batch_size: u32,

    /// Chu kỳ quét bảng outbox (mili-giây).
    #[validate(range(min = 50, max = 60000))]
    pub outbox_poll_interval_ms: u64,

    /// Số lần thử lại tối đa khi gửi tin thất bại trước khi đưa vào Dead Letter Queue.
    #[validate(range(min = 0, max = 20))]
    pub max_retries: u32,

    /// Phần mở rộng đuôi cho Dead Letter Queue topic.
    #[validate(length(min = 1))]
    pub dlq_topic_suffix: String,
}

impl Default for MessagingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            broker_url: SecretString::from("amqp://guest:guest@localhost:5672/%2f"),
            outbox_batch_size: 100,
            outbox_poll_interval_ms: 500,
            max_retries: 5,
            dlq_topic_suffix: ".dlq".to_string(),
        }
    }
}

impl MessagingConfig {
    /// Trả về chuỗi kết nối broker đã được che mật khẩu.
    #[must_use]
    pub fn redacted_broker_url(&self) -> String {
        redact_connection_url(self.broker_url.expose_secret())
    }
}
