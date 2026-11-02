//! Session context and zero-round-trip transaction header injection.
//!
//! Enriches PostgreSQL transactions with distributed tracing context (`trace_id`, `span_id`),
//! actor identity (`actor_id`), tenant identity (`tenant_id`), and server timeouts
//! directly in the multi-statement `BEGIN` command via Simple Query Protocol.

use uuid::Uuid;

use crate::config::DatabaseSettings;
use crate::transaction::{IsolationLevel, TxOptions};

/// Định danh chủ thể thực thi thao tác dữ liệu.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Actor {
    /// Người dùng xác thực thông qua hệ thống.
    User(Uuid),
    /// Tác vụ nền hoặc microservice nội bộ với UUID tất định.
    Service(Uuid),
    /// Khách truy cập hoặc tiến trình không định danh.
    #[default]
    Anonymous,
}

impl Actor {
    /// Trả về [`Uuid`] của chủ thể nếu có.
    #[must_use]
    pub fn id(&self) -> Option<Uuid> {
        match self {
            Self::User(id) | Self::Service(id) => Some(*id),
            Self::Anonymous => None,
        }
    }

    /// Khởi tạo actor dạng Service với `UUIDv5` tất định từ tên dịch vụ.
    #[must_use]
    pub fn service(service_name: &str) -> Self {
        Self::Service(Uuid::new_v5(&Uuid::NAMESPACE_DNS, service_name.as_bytes()))
    }
}

/// Định danh distributed trace W3C chuẩn (32 ký tự hex, không phải toàn 0).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraceId(String);

impl TraceId {
    /// Phân tích chuỗi 32 hex thành [`TraceId`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.len() == 32
            && trimmed.chars().all(|c| c.is_ascii_hexdigit())
            && !trimmed.chars().all(|c| c == '0')
        {
            Some(Self(trimmed.to_ascii_lowercase()))
        } else {
            None
        }
    }

    /// Trả về chuỗi 32 ký tự hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Định danh span W3C chuẩn (16 ký tự hex, không phải toàn 0).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpanId(String);

impl SpanId {
    /// Phân tích chuỗi 16 hex thành [`SpanId`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.len() == 16
            && trimmed.chars().all(|c| c.is_ascii_hexdigit())
            && !trimmed.chars().all(|c| c == '0')
        {
            Some(Self(trimmed.to_ascii_lowercase()))
        } else {
            None
        }
    }

    /// Trả về chuỗi 16 ký tự hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ngữ cảnh phiên làm việc được truyền tường minh qua từng transaction/operation.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    /// Distributed trace ID.
    pub trace_id: Option<TraceId>,
    /// Span ID hiện tại.
    pub span_id: Option<SpanId>,
    /// Chủ thể thực thi.
    pub actor: Actor,
    /// Định danh khách hàng doanh nghiệp (Tenant ID).
    pub tenant_id: Option<Uuid>,
}

impl SessionContext {
    /// Dựng ngữ cảnh phiên từ HTTP [`omni_common::middleware::RequestContext`].
    #[must_use]
    pub fn from_request(ctx: &omni_common::middleware::RequestContext) -> Self {
        Self {
            trace_id: TraceId::parse(&ctx.trace_id),
            span_id: SpanId::parse(&ctx.span_id),
            actor: ctx
                .user_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
                .map_or(Actor::Anonymous, Actor::User),
            tenant_id: ctx
                .tenant_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok()),
        }
    }

    /// Dựng ngữ cảnh phiên cho tác vụ nền / consumer với actor là service account.
    #[must_use]
    pub fn system(service_name: &str) -> Self {
        Self {
            trace_id: None,
            span_id: None,
            actor: Actor::service(service_name),
            tenant_id: None,
        }
    }
}

/// Tạo câu lệnh `BEGIN` đa tầng kết hợp cấu hình phiên trong đúng 1 round-trip.
///
/// Các giá trị được kiểm tra kiểu tĩnh tuyệt đối (enum, Uuid, Duration, `TraceId`)
/// đảm bảo tính an toàn chống SQL injection.
#[must_use]
pub fn build_begin_statement(
    opts: &TxOptions,
    session: &SessionContext,
    settings: &DatabaseSettings,
) -> String {
    use std::fmt::Write as _;

    let mut sql = String::with_capacity(256);
    sql.push_str("BEGIN ISOLATION LEVEL ");
    sql.push_str(opts.isolation.as_sql());

    if opts.read_only {
        sql.push_str(" READ ONLY");
        if opts.deferrable && opts.isolation == IsolationLevel::Serializable {
            sql.push_str(" DEFERRABLE");
        }
    }
    sql.push(';');

    if let Some(t) = opts.statement_timeout {
        let _ = write!(sql, " SET LOCAL statement_timeout = {};", t.as_millis());
    }

    if let Some(t) = opts.lock_timeout {
        let _ = write!(sql, " SET LOCAL lock_timeout = {};", t.as_millis());
    }

    if settings.raw.trace_correlation != omni_common::config::TraceCorrelation::Off
        && let Some(trace) = &session.trace_id
    {
        let app = settings.application_name();
        let _ = write!(
            sql,
            " SET LOCAL application_name = '{app}:{}';",
            trace.as_str()
        );
        let _ = write!(sql, " SET LOCAL omni.trace_id = '{}';", trace.as_str());
    }

    if let Some(actor_id) = session.actor.id() {
        let _ = write!(sql, " SET LOCAL omni.actor_id = '{actor_id}';");
    }

    if let Some(tenant_id) = session.tenant_id {
        let _ = write!(sql, " SET LOCAL omni.tenant_id = '{tenant_id}';");
    }

    sql
}
