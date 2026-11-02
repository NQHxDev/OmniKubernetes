//! Context bridging to omni-database PostgreSQL `SessionContext`.

#[cfg(feature = "db-session")]
use omni_database::session::{Actor, SessionContext, SpanId, TraceId};

use crate::extractor::AuthenticatedUser;

/// Khởi tạo [`SessionContext`] cho giao dịch cơ sở dữ liệu từ [`AuthenticatedUser`].
#[cfg(feature = "db-session")]
#[must_use]
pub fn create_session_context(
    user: &AuthenticatedUser,
    trace_id: Option<&str>,
    span_id: Option<&str>,
) -> SessionContext {
    SessionContext {
        trace_id: trace_id.and_then(TraceId::parse),
        span_id: span_id.and_then(SpanId::parse),
        actor: Actor::User(user.user_id),
        tenant_id: user.tenant_id,
    }
}
