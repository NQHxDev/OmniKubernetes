//! Hierarchical Configuration Model & Guardrails (`config.rs`).
//!
//! Provides typed configuration structs for Message Brokers, Transactional Outbox,
//! and Resilient Consumer with Phase 2 fail-fast startup invariants.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::error::MessagingError;

/// Cấu hình tổng thể của phân hệ Event-Driven Messaging.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct MessagingConfig {
    /// Loại broker sử dụng: `in_memory`, `rabbitmq`, `kafka`, `nats`.
    pub broker_type: BrokerType,

    /// Tham số kết nối broker.
    #[validate(nested)]
    pub broker: BrokerConnectionConfig,

    /// Tham số của Transactional Outbox Engine.
    #[validate(nested)]
    pub outbox: OutboxConfig,

    /// Tham số của Resilient Consumer Framework.
    #[validate(nested)]
    pub consumer: ConsumerConfig,
}

impl MessagingConfig {
    /// Kiểm tra các bất biến logic (Phase 2 Fail-Fast Startup Guardrails).
    pub fn validate_guardrails(&self) -> Result<(), MessagingError> {
        // 1. Kiểm tra URL Broker theo từng loại
        let url = self.broker.url.expose_secret();
        match self.broker_type {
            BrokerType::Rabbitmq => {
                if !url.starts_with("amqp://") && !url.starts_with("amqps://") {
                    return Err(MessagingError::Configuration(
                        "RabbitMQ broker URL must start with 'amqp://' or 'amqps://'".into(),
                    ));
                }
            }
            BrokerType::Nats => {
                if !url.starts_with("nats://") && !url.starts_with("tls://") {
                    return Err(MessagingError::Configuration(
                        "NATS broker URL must start with 'nats://' or 'tls://'".into(),
                    ));
                }
            }
            BrokerType::Kafka => {
                if url.trim().is_empty() {
                    return Err(MessagingError::Configuration(
                        "Kafka bootstrap servers URL cannot be empty".into(),
                    ));
                }
            }
            BrokerType::InMemory => {}
        }

        // 2. Backoff Invariant
        if self.consumer.initial_retry_backoff > self.consumer.max_retry_backoff {
            return Err(MessagingError::Configuration(format!(
                "Consumer initial_retry_backoff ({:?}) must be <= max_retry_backoff ({:?})",
                self.consumer.initial_retry_backoff, self.consumer.max_retry_backoff
            )));
        }

        // 3. Lock Duration vs Poll Interval Invariant
        // lock_duration must be at least 3x poll_interval to prevent premature duplicate processing
        if self.outbox.lock_duration < self.outbox.poll_interval * 3 {
            return Err(MessagingError::Configuration(format!(
                "Outbox lock_duration ({:?}) must be at least 3x poll_interval ({:?})",
                self.outbox.lock_duration, self.outbox.poll_interval
            )));
        }

        Ok(())
    }
}

/// Loại Message Broker được hỗ trợ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrokerType {
    /// In-memory channel dùng cho Unit Testing và Standalone verification.
    #[default]
    InMemory,
    /// RabbitMQ AMQP 0-9-1.
    Rabbitmq,
    /// Apache Kafka / Redpanda.
    Kafka,
    /// NATS JetStream.
    Nats,
}

/// Cấu hình kết nối tới Message Broker.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BrokerConnectionConfig {
    /// URL kết nối được bảo vệ trong SecretString chống rò rỉ.
    #[serde(serialize_with = "omni_common::config::secret::serialize_redacted_secret")]
    pub url: SecretString,

    /// Thời gian chờ kết nối tối đa.
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: Duration,

    /// Tần suất gửi heartbeat giữ kết nối sống (giây).
    #[serde(default = "default_heartbeat_secs")]
    pub heartbeat_secs: u64,

    /// Kích thước pool kết nối nội bộ.
    #[validate(range(min = 1, max = 100))]
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,

    /// Bật/tắt bảo mật tầng vận chuyển TLS.
    #[serde(default)]
    pub tls_enabled: bool,
}

impl Default for BrokerConnectionConfig {
    fn default() -> Self {
        Self {
            url: SecretString::from("amqp://guest:guest@localhost:5672/%2f"),
            connection_timeout: default_connection_timeout(),
            heartbeat_secs: default_heartbeat_secs(),
            pool_size: default_pool_size(),
            tls_enabled: false,
        }
    }
}

