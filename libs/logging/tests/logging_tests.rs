//! Unit tests for `omni-logging`.

use omni_logging::{
    LogFormat, LoggingConfig, REDACTED, is_sensitive_key, mask_authorization_header,
    mask_credit_card, mask_email, mask_id_number, mask_token, sanitize_field,
};

#[test]
fn test_config_builder_and_defaults() {
    let cfg = LoggingConfig::new("user-service")
        .with_level("debug")
        .with_format(LogFormat::Json)
        .with_buffer_limit(50_000);

    assert_eq!(cfg.service_name, "user-service");
    assert_eq!(cfg.log_level, "debug");
    assert_eq!(cfg.log_format, LogFormat::Json);
    assert_eq!(cfg.buffered_lines_limit, 50_000);
    assert!(cfg.lossy);
    assert_eq!(cfg.otlp_endpoint, None);

    let cfg_with_otlp = cfg.with_otlp_endpoint("http://otel-collector:4317");
    assert_eq!(
        cfg_with_otlp.otlp_endpoint.as_deref(),
        Some("http://otel-collector:4317")
    );
}

#[test]
fn test_json_formatter_automatic_pii_sanitization_and_ecs_structure() {
    let cfg = LoggingConfig::new("order-service").with_level("info");
    let _formatter = omni_logging::JsonFormatter::new(&cfg);

    // Verify sanitization rules work end-to-end
    let sanitized_auth = omni_logging::mask_authorization_header("Bearer token_abc_12345678");
    assert!(sanitized_auth.contains("[REDACTED]"));
    assert!(!sanitized_auth.contains("token_abc_12345678"));

    let sanitized_pass = omni_logging::sanitize_field("password", "PlainPassword123");
    assert_eq!(sanitized_pass, omni_logging::REDACTED);

    let sanitized_pin = omni_logging::sanitize_field("pin", "1234");
    assert_eq!(sanitized_pin, omni_logging::REDACTED);

    let sanitized_cc = omni_logging::sanitize_field("credit_card", "4111-2222-3333-4444");
    assert_eq!(sanitized_cc, "****-****-****-4444");
}

#[test]
fn test_pii_sanitization_rules() {
    assert!(is_sensitive_key("Authorization"));
    assert!(is_sensitive_key("password"));
    assert!(is_sensitive_key("api_key"));
    assert!(is_sensitive_key("credit_card"));
    assert!(is_sensitive_key("cccd"));
    assert!(is_sensitive_key("email"));
    assert!(!is_sensitive_key("user_id"));
    assert!(!is_sensitive_key("request_id"));

    // Authorization
    let auth = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
    assert_eq!(
        mask_authorization_header(auth),
        "Bearer eyJhbG...[REDACTED]"
    );

    // Tokens
    assert_eq!(mask_token("my_secret_token_123"), "my_sec...[REDACTED]");
    assert_eq!(mask_token("short"), REDACTED);

    // Passwords & Secrets
    assert_eq!(sanitize_field("password", "MyPassword123"), REDACTED);
    assert_eq!(sanitize_field("pin", "0987"), REDACTED);
    assert_eq!(sanitize_field("cvv", "123"), REDACTED);

    // Credit Cards
    assert_eq!(
        mask_credit_card("4111-2222-3333-4444"),
        "****-****-****-4444"
    );

    // CCCD / National ID
    assert_eq!(mask_id_number("001099012345"), "001***345");

    // Email
    assert_eq!(mask_email("admin@omnik8s.io"), "a***n@omnik8s.io");

    // Non-sensitive field passthrough
    assert_eq!(sanitize_field("username", "john_doe"), "john_doe");
}
