//! Tests for `DatabaseSettings`, options builder, and URL redaction.

use std::time::Duration;

use omni_common::config::{DatabaseConfig, DbSslMode, ExposeSecret, SecretString};
use omni_database::DatabaseSettings;

#[test]
fn test_database_settings_application_name_normalization() {
    let raw = DatabaseConfig::default();
    let settings = DatabaseSettings::new(raw, "Order_Service_v1@2026!");
    assert_eq!(settings.application_name(), "orderservicev12026");

    let raw2 = DatabaseConfig {
        application_name: Some("custom-app-db".to_string()),
        ..DatabaseConfig::default()
    };
    let settings2 = DatabaseSettings::new(raw2, "ignored-name");
    assert_eq!(settings2.application_name(), "custom-app-db");
}

#[test]
fn test_database_settings_jitter() {
    let raw = DatabaseConfig {
        max_lifetime_secs: 1800,
        max_lifetime_jitter_pct: 10,
        ..DatabaseConfig::default()
    };
    let settings = DatabaseSettings::new(raw, "test-svc");

    // max_lifetime phải nằm trong [1800 * 0.9, 1800]
    let min_allowed = Duration::from_mins(27);
    let max_allowed = Duration::from_mins(30);
    assert!(settings.max_lifetime >= min_allowed);
    assert!(settings.max_lifetime <= max_allowed);
}

#[test]
fn test_url_redaction_and_percent_encoding() {
    let raw = DatabaseConfig {
        username: "user@domain".to_string(),
        password: SecretString::from("p@ss:w/ord#2026?"),
        host: "db.internal".to_string(),
        port: 5432,
        database_name: "prod_db".to_string(),
        ssl_mode: DbSslMode::Require,
        ..DatabaseConfig::default()
    };

    let full_url = raw.connection_url();
    let exposed = full_url.expose_secret();

    assert!(exposed.starts_with("postgres://user%40domain:p%40ss%3Aw%2Ford%232026%3F@db.internal:5432/prod_db?sslmode=require"));

    let redacted = raw.redacted_url();
    assert_eq!(
        redacted,
        "postgres://user%40domain:[REDACTED]@db.internal:5432/prod_db?sslmode=require"
    );
    assert!(!redacted.contains("p@ss"));
}
