//! OpenTelemetry & Prometheus Messaging Metrics Facade (`metrics.rs`).
//!
//! Provides standard Prometheus metric keys and helper functions adhering to
//! OpenTelemetry Semantic Conventions for Messaging Systems v1.27+.

use std::time::Duration;

// Bộ chỉ số Transactional Outbox (Publisher Side)
pub const METRIC_OUTBOX_QUEUE_DEPTH: &str = "messaging_outbox_queue_depth";
pub const METRIC_OUTBOX_PUBLISHED_TOTAL: &str = "messaging_outbox_published_total";
pub const METRIC_OUTBOX_PUBLISH_DURATION: &str = "messaging_outbox_publish_duration_seconds";
pub const METRIC_OUTBOX_RETRIES_TOTAL: &str = "messaging_outbox_retries_total";
pub const METRIC_OUTBOX_FAILED_TOTAL: &str = "messaging_outbox_failed_total";

// Bộ chỉ số Resilient Consumer (Consumer Side)
pub const METRIC_CONSUMER_RECEIVED_TOTAL: &str = "messaging_consumer_received_total";
pub const METRIC_CONSUMER_PROCESS_DURATION: &str = "messaging_consumer_process_duration_seconds";
pub const METRIC_CONSUMER_ERRORS_TOTAL: &str = "messaging_consumer_errors_total";
pub const METRIC_CONSUMER_RETRIES_TOTAL: &str = "messaging_consumer_retries_total";
pub const METRIC_CONSUMER_DEDUP_HITS_TOTAL: &str = "messaging_consumer_dedup_hits_total";
pub const METRIC_DLQ_ROUTED_TOTAL: &str = "messaging_dlq_routed_total";

/// Ghi nhận độ sâu hàng đợi outbox (số lượng bản ghi PENDING).
pub fn record_outbox_queue_depth(depth: f64) {
    metrics::gauge!(METRIC_OUTBOX_QUEUE_DEPTH).set(depth);
}

/// Ghi nhận bản tin outbox đã gửi thành công sang Broker.
pub fn record_outbox_published(topic: &str) {
    metrics::counter!(METRIC_OUTBOX_PUBLISHED_TOTAL, "topic" => topic.to_string()).increment(1);
}

/// Ghi nhận thời gian gửi bản tin sang Broker.
pub fn record_outbox_publish_duration(topic: &str, duration: Duration) {
    metrics::histogram!(METRIC_OUTBOX_PUBLISH_DURATION, "topic" => topic.to_string())
        .record(duration.as_secs_f64());
}

/// Ghi nhận số lần thử lại gửi outbox.
pub fn record_outbox_retry(topic: &str) {
    metrics::counter!(METRIC_OUTBOX_RETRIES_TOTAL, "topic" => topic.to_string()).increment(1);
}

/// Ghi nhận outbox gửi thất bại vĩnh viễn (FAILED/DEAD).
pub fn record_outbox_failed(topic: &str) {
    metrics::counter!(METRIC_OUTBOX_FAILED_TOTAL, "topic" => topic.to_string()).increment(1);
}

/// Ghi nhận thông điệp nhận từ broker tại consumer.
pub fn record_consumer_received(topic: &str, consumer_group: &str) {
    metrics::counter!(
        METRIC_CONSUMER_RECEIVED_TOTAL,
        "topic" => topic.to_string(),
        "consumer_group" => consumer_group.to_string()
    )
    .increment(1);
}

/// Ghi nhận thời gian consumer xử lý thông điệp.
pub fn record_consumer_duration(topic: &str, consumer_group: &str, duration: Duration) {
    metrics::histogram!(
        METRIC_CONSUMER_PROCESS_DURATION,
        "topic" => topic.to_string(),
        "consumer_group" => consumer_group.to_string()
    )
    .record(duration.as_secs_f64());
}

/// Ghi nhận lỗi trong quá trình consumer xử lý thông điệp.
pub fn record_consumer_error(topic: &str, consumer_group: &str, error_type: &str) {
    metrics::counter!(
        METRIC_CONSUMER_ERRORS_TOTAL,
        "topic" => topic.to_string(),
        "consumer_group" => consumer_group.to_string(),
        "error_type" => error_type.to_string()
    )
    .increment(1);
}

/// Ghi nhận số lần consumer kích hoạt retry.
pub fn record_consumer_retry(topic: &str, consumer_group: &str) {
    metrics::counter!(
        METRIC_CONSUMER_RETRIES_TOTAL,
        "topic" => topic.to_string(),
        "consumer_group" => consumer_group.to_string()
    )
    .increment(1);
}

/// Ghi nhận trường hợp bản tin bị trùng lặp và bỏ qua xử lý.
pub fn record_consumer_dedup_hit(topic: &str, consumer_group: &str) {
    metrics::counter!(
        METRIC_CONSUMER_DEDUP_HITS_TOTAL,
        "topic" => topic.to_string(),
        "consumer_group" => consumer_group.to_string()
    )
    .increment(1);
}

/// Ghi nhận bản tin được điều hướng sang Dead Letter Queue (DLQ).
pub fn record_dlq_routed(topic: &str, reason: &str) {
    metrics::counter!(
        METRIC_DLQ_ROUTED_TOTAL,
        "topic" => topic.to_string(),
        "reason" => reason.to_string()
    )
    .increment(1);
}

/// Khử dữ liệu cá nhân nhạy cảm (PII) trong cây JSON payload trước khi in ra log.
pub fn sanitize_json_payload(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sanitized = serde_json::Map::new();
            for (k, v) in map {
                if omni_logging::sanitizer::is_sensitive_key(k) {
                    if let serde_json::Value::String(s) = v {
                        sanitized.insert(
                            k.clone(),
                            serde_json::Value::String(
                                omni_logging::sanitizer::sanitize_field(k, s).into_owned(),
                            ),
                        );
                    } else {
                        sanitized.insert(
                            k.clone(),
                            serde_json::Value::String(
                                omni_logging::sanitizer::REDACTED.to_string(),
                            ),
                        );
                    }
                } else {
                    sanitized.insert(k.clone(), sanitize_json_payload(v));
                }
            }
            serde_json::Value::Object(sanitized)
        }
        serde_json::Value::Array(arr) => {
            let sanitized: Vec<serde_json::Value> = arr.iter().map(sanitize_json_payload).collect();
            serde_json::Value::Array(sanitized)
        }
        other => other.clone(),
    }
}
