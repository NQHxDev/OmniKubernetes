use std::time::Duration;

use omni_messaging::{
    BrokerConnectionConfig, BrokerType, CleanupStrategy, ConsumerConfig, MessagingConfig,
    OutboxConfig, OutboxStatus,
};
use secrecy::SecretString;

#[test]
fn test_outbox_status_string_representation() {
    assert_eq!(OutboxStatus::Pending.as_str(), "PENDING");
    assert_eq!(OutboxStatus::Processing.as_str(), "PROCESSING");
    assert_eq!(OutboxStatus::Published.as_str(), "PUBLISHED");
    assert_eq!(OutboxStatus::Failed.as_str(), "FAILED");
    assert_eq!(OutboxStatus::Dead.as_str(), "DEAD");

    assert_eq!(OutboxStatus::from("PENDING"), OutboxStatus::Pending);
    assert_eq!(OutboxStatus::from("PROCESSING"), OutboxStatus::Processing);
    assert_eq!(OutboxStatus::from("PUBLISHED"), OutboxStatus::Published);
    assert_eq!(OutboxStatus::from("FAILED"), OutboxStatus::Failed);
    assert_eq!(OutboxStatus::from("DEAD"), OutboxStatus::Dead);
}

#[test]
fn test_config_guardrails_valid_default() {
    let config = MessagingConfig::default();
    assert!(config.validate_guardrails().is_ok());
}

#[test]
fn test_config_guardrails_rabbitmq_url_validation() {
    let mut config = MessagingConfig {
        broker_type: BrokerType::Rabbitmq,
        broker: BrokerConnectionConfig {
            url: SecretString::from("http://localhost:5672"), // Invalid protocol for RabbitMQ!
            ..Default::default()
        },
        ..Default::default()
    };

    let err = config.validate_guardrails().unwrap_err();
    assert!(
        err.to_string()
            .contains("RabbitMQ broker URL must start with 'amqp://' or 'amqps://'")
    );

    config.broker.url = SecretString::from("amqp://guest:guest@localhost:5672/%2f");
    assert!(config.validate_guardrails().is_ok());
}

#[test]
fn test_config_guardrails_backoff_invariant() {
    let config = MessagingConfig {
        consumer: ConsumerConfig {
            initial_retry_backoff: Duration::from_secs(10),
            max_retry_backoff: Duration::from_secs(1), // Invalid: initial > max
            ..Default::default()
        },
        ..Default::default()
    };

    let err = config.validate_guardrails().unwrap_err();
    assert!(err.to_string().contains("initial_retry_backoff"));
}

#[test]
fn test_config_guardrails_lock_duration_vs_poll_interval() {
    let config = MessagingConfig {
        outbox: OutboxConfig {
            poll_interval: Duration::from_secs(2),
            lock_duration: Duration::from_secs(3), // Invalid: lock_duration must be >= 3 * poll_interval (6s)
            ..Default::default()
        },
        ..Default::default()
    };

    let err = config.validate_guardrails().unwrap_err();
    assert!(err.to_string().contains("lock_duration"));
}

#[test]
fn test_cleanup_strategy_variants() {
    assert_eq!(
        CleanupStrategy::default(),
        CleanupStrategy::RetentionPruning
    );
}
