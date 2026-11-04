//! Context bridge tests for `RequestContext` and PostgreSQL `SessionContext`.

use omni_auth::{bridge::enrich_request_context, extractor::AuthenticatedUser, token::Role};
use omni_common::middleware::RequestContext;
use uuid::Uuid;

#[test]
fn test_enrich_request_context() {
    let mut ctx = RequestContext::new(
        "req-123",
        "00-4bf92f3577b34da6a3ce929d0e0e4736",
        "vi-VN",
        "/api/v1/orders",
    );
    assert!(ctx.user_id.is_none());
    assert!(ctx.tenant_id.is_none());

    let user_id = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let user = AuthenticatedUser {
        user_id,
        tenant_id: Some(tenant_id),
        session_id: Some(Uuid::now_v7()),
        roles: vec![Role::User],
        permissions: vec!["orders:create".to_string()],
        email: Some("user@example.com".to_string()),
        token_id: Uuid::now_v7(),
    };

    enrich_request_context(&mut ctx, &user);
    assert_eq!(ctx.user_id.as_deref(), Some(user_id.to_string().as_str()));
    assert_eq!(
        ctx.tenant_id.as_deref(),
        Some(tenant_id.to_string().as_str())
    );
}

#[cfg(feature = "db-session")]
#[test]
fn test_create_session_context_bridge() {
    use omni_auth::bridge::create_session_context;
    use omni_database::session::{Actor, SpanId, TraceId};

    let user_id = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let user = AuthenticatedUser {
        user_id,
        tenant_id: Some(tenant_id),
        session_id: None,
        roles: vec![Role::Admin],
        permissions: Vec::new(),
        email: None,
        token_id: Uuid::now_v7(),
    };

    let session = create_session_context(
        &user,
        Some("4bf92f3577b34da6a3ce929d0e0e4736"),
        Some("00f067aa0ba902b7"),
    );

    assert_eq!(session.actor, Actor::User(user_id));
    assert_eq!(session.tenant_id, Some(tenant_id));
    assert_eq!(
        session.trace_id.as_ref().map(TraceId::as_str),
        Some("4bf92f3577b34da6a3ce929d0e0e4736")
    );
    assert_eq!(
        session.span_id.as_ref().map(SpanId::as_str),
        Some("00f067aa0ba902b7")
    );
}
