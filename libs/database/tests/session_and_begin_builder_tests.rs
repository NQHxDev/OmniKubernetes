//! Tests for Actor, `SessionContext`, and Zero-Round-Trip BEGIN Statement Builder.

use std::time::Duration;

use omni_common::config::{DatabaseConfig, TraceCorrelation};
use omni_common::middleware::RequestContext;
use omni_database::{
    Actor, DatabaseSettings, IsolationLevel, SessionContext, SpanId, TraceId, TxOptions,
    build_begin_statement,
};
use uuid::Uuid;

#[test]
fn test_actor_service_deterministic_uuidv5() {
    let actor1 = Actor::service("order-service");
    let actor2 = Actor::service("order-service");
    let actor3 = Actor::service("payment-service");

    assert_eq!(actor1, actor2);
    assert_ne!(actor1, actor3);
    assert!(actor1.id().is_some());
    assert_eq!(Actor::Anonymous.id(), None);
}

#[test]
fn test_trace_id_and_span_id_validation() {
    let valid_trace = "4bf92f3577b34da6a3ce929d0e0e4736";
    let trace = TraceId::parse(valid_trace).expect("valid 32-hex trace_id must parse");
    assert_eq!(trace.as_str(), valid_trace);

    // Bác bỏ chuỗi toàn số 0 theo W3C Trace Context
    assert!(TraceId::parse("00000000000000000000000000000000").is_none());
    // Bác bỏ độ dài sai
    assert!(TraceId::parse("4bf92f3577b34da6").is_none());
    // Bác bỏ ký tự không phải hex
    assert!(TraceId::parse("4bf92f3577b34da6a3ce929d0e0e473g").is_none());

    let valid_span = "00f067aa0ba902b7";
    let span = SpanId::parse(valid_span).expect("valid 16-hex span_id must parse");
    assert_eq!(span.as_str(), valid_span);

    assert!(SpanId::parse("0000000000000000").is_none());
    assert!(SpanId::parse("00f067aa").is_none());
}

#[test]
fn test_session_context_from_request() {
    let user_id = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let mut ctx = RequestContext::new(
        "0192d4b8-f2e1-7e83-a4c3-b8162e24cf81",
        "4bf92f3577b34da6a3ce929d0e0e4736",
        "vi-VN",
        "/orders",
    );
    ctx.span_id = std::borrow::Cow::Borrowed("00f067aa0ba902b7");
    ctx.user_id = Some(std::borrow::Cow::Owned(user_id.to_string()));
    ctx.tenant_id = Some(std::borrow::Cow::Owned(tenant_id.to_string()));

    let session = SessionContext::from_request(&ctx);
    assert_eq!(
        session.trace_id.as_ref().map(TraceId::as_str),
        Some("4bf92f3577b34da6a3ce929d0e0e4736")
    );
    assert_eq!(
        session.span_id.as_ref().map(SpanId::as_str),
        Some("00f067aa0ba902b7")
    );
    assert_eq!(session.actor, Actor::User(user_id));
    assert_eq!(session.tenant_id, Some(tenant_id));
}

#[test]
fn test_build_begin_statement_full_context() {
    let raw = DatabaseConfig {
        trace_correlation: TraceCorrelation::Transaction,
        ..DatabaseConfig::default()
    };
    let settings = DatabaseSettings::new(raw, "order-service");

    let actor_id = Uuid::parse_str("0192d4b8-f2e1-7e83-a4c3-b8162e24cf81").unwrap();
    let tenant_id = Uuid::parse_str("0192d4b8-f2e1-7e83-a4c3-b8162e24cf82").unwrap();
    let session = SessionContext {
        trace_id: TraceId::parse("4bf92f3577b34da6a3ce929d0e0e4736"),
        span_id: SpanId::parse("00f067aa0ba902b7"),
        actor: Actor::User(actor_id),
        tenant_id: Some(tenant_id),
    };

    let opts = TxOptions {
        isolation: IsolationLevel::ReadCommitted,
        read_only: false,
        deferrable: false,
        statement_timeout: Some(Duration::from_millis(5000)),
        lock_timeout: Some(Duration::from_millis(3000)),
        ..Default::default()
    };

    let sql = build_begin_statement(&opts, &session, &settings);

    assert!(sql.starts_with("BEGIN ISOLATION LEVEL READ COMMITTED;"));
    assert!(sql.contains("SET LOCAL statement_timeout = 5000;"));
    assert!(sql.contains("SET LOCAL lock_timeout = 3000;"));
    assert!(sql.contains(
        "SET LOCAL application_name = 'order-service:4bf92f3577b34da6a3ce929d0e0e4736';"
    ));
    assert!(sql.contains("SET LOCAL omni.trace_id = '4bf92f3577b34da6a3ce929d0e0e4736';"));
    assert!(sql.contains(&format!("SET LOCAL omni.actor_id = '{actor_id}';")));
    assert!(sql.contains(&format!("SET LOCAL omni.tenant_id = '{tenant_id}';")));
}

#[test]
fn test_build_begin_statement_serializable_read_only_deferrable() {
    let settings = DatabaseSettings::new(DatabaseConfig::default(), "report-service");
    let session = SessionContext::default();
    let opts = TxOptions {
        isolation: IsolationLevel::Serializable,
        read_only: true,
        deferrable: true,
        statement_timeout: None,
        lock_timeout: None,
        ..Default::default()
    };

    let sql = build_begin_statement(&opts, &session, &settings);
    assert_eq!(
        sql,
        "BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY DEFERRABLE;"
    );
}
