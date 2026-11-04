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

#[cfg(all(test, feature = "db-session"))]
mod tests {
    use super::*;
    use crate::token::Role;
    use uuid::Uuid;

    #[test]
    fn test_create_session_context_with_traces() {
        let user_id = Uuid::now_v7();
        let tenant_id = Uuid::now_v7();
        let user = AuthenticatedUser {
            user_id,
            tenant_id: Some(tenant_id),
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
            token_id: Uuid::now_v7(),
        };

        let valid_trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";
        let valid_span_id = "00f067aa0ba902b7";

        let session_ctx = create_session_context(&user, Some(valid_trace_id), Some(valid_span_id));
        assert_eq!(session_ctx.actor, Actor::User(user_id));
        assert_eq!(session_ctx.tenant_id, Some(tenant_id));
        assert!(session_ctx.trace_id.is_some());
        assert!(session_ctx.span_id.is_some());

        // Without trace/span IDs
        let simple_ctx = create_session_context(&user, None, None);
        assert_eq!(simple_ctx.actor, Actor::User(user_id));
        assert_eq!(simple_ctx.tenant_id, Some(tenant_id));
        assert!(simple_ctx.trace_id.is_none());
        assert!(simple_ctx.span_id.is_none());
    }
}
