//! Idempotent Inbox Model Representation (`model.rs`).
//!
//! Provides the [`InboxRecord`] struct representing processed messages recorded in PostgreSQL.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Thực thể lưu trữ trong bảng `inbox_events` để bảo đảm tính lũy thừa (Idempotency).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxRecord {
    /// Định danh duy nhất của sự kiện (Idempotency Key / UUIDv7).
    pub id: Uuid,

    /// Tên loại sự kiện.
    pub event_type: String,

    /// Thành phần phát hành sự kiện (URN).
    pub source: String,

    /// Nhóm consumer đã xử lý bản tin.
    pub consumer_group: String,

    /// Thời điểm hoàn tất xử lý bản tin.
    pub processed_at: DateTime<Utc>,

    /// Trạng thái xử lý (thường là 'SUCCESS').
    pub status: String,

    /// Thời điểm bản ghi được tạo ra trong cơ sở dữ liệu.
    pub created_at: DateTime<Utc>,
}
