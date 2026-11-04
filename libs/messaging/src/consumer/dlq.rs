//! Poison Pill Isolation & Dead Letter Queue (DLQ) Routing (`dlq.rs`).
//!
//! Provides diagnostic headers injection and dead-letter routing for poison pills
//! or messages that exceed maximum retry thresholds.

use chrono::Utc;

use crate::{
    broker::{RawBrokerMessage, publisher::MessagePublisher},
    error::MessagingError,
    instrument::record_dlq_routed,
};

/// Các Header chẩn đoán được tiêm vào bản tin khi định tuyến sang Dead Letter Queue.
#[derive(Debug)]
pub struct DeadLetterHeaders;

impl DeadLetterHeaders {
    /// Nguyên nhân dẫn tới việc đưa vào DLQ.
    pub const REASON: &'static str = "x-omni-death-reason";
    /// Tên topic gốc trước khi bị chuyển hướng.
    pub const ORIGINAL_TOPIC: &'static str = "x-omni-original-topic";
    /// Tổng số lần thử lại đã thực hiện.
    pub const FAILURE_COUNT: &'static str = "x-omni-failure-count";
    /// Dấu thời gian khi bản tin bị chuyển sang DLQ (RFC 3339 UTC).
    pub const FAILED_AT: &'static str = "x-omni-failed-at";
    /// Chuỗi thông báo lỗi hoặc vết lỗi.
    pub const STACK_TRACE: &'static str = "x-omni-stack-trace";
}

/// Đóng gói bản tin với các header chẩn đoán lỗi và xuất bản sang Dead Letter Topic.
pub async fn route_to_dlq(
    publisher: &dyn MessagePublisher,
    message: &RawBrokerMessage,
    dlq_topic: &str,
    reason: &str,
    attempt: u32,
) -> Result<(), MessagingError> {
    let mut dlq_headers = message.headers.clone();

    dlq_headers.insert(DeadLetterHeaders::REASON.to_string(), reason.to_string());
    dlq_headers.insert(
        DeadLetterHeaders::ORIGINAL_TOPIC.to_string(),
        message.topic.clone(),
    );
    dlq_headers.insert(
        DeadLetterHeaders::FAILURE_COUNT.to_string(),
        attempt.to_string(),
    );
    dlq_headers.insert(
        DeadLetterHeaders::FAILED_AT.to_string(),
        Utc::now().to_rfc3339(),
    );
    dlq_headers.insert(
        DeadLetterHeaders::STACK_TRACE.to_string(),
        reason.to_string(),
    );

    let dlq_message = RawBrokerMessage {
        topic: dlq_topic.to_string(),
        partition_key: message.partition_key.clone(),
        payload: message.payload.clone(),
        headers: dlq_headers,
    };

    tracing::error!(
        original_topic = %message.topic,
        dlq_topic = %dlq_topic,
        attempt = attempt,
        reason = %reason,
        "Routing unrecoverable or retry-exhausted message to Dead Letter Queue"
    );

    record_dlq_routed(&message.topic, reason);

    publisher.publish(dlq_message).await
}
