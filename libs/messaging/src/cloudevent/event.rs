//! CNCF CloudEvents v1.0.2 Data Envelope (`event.rs`).
//!
//! Provides the primary [`CloudEvent<T>`] generic container adhering to
//! CNCF CloudEvents specification v1.0.2 with UUIDv7 identifiers, RFC 3339 UTC timestamps,
//! and reverse-DNS event taxonomies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cloudevent::{builder::CloudEventBuilder, extensions::CloudEventExtensions};

/// Hàm mặc định cung cấp kiểu nội dung JSON cho CloudEvents.
#[must_use]
pub fn default_content_type() -> String {
    "application/json".to_string()
}

/// Phong bì sự kiện chuẩn CNCF CloudEvents v1.0.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudEvent<T> {
    /// Định danh sự kiện duy nhất (Bắt buộc phải là UUIDv7 time-ordered).
    pub id: Uuid,

    /// Định danh thành phần phát hành sự kiện (URI-reference dạng URN).
    /// Ví dụ: `urn:omni:service:order`
    pub source: String,

    /// Phiên bản của đặc tả CloudEvents. Luôn cố định là "1.0".
    pub specversion: String,

    /// Tên loại sự kiện theo quy tắc phân cấp ngược miền (Reverse-DNS taxonomy).
    /// Ví dụ: `com.omni.order.created.v1`
    #[serde(rename = "type")]
    pub event_type: String,

    /// Thời điểm sinh ra sự kiện theo chuẩn RFC 3339 UTC.
    pub time: DateTime<Utc>,

    /// Kiểu nội dung của payload dữ liệu. Mặc định là "application/json".
    #[serde(default = "default_content_type")]
    pub datacontenttype: String,

    /// URI trỏ tới JSON Schema định nghĩa cấu trúc dữ liệu của payload (tùy chọn).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataschema: Option<String>,

    /// Định danh thực thể nghiệp vụ là đối tượng của sự kiện (Aggregate ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    /// Payload dữ liệu nghiệp vụ thực tế.
    pub data: T,

    /// Các thuộc tính mở rộng chuẩn hóa (CloudEvents Extension Context Attributes).
    #[serde(flatten)]
    pub extensions: CloudEventExtensions,
}

impl<T> CloudEvent<T> {
    /// Khởi tạo một [`CloudEventBuilder`] mới để dựng sự kiện kiểu tĩnh.
    pub fn builder(
        source: impl Into<String>,
        event_type: impl Into<String>,
        data: T,
    ) -> CloudEventBuilder<T> {
        CloudEventBuilder::new(source, event_type, data)
    }

    /// Chuyển đổi kiểu dữ liệu payload sang kiểu khác bằng hàm biến đổi.
    pub fn map_data<U>(self, f: impl FnOnce(T) -> U) -> CloudEvent<U> {
        CloudEvent {
            id: self.id,
            source: self.source,
            specversion: self.specversion,
            event_type: self.event_type,
            time: self.time,
            datacontenttype: self.datacontenttype,
            dataschema: self.dataschema,
            subject: self.subject,
            data: f(self.data),
            extensions: self.extensions,
        }
    }
}
