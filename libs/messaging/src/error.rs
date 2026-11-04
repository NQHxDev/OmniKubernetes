//! Messaging Error Taxonomy & RFC 9457 Integration (`error.rs`).
//!
//! Provides comprehensive error types for broker infrastructure, serialization,
//! deduplication, poison pill isolation, and seamless mapping to [`omni_errors::AppError`].

use std::time::Duration;

use thiserror::Error;

/// Bảng mã lỗi của phân hệ Messaging.
#[derive(Debug, Error)]
pub enum MessagingError {
    /// Lỗi kết nối tới Message Broker.
    #[error("Broker connection failed: {0}")]
    BrokerConnectionFailed(String),

    /// Thao tác gửi bản tin bị timeout.
    #[error("Message publish timed out after {0:?}")]
    PublishTimeout(Duration),

    /// Message Broker từ chối tiếp nhận bản tin (NACK / Reject).
    #[error("Broker rejected message: {0}")]
    PublishRejected(String),

    /// Lỗi tuần tự hóa dữ liệu CloudEvent sang JSON / Binary.
    #[error("Failed to serialize CloudEvent: {0}")]
    SerializationFailed(String),

    /// Lỗi giải tuần tự hóa dữ liệu CloudEvent từ Broker payload / headers.
    #[error("Failed to deserialize CloudEvent: {0}")]
    DeserializationFailed(String),

    /// Lỗi truy xuất cơ sở dữ liệu PostgreSQL cho Transactional Outbox / Inbox.
    #[cfg(feature = "postgres")]
    #[error("Database persistence error: {0}")]
    DatabaseError(#[from] omni_database::DbError),

    /// Lỗi truy xuất cơ sở dữ liệu khi không bật feature postgres.
    #[cfg(not(feature = "postgres"))]
    #[error("Database persistence error: {0}")]
    DatabaseError(String),

    /// Lỗi tương tác với tầng cache phục vụ Fast-path Idempotency.
    #[error("Cache deduplication error: {0}")]
    CacheError(String),

    /// Bản tin bị trùng lặp dựa trên idempotency_key.
    #[error("Duplicate event detected with idempotency_key: {0}")]
    DuplicateEvent(String),

    /// Vượt quá số lần thử lại tối đa cho phép, chuyển tiếp sang Dead Letter Queue (DLQ).
    #[error("Exceeded maximum retry attempts ({max_retries}). Routed to DLQ: {reason}")]
    DeadLetterThresholdExceeded {
        /// Số lần thử tối đa đã vượt qua.
        max_retries: u32,
        /// Lý do lỗi chi tiết.
        reason: String,
    },

    /// Phát hiện bản tin độc (Poison Pill) không thể hồi phục.
    #[error("Unrecoverable poison pill detected: {0}")]
    PoisonPill(String),

    /// Xung đột tranh chấp khóa bản ghi Outbox giữa các worker.
    #[error("Outbox lock contention on record: {0}")]
    LockContention(uuid::Uuid),

    /// Lỗi cấu hình tham số phân hệ Messaging.
    #[error("Messaging configuration error: {0}")]
    Configuration(String),

    /// Lỗi nội bộ không xác định khác.
    #[error("Internal messaging error: {0}")]
    Internal(String),
}

/// Phân loại lỗi xử lý bản tin từ phía Consumer Handler.
#[derive(Debug, Error)]
pub enum MessageProcessingError {
    /// Lỗi tạm thời (Transient): Có thể tự hồi phục bằng cách thử lại sau exponential backoff.
    #[error("Transient error (will retry): {0}")]
    Transient(String),

    /// Lỗi vĩnh viễn (Terminal / Poison Pill): Không thể khắc phục, chuyển thẳng sang DLQ.
    #[error("Terminal error (will route to DLQ): {0}")]
    Terminal(String),
}

impl From<MessageProcessingError> for MessagingError {
    fn from(err: MessageProcessingError) -> Self {
        match err {
            MessageProcessingError::Transient(msg) => Self::Internal(msg),
            MessageProcessingError::Terminal(msg) => Self::PoisonPill(msg),
        }
    }
}

impl From<serde_json::Error> for MessagingError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationFailed(err.to_string())
    }
}

impl From<MessagingError> for omni_errors::AppError {
    fn from(err: MessagingError) -> Self {
        match err {
            MessagingError::DuplicateEvent(key) => omni_errors::AppError::Conflict {
                detail: Some(format!("Duplicate event detected: {key}").into()),
            },
            MessagingError::PoisonPill(detail) | MessagingError::DeserializationFailed(detail) => {
                omni_errors::AppError::BadRequest {
                    detail: Some(format!("Unprocessable event payload: {detail}").into()),
                }
            }
            MessagingError::PublishTimeout(_) | MessagingError::BrokerConnectionFailed(_) => {
                omni_errors::AppError::ServiceUnavailable
            }
            MessagingError::DeadLetterThresholdExceeded {
                max_retries,
                reason,
            } => omni_errors::AppError::Internal(format!(
                "Dead letter threshold exceeded (max_retries={max_retries}): {reason}"
            )),
            other => omni_errors::AppError::Internal(other.to_string()),
        }
    }
}
