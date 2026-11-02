//! Type-Safe CloudEvent Builder Pattern (`builder.rs`).
//!
//! Provides a fluent, infallible builder for creating [`CloudEvent<T>`] instances with
//! automatic UUIDv7 generation, W3C trace context injection from [`RequestContext`],
//! and partition key derivation.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::cloudevent::{
    event::{CloudEvent, default_content_type},
    extensions::CloudEventExtensions,
};

/// Builder hỗ trợ khởi tạo [`CloudEvent<T>`] một cách an toàn và tiện lợi.
#[derive(Debug, Clone)]
pub struct CloudEventBuilder<T> {
    id: Option<Uuid>,
    source: String,
    event_type: String,
    time: Option<DateTime<Utc>>,
    datacontenttype: Option<String>,
    dataschema: Option<String>,
    subject: Option<String>,
    data: T,
    extensions: CloudEventExtensions,
}

impl<T> CloudEventBuilder<T> {
    /// Khởi tạo [`CloudEventBuilder`] với các thông tin cốt lõi bắt buộc.
    pub fn new(source: impl Into<String>, event_type: impl Into<String>, data: T) -> Self {
        Self {
            id: None,
            source: source.into(),
            event_type: event_type.into(),
            time: None,
            datacontenttype: None,
            dataschema: None,
            subject: None,
            data,
            extensions: CloudEventExtensions::default(),
        }
    }

    /// Thiết lập thủ công ID cho sự kiện (nếu không set sẽ tự sinh UUIDv7).
    #[must_use]
    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Thiết lập thực thể nghiệp vụ là đối tượng của sự kiện (Aggregate ID).
    #[must_use]
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Thiết lập URI trỏ tới JSON Schema của payload.
    #[must_use]
    pub fn dataschema(mut self, schema: impl Into<String>) -> Self {
        self.dataschema = Some(schema.into());
        self
    }

    /// Thiết lập kiểu nội dung của payload (mặc định "application/json").
    #[must_use]
    pub fn datacontenttype(mut self, content_type: impl Into<String>) -> Self {
        self.datacontenttype = Some(content_type.into());
        self
    }

    /// Thiết lập thời điểm sinh sự kiện (mặc định Utc::now()).
    #[must_use]
    pub fn time(mut self, time: DateTime<Utc>) -> Self {
        self.time = Some(time);
        self
    }

    /// Thiết lập khóa phân vùng (Partition Key) bảo đảm thứ tự tuần tự tại Broker.
    #[must_use]
    pub fn partition_key(mut self, key: impl Into<String>) -> Self {
        self.extensions.partition_key = Some(key.into());
        self
    }

    /// Thiết lập khóa chống trùng lặp (Idempotency Key).
    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.extensions.idempotency_key = Some(key.into());
        self
    }

    /// Thiết lập định danh Tenant trong môi trường đa người thuê (Multi-Tenancy).
    #[must_use]
    pub fn tenant_id(mut self, tenant_id: Uuid) -> Self {
        self.extensions.tenant_id = Some(tenant_id);
        self
    }

    /// Thiết lập định danh Actor/User thực hiện thao tác.
    #[must_use]
    pub fn actor_id(mut self, actor_id: Uuid) -> Self {
        self.extensions.actor_id = Some(actor_id);
        self
    }

    /// Thiết lập W3C Traceparent header thủ công.
    #[must_use]
    pub fn traceparent(mut self, tp: impl Into<String>) -> Self {
        self.extensions.traceparent = Some(tp.into());
        self
    }

    /// Thiết lập W3C Tracestate header thủ công.
    #[must_use]
    pub fn tracestate(mut self, ts: impl Into<String>) -> Self {
        self.extensions.tracestate = Some(ts.into());
        self
    }

    /// Gán một thuộc tính mở rộng tùy biến.
    #[must_use]
    pub fn custom_extension(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extensions.insert_custom(key, value);
        self
    }

    /// Tiêm toàn bộ vết giám sát và ngữ cảnh từ [`omni_common::middleware::RequestContext`].
    #[must_use]
    pub fn with_request_context(mut self, ctx: &omni_common::middleware::RequestContext) -> Self {
        // Tái tạo W3C traceparent từ trace_id, span_id, is_sampled
        let formatted_tp = omni_common::middleware::traceparent::format_traceparent(
            &ctx.trace_id,
            &ctx.span_id,
            ctx.is_sampled,
        );
        self.extensions.traceparent = Some(formatted_tp);

        if let Some(ref ts) = ctx.tracestate {
            self.extensions.tracestate = Some(ts.to_string());
        }

        if let Some(ref tenant) = ctx.tenant_id
            && let Ok(tid) = Uuid::parse_str(tenant)
        {
            self.extensions.tenant_id = Some(tid);
        }

        if let Some(ref user) = ctx.user_id
            && let Ok(uid) = Uuid::parse_str(user)
        {
            self.extensions.actor_id = Some(uid);
        }

        self
    }

    /// Hoàn thiện việc đóng gói và trả về [`CloudEvent<T>`].
    #[must_use]
    pub fn build(self) -> CloudEvent<T> {
        let event_id = self.id.unwrap_or_else(Uuid::now_v7);
        let mut extensions = self.extensions;

        if extensions.idempotency_key.is_none() {
            extensions.idempotency_key = Some(event_id.to_string());
        }

        CloudEvent {
            id: event_id,
            source: self.source,
            specversion: "1.0".to_string(),
            event_type: self.event_type,
            time: self.time.unwrap_or_else(Utc::now),
            datacontenttype: self.datacontenttype.unwrap_or_else(default_content_type),
            dataschema: self.dataschema,
            subject: self.subject,
            data: self.data,
            extensions,
        }
    }
}