/// Cấu hình Transactional Outbox Engine.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OutboxConfig {
    /// Bật hoặc tắt tính năng Outbox Publisher Worker trên tiến trình này.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Chu kỳ quét cơ sở dữ liệu định kỳ (ms).
    #[serde(default = "default_poll_interval")]
    pub poll_interval: Duration,

    /// Số lượng bản tin lấy ra tối đa trong mỗi mẻ quét.
    #[validate(range(min = 1, max = 1000))]
    #[serde(default = "default_batch_size")]
    pub batch_size: i64,

    /// Thời gian khóa một bản tin trước khi cho phép worker khác lấy lại.
    #[serde(default = "default_lock_duration")]
    pub lock_duration: Duration,

    /// Số lần thử gửi lại tối đa khi broker từ chối hoặc lỗi mạng.
    #[validate(range(min = 1, max = 20))]
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,

    /// Tên kênh PostgreSQL LISTEN/NOTIFY để đánh thức relay tức thì.
    #[serde(default = "default_notify_channel")]
    pub notify_channel: String,

    /// Chiến lược dọn dẹp dữ liệu outbox đã gửi thành công.
    #[serde(default)]
    pub cleanup_strategy: CleanupStrategy,

    /// Thời hạn lưu trữ dữ liệu trước khi dọn dẹp (giờ).
    #[validate(range(min = 1, max = 720))]
    #[serde(default = "default_retention_hours")]
    pub retention_hours: i64,
}

impl Default for OutboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval: default_poll_interval(),
            batch_size: default_batch_size(),
            lock_duration: default_lock_duration(),
            max_retries: default_max_retries(),
            notify_channel: default_notify_channel(),
            cleanup_strategy: CleanupStrategy::default(),
            retention_hours: default_retention_hours(),
        }
    }
}

/// Chiến lược dọn dẹp bản tin Outbox / Inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStrategy {
    /// Xóa vật lý ngay lập tức khi gửi thành công sang Broker (Tối ưu lưu trữ nhất).
    HardDelete,
    /// Đánh dấu trạng thái `PUBLISHED` và giữ nguyên (Cần công cụ ngoài prune).
    SoftDelete,
    /// Định kỳ quét và xóa các bản tin đã hoàn tất vượt quá retention_hours (Khuyến nghị).
    #[default]
    RetentionPruning,
}

/// Cấu hình Resilient Consumer Framework.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConsumerConfig {
    /// Số lượng tác vụ worker song song xử lý bản tin.
    #[validate(range(min = 1, max = 256))]
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    /// Số bản tin lấy về sẵn trong bộ nhớ đệm của socket (Prefetch Count).
    #[validate(range(min = 1, max = 1000))]
    #[serde(default = "default_prefetch_count")]
    pub prefetch_count: u16,

    /// Số lần thử lại tối đa cho mỗi thông điệp bị lỗi trước khi chuyển sang DLQ.
    #[validate(range(min = 0, max = 20))]
    #[serde(default = "default_max_retries_u32")]
    pub max_retries: u32,

    /// Thời gian chờ thử lại ban đầu.
    #[serde(default = "default_initial_backoff")]
    pub initial_retry_backoff: Duration,

    /// Thời gian chờ thử lại tối đa.
    #[serde(default = "default_max_backoff")]
    pub max_retry_backoff: Duration,

    /// Hệ số nhân Exponential Backoff.
    #[validate(range(min = 1.0, max = 10.0))]
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// Bật jitter ngẫu nhiên để chống thundering herd.
    #[serde(default = "default_true")]
    pub backoff_jitter: bool,

    /// Hậu tố gắn vào topic để tạo DLQ topic tương ứng.
    #[serde(default = "default_dlq_suffix")]
    pub dlq_topic_suffix: String,

    /// Thời gian sống của bản ghi kiểm tra trùng lặp trên Cache (giây).
    #[serde(default = "default_dedup_ttl")]
    pub dedup_ttl: Duration,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            prefetch_count: default_prefetch_count(),
            max_retries: default_max_retries_u32(),
            initial_retry_backoff: default_initial_backoff(),
            max_retry_backoff: default_max_backoff(),
            backoff_multiplier: default_backoff_multiplier(),
            backoff_jitter: default_true(),
            dlq_topic_suffix: default_dlq_suffix(),
            dedup_ttl: default_dedup_ttl(),
        }
    }
}

// Default helper functions
const fn default_true() -> bool {
    true
}

const fn default_connection_timeout() -> Duration {
    Duration::from_millis(5000)
}

const fn default_heartbeat_secs() -> u64 {
    30
}

const fn default_pool_size() -> usize {
    10
}

const fn default_poll_interval() -> Duration {
    Duration::from_millis(1000)
}

const fn default_batch_size() -> i64 {
    100
}

const fn default_lock_duration() -> Duration {
    Duration::from_secs(30)
}

const fn default_max_retries() -> i32 {
    5
}

const fn default_max_retries_u32() -> u32 {
    5
}

fn default_notify_channel() -> String {
    "omni_outbox_tick".to_string()
}

const fn default_retention_hours() -> i64 {
    72
}

const fn default_concurrency() -> usize {
    16
}

const fn default_prefetch_count() -> u16 {
    50
}

const fn default_initial_backoff() -> Duration {
    Duration::from_millis(100)
}

const fn default_max_backoff() -> Duration {
    Duration::from_secs(60)
}

const fn default_backoff_multiplier() -> f64 {
    2.0
}

fn default_dlq_suffix() -> String {
    ".dlq".to_string()
}

const fn default_dedup_ttl() -> Duration {
    Duration::from_hours(24)
}
