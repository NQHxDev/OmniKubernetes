//! Transactional Outbox Model Representation (`model.rs`).
//!
//! Provides the [`OutboxRecord`] struct and [`OutboxStatus`] state machine.

use std::fmt::Display;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trạng thái của bản ghi Outbox theo State Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutboxStatus {
    /// Đang chờ Outbox Relay kéo đi gửi.
    #[default]
    Pending,
    /// Đang được một luồng worker khóa để chuẩn bị xuất bản sang Broker.
    Processing,
    /// Broker đã xác nhận tiếp nhận thành công (Publisher Confirm).
    Published,
    /// Quá trình gửi thất bại vượt quá số lần thử lại tối đa.
    Failed,
    /// Đã chuyển tiếp sang Dead Letter Topic hoặc cần can thiệp thủ công.
    Dead,
}

impl OutboxStatus {
    /// Trả về chuỗi đại diện cho trạng thái lưu trữ trong SQL.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Processing => "PROCESSING",
            Self::Published => "PUBLISHED",
            Self::Failed => "FAILED",
            Self::Dead => "DEAD",
        }
    }
}

impl Display for OutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for OutboxStatus {
    fn from(s: &str) -> Self {
        match s {
            "PROCESSING" => Self::Processing,
            "PUBLISHED" => Self::Published,
            "FAILED" => Self::Failed,
            "DEAD" => Self::Dead,
            _ => Self::Pending,
        }
    }
}

/// Thực thể lưu trữ trong bảng `outbox_events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxRecord {
    /// Định danh sự kiện duy nhất (UUIDv7).
    pub id: Uuid,

    /// Tên loại Aggregate Root (ví dụ: "order", "payment").
    pub aggregate_type: String,

    /// Định danh của Aggregate Root instance.
    pub aggregate_id: String,

    /// Tên loại sự kiện (Reverse-DNS taxonomy).
    pub event_type: String,

    /// Topic / Exchange đích gửi tới Message Broker.
    pub destination_topic: String,

    /// Khóa phân vùng để Broker định tuyến vào cùng partition.
    pub partition_key: Option<String>,

    /// Toàn bộ CloudEvent payload lưu trữ dưới dạng JSONB.
    pub payload: serde_json::Value,

    /// Metadata headers lưu trữ dưới dạng JSONB.
    pub headers: serde_json::Value,

    /// Trạng thái xử lý hiện tại của bản ghi.
    pub status: OutboxStatus,

    /// Số lần thử lại đã thực hiện.
    pub retry_count: i32,

    /// Số lần thử lại tối đa cho phép.
    pub max_retries: i32,

    /// Thời điểm cho phép thử lại tiếp theo.
    pub next_retry_at: DateTime<Utc>,

    /// Thời hạn khóa đang được giữ bởi worker.
    pub locked_until: Option<DateTime<Utc>>,

    /// Thông điệp lỗi gần nhất nếu có.
    pub last_error: Option<String>,

    /// Thời điểm bản ghi được tạo ra trong Transaction nghiệp vụ.
    pub created_at: DateTime<Utc>,

    /// Thời điểm bản tin được Broker xác nhận xuất bản thành công.
    pub published_at: Option<DateTime<Utc>>,
}
