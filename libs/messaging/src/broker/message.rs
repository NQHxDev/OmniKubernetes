//! Raw Broker Transport Message Representation (`message.rs`).
//!
//! Provides the broker-agnostic wire message format [`RawBrokerMessage`] encapsulating
//! routing topics, partition keys, raw byte payloads, and metadata headers.

use std::collections::HashMap;

/// Đại diện cho một thông điệp thô trên đường truyền Broker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBrokerMessage {
    /// Tên topic, exchange, hoặc subject đích.
    pub topic: String,

    /// Khóa phân vùng bảo đảm thứ tự tuần tự của bản tin.
    pub partition_key: Option<String>,

    /// Payload dữ liệu nhị phân thô của bản tin.
    pub payload: Vec<u8>,

    /// Bảng metadata headers đính kèm bản tin.
    pub headers: HashMap<String, String>,
}

impl RawBrokerMessage {
    /// Khởi tạo một [`RawBrokerMessage`] mới.
    #[must_use]
    pub fn new(topic: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            topic: topic.into(),
            partition_key: None,
            payload,
            headers: HashMap::new(),
        }
    }

    /// Gán khóa phân vùng cho bản tin.
    #[must_use]
    pub fn with_partition_key(mut self, key: impl Into<String>) -> Self {
        self.partition_key = Some(key.into());
        self
    }

    /// Thêm một header vào bảng metadata.
    #[must_use]
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Gán toàn bộ headers.
    #[must_use]
    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Lấy giá trị của một header (tìm kiếm không phân biệt hoa thường).
    #[must_use]
    pub fn header(&self, key: &str) -> Option<&str> {
        let key_lower = key.to_ascii_lowercase();
        self.headers.iter().find_map(|(k, v)| {
            if k.to_ascii_lowercase() == key_lower {
                Some(v.as_str())
            } else {
                None
            }
        })
    }
}
