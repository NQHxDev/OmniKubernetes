//! Tests for error taxonomy, SQLSTATE classification, and `AppError` mapping.

use omni_database::{DbError, Retryable, SqlState};
use omni_errors::AppError;

#[test]
fn test_sqlstate_parsing_and_classification() {
    let state = SqlState::parse("23505").expect("23505 must be valid SQLSTATE");
    assert_eq!(state, SqlState::UNIQUE_VIOLATION);
    assert_eq!(state.class(), *b"23");
    assert_eq!(state.as_str(), "23505");
    assert_eq!(format!("{state}"), "23505");

    let dead = SqlState::parse("40P01").expect("40P01 must be valid SQLSTATE");
    assert_eq!(dead, SqlState::DEADLOCK_DETECTED);
    assert_eq!(dead.class(), *b"40");

    assert!(SqlState::parse("").is_none());
    assert!(SqlState::parse("2350").is_none());
    assert!(SqlState::parse("235055").is_none());
    assert!(SqlState::parse("235!5").is_none());
}

#[test]
fn test_dberror_kind_str() {
    assert_eq!(DbError::NotFound.kind_str(), "not_found");
    assert_eq!(
        DbError::UniqueViolation { constraint: None }.kind_str(),
        "unique_violation"
    );
    assert_eq!(
        DbError::ForeignKeyViolation { constraint: None }.kind_str(),
        "foreign_key_violation"
    );
    assert_eq!(
        DbError::ConcurrentModification { entity: "order" }.kind_str(),
        "concurrent_modification"
    );
    assert_eq!(
        DbError::SerializationFailure(SqlState::SERIALIZATION_FAILURE).kind_str(),
        "serialization_failure"
    );
    assert_eq!(DbError::LockNotAvailable.kind_str(), "lock_not_available");
    assert_eq!(DbError::QueryCanceled.kind_str(), "query_canceled");
    assert_eq!(DbError::PoolExhausted.kind_str(), "pool_exhausted");
}

#[test]
fn test_retryable_trait() {
    let ser_err = DbError::SerializationFailure(SqlState::SERIALIZATION_FAILURE);
    assert!(ser_err.is_transient());
    assert_eq!(ser_err.retry_reason(), "serialization_failure");

    let dead_err = DbError::SerializationFailure(SqlState::DEADLOCK_DETECTED);
    assert!(dead_err.is_transient());
    assert_eq!(dead_err.retry_reason(), "deadlock_detected");

    let not_found = DbError::NotFound;
    assert!(!not_found.is_transient());

    let unique = DbError::UniqueViolation {
        constraint: Some("uq_email".into()),
    };
    assert!(!unique.is_transient());
}

#[test]
fn test_app_error_conversion_and_security_redaction() {
    // 1. NotFound -> AppError::NotFound (404)
    let err: AppError = DbError::NotFound.into();
    assert!(matches!(err, AppError::NotFound));

    // 2. UniqueViolation -> AppError::Conflict (409) without leaking constraint name
    let err: AppError = DbError::UniqueViolation {
        constraint: Some("users_email_key".into()),
    }
    .into();
    assert!(matches!(err, AppError::Conflict { detail: None }));

    // 3. ForeignKeyViolation -> AppError::Conflict (409)
    let err: AppError = DbError::ForeignKeyViolation {
        constraint: Some("orders_customer_id_fkey".into()),
    }
    .into();
    assert!(matches!(err, AppError::Conflict { detail: None }));

    // 4. ConcurrentModification -> AppError::Conflict (409)
    let err: AppError = DbError::ConcurrentModification { entity: "order" }.into();
    assert!(matches!(err, AppError::Conflict { detail: None }));

    // 5. IntegrityViolation -> AppError::BadRequest (400)
    let err: AppError = DbError::IntegrityViolation {
        constraint: Some("check_positive_amount".into()),
        sqlstate: SqlState::CHECK_VIOLATION,
    }
    .into();
    assert!(matches!(err, AppError::BadRequest { detail: None }));

    // 6. LockNotAvailable, QueryCanceled, PoolExhausted -> AppError::ServiceUnavailable (503)
    let err: AppError = DbError::LockNotAvailable.into();
    assert!(matches!(err, AppError::ServiceUnavailable));

    let err: AppError = DbError::QueryCanceled.into();
    assert!(matches!(err, AppError::ServiceUnavailable));

    let err: AppError = DbError::PoolExhausted.into();
    assert!(matches!(err, AppError::ServiceUnavailable));

    // 7. Internal -> AppError::Internal (500)
    let err: AppError = DbError::Configuration("bad setting".into()).into();
    assert!(matches!(err, AppError::Internal(_)));
}
