//! Context bridging to omni-common `RequestContext` and OpenTelemetry Span attributes.

use omni_common::middleware::RequestContext;
use std::borrow::Cow;

use crate::extractor::AuthenticatedUser;

/// Đồng bộ định danh người dùng vào [`RequestContext`].
pub fn enrich_request_context(ctx: &mut RequestContext, user: &AuthenticatedUser) {
    ctx.user_id = Some(Cow::Owned(user.user_id.to_string()));
    ctx.tenant_id = user.tenant_id.map(|t| Cow::Owned(t.to_string()));
}

/// Đính kèm các trường định danh an toàn (không PII) vào OpenTelemetry active span.
pub fn enrich_tracing_span(user: &AuthenticatedUser) {
    let span = tracing::Span::current();
    span.record("user.id", tracing::field::display(user.user_id));
    if let Some(tenant_id) = user.tenant_id {
        span.record("tenant.id", tracing::field::display(tenant_id));
    }
    span.record("auth.token_id", tracing::field::display(user.token_id));
    span.record("auth.roles_count", user.roles.len() as u64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Role;
    use uuid::Uuid;

    #[test]
    fn test_enrich_request_context_with_tenant() {
        let user_id = Uuid::now_v7();
        let tenant_id = Uuid::now_v7();
        let user = AuthenticatedUser {
            user_id,
            tenant_id: Some(tenant_id),
            session_id: None,
            roles: vec![Role::Admin],
            permissions: vec!["all".to_string()],
            email: None,
            token_id: Uuid::now_v7(),
        };

        let mut ctx = RequestContext::default();
        enrich_request_context(&mut ctx, &user);

        assert_eq!(ctx.user_id.as_deref(), Some(user_id.to_string().as_str()));
        assert_eq!(
            ctx.tenant_id.as_deref(),
            Some(tenant_id.to_string().as_str())
        );
    }

    #[test]
    fn test_enrich_request_context_without_tenant() {
        let user_id = Uuid::now_v7();
        let user = AuthenticatedUser {
            user_id,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
            token_id: Uuid::now_v7(),
        };

        let mut ctx = RequestContext::default();
        enrich_request_context(&mut ctx, &user);

        assert_eq!(ctx.user_id.as_deref(), Some(user_id.to_string().as_str()));
        assert_eq!(ctx.tenant_id, None);
    }

    #[test]
    fn test_enrich_tracing_span_does_not_panic() {
        let user = AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: Some(Uuid::now_v7()),
            session_id: None,
            roles: vec![Role::User, Role::Operator],
            permissions: vec![],
            email: None,
            token_id: Uuid::now_v7(),
        };

        let span = tracing::info_span!("test_span");
        let _guard = span.enter();
        enrich_tracing_span(&user);
    }
}
