//! Tests for Id<T> strongly-typed identifier, `AuditColumns`, and Optimistic Locking.

use omni_database::{AuditColumns, DbError, Id, check_optimistic_lock};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
struct Order;

#[derive(Debug, PartialEq, Eq)]
struct _Customer;

#[test]
fn test_id_type_safety_and_serialization() {
    let order_id = Id::<Order>::new();
    let order_id_str = order_id.to_string();

    let parsed: Id<Order> = order_id_str.parse().expect("valid UUID must parse");
    assert_eq!(order_id, parsed);

    // JSON serialization round-trip
    let json = serde_json::to_string(&order_id).expect("Id must serialize to json string");
    assert_eq!(json, format!("\"{order_id_str}\""));

    let deserialized: Id<Order> =
        serde_json::from_str(&json).expect("Id must deserialize from json string");
    assert_eq!(order_id, deserialized);
}

#[test]
fn test_audit_columns_lifecycle() {
    let actor_id = Uuid::now_v7();
    let mut audit = AuditColumns::new_initial(Some(actor_id));

    assert_eq!(audit.version, 1);
    assert_eq!(audit.created_by, Some(actor_id));
    assert_eq!(audit.updated_by, Some(actor_id));
    assert!(!audit.is_deleted());
    assert!(audit.deleted_at.is_none());

    let updater_id = Uuid::now_v7();
    audit.touch(Some(updater_id));
    assert_eq!(audit.version, 2);
    assert_eq!(audit.updated_by, Some(updater_id));

    let deleter_id = Uuid::now_v7();
    audit.mark_deleted(Some(deleter_id));
    assert_eq!(audit.version, 3);
    assert_eq!(audit.deleted_by, Some(deleter_id));
    assert!(audit.is_deleted());
}

#[test]
fn test_optimistic_locking_check() {
    // 1 row updated -> success
    assert!(check_optimistic_lock(1, "orders").is_ok());

    // 0 rows updated -> concurrent modification conflict
    let err = check_optimistic_lock(0, "orders");
    assert!(err.is_err());
    match err.unwrap_err() {
        DbError::ConcurrentModification { entity } => assert_eq!(entity, "orders"),
        other => panic!("expected ConcurrentModification, got {other:?}"),
    }
}
