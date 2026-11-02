//! CloudEvents Extension Context Attributes (`extensions.rs`).
//!
//! Provides standardized and custom metadata extensions adhering to CNCF CloudEvents v1.0.2,
//! including W3C Distributed Tracing (`traceparent`, `tracestate`), multi-tenancy, actor identification,
//! aggregate partitioning, and idempotency keys.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Các thuộc tính mở rộng chuẩn hóa đính kèm trong CloudEvent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudEventExtensions {
    /// W3C Distributed Trace Context: `traceparent` (version-trace_id-parent_id-trace_flags).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,

    /// W3C `tracestate` lưu trữ routing context của các nhà cung cấp APM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,

    /// Định danh Tenant sở hữu dữ liệu trong kiến trúc Multi-Tenant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<Uuid>,

    /// Định danh Actor/User khởi tạo hành động.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<Uuid>,

    /// Khóa phân vùng để Broker định tuyến vào cùng partition (Aggregate ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_key: Option<String>,

    /// Khóa định danh chống trùng lặp (Idempotency Key). Nếu không điền, mặc định bằng `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,

    /// Bảng lưu các thuộc tính mở rộng tùy biến khác.
    #[serde(flatten)]
    pub custom: HashMap<String, String>,
}

impl CloudEventExtensions {
    /// Khởi tạo [`CloudEventExtensions`] rỗng.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Trả về khóa chống trùng lặp hiệu dụng, fallback về chuỗi UUID nếu không được thiết lập.
    #[must_use]
    pub fn effective_idempotency_key(&self, fallback_id: Uuid) -> String {
        self.idempotency_key
            .clone()
            .unwrap_or_else(|| fallback_id.to_string())
    }

    /// Lấy giá trị của một thuộc tính mở rộng tùy biến.
    #[must_use]
    pub fn get_custom(&self, key: &str) -> Option<&str> {
        self.custom.get(key).map(String::as_str)
    }

    /// Thêm một thuộc tính mở rộng tùy biến.
    pub fn insert_custom(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.custom.insert(key.into(), value.into());
    }
}
