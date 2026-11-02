//! Enterprise Type-Safe Configuration & Environment Binding Tests.
//!
//! Validates the 5-layer hierarchical resolution, Fail-Fast validation,
//! double-underscore mapping, zero-leak secrets, logging bridge, and custom domain extension.

use omni_common::config::{
    AppConfig, AppEnvironment, ConfigError, ConfigLoader, DbSslMode, ExposeSecret,
};
use serde::Deserialize;
use validator::Validate;

#[test]
fn test_hierarchical_5_layer_precedence() {
    // 1. Compiled defaults & Base YAML: port is 8080 by default in base.yaml.
    // 2. Dev profile overrides log_level to "debug" and log_format to Pretty.
    let dev_config: AppConfig = ConfigLoader::for_test("order-service")
        .with_environment(AppEnvironment::Dev)
        .load()
        .expect("Dev configuration must load cleanly");

    assert_eq!(dev_config.service.name, "order-service");
    assert_eq!(dev_config.service.environment, AppEnvironment::Dev);
    assert_eq!(dev_config.server.port, 8080);
    assert_eq!(dev_config.logging.log_level, "debug");
    assert_eq!(
        dev_config.logging.log_format,
        omni_logging::LogFormat::Pretty
    );

    // 3. Env var layer overrides Dev profile log_level to "warn"
    let env_override_config: AppConfig = ConfigLoader::for_test("order-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([("OMNI__LOGGING__LOG_LEVEL", "warn")])
        .load()
        .expect("Config with env override must load cleanly");

    assert_eq!(env_override_config.logging.log_level, "warn");

    // 4. Programmatic override (Layer 5) overrides env var to "error"
    let final_config: AppConfig = ConfigLoader::for_test("order-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([("OMNI__LOGGING__LOG_LEVEL", "warn")])
        .with_override("logging.log_level", "error")
        .load()
        .expect("Config with programmatic override must load cleanly");

    assert_eq!(final_config.logging.log_level, "error");
}

#[test]
fn test_double_underscore_and_snake_case_parsing() {
    let config: AppConfig = ConfigLoader::for_test("payment-service")
        .with_environment(AppEnvironment::Staging)
        .with_env_vars([
            ("OMNI__SERVER__PORT", "9090"),
            ("OMNI__SERVER__SHUTDOWN_TIMEOUT_SECS", "45"),
            ("OMNI__DATABASE__MAX_CONNECTIONS", "64"),
            ("OMNI__DATABASE__MIN_CONNECTIONS", "8"),
            ("OMNI__DATABASE__ACQUIRE_TIMEOUT_SECS", "4"),
            ("OMNI__LOGGING__BUFFERED_LINES_LIMIT", "25000"),
            ("OMNI__DATABASE__SSL_MODE", "require"),
        ])
        .load()
        .expect("Double underscore parsing must succeed");

    assert_eq!(config.server.port, 9090);
    assert_eq!(config.server.shutdown_timeout_secs, 45);
    assert_eq!(config.database.max_connections, 64);
    assert_eq!(config.database.min_connections, 8);
    assert_eq!(config.database.acquire_timeout_secs, 4);
    assert_eq!(config.logging.buffered_lines_limit, 25000);
    assert_eq!(config.database.ssl_mode, DbSslMode::Require);
}

#[test]
fn test_cloud_native_alias_and_k8s_downward_api_fallbacks() {
    let config: AppConfig = ConfigLoader::for_test("inventory-service")
        .with_env_vars([
            ("APP_ENV", "staging"),
            ("RUST_LOG", "trace"),
            (
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://otel-collector.observability:4317",
            ),
            ("K8S_POD_NAME", "inventory-service-7d4f9b-w9k21"),
            ("K8S_NAMESPACE", "omni-commerce"),
            ("K8S_NODE_NAME", "k8s-worker-pool-03"),
        ])
        .load()
        .expect("Cloud-native fallback mapping must succeed");

    assert_eq!(config.service.environment, AppEnvironment::Staging);
    assert_eq!(config.logging.log_level, "trace");
    assert_eq!(
        config.logging.otlp_endpoint.as_deref(),
        Some("http://otel-collector.observability:4317")
    );
    assert_eq!(
        config.k8s.pod_name.as_deref(),
        Some("inventory-service-7d4f9b-w9k21")
    );
    assert_eq!(config.k8s.namespace.as_deref(), Some("omni-commerce"));
    assert_eq!(config.k8s.node_name.as_deref(), Some("k8s-worker-pool-03"));
}

#[test]
fn test_fail_fast_multi_error_reporting_with_hints() {
    let result: Result<AppConfig, ConfigError> = ConfigLoader::for_test("faulty-service")
        .with_environment(AppEnvironment::Prod)
        .with_env_vars([
            // Lỗi 1: Cổng đặc quyền (< 1024)
            ("OMNI__SERVER__PORT", "80"),
            // Lỗi 2: database pool vi phạm min > max
            ("OMNI__DATABASE__ENABLED", "true"),
            ("OMNI__DATABASE__MIN_CONNECTIONS", "50"),
            ("OMNI__DATABASE__MAX_CONNECTIONS", "20"),
            // Lỗi 3: Timeout hierarchy vi phạm acquire > request
            ("OMNI__SERVER__REQUEST_TIMEOUT_SECS", "10"),
            ("OMNI__DATABASE__ACQUIRE_TIMEOUT_SECS", "30"),
            // Lỗi 4: Swagger UI bật trên Production
            ("OMNI__OBSERVABILITY__SWAGGER_UI_ENABLED", "true"),
        ])
        .load();

    match result {
        Ok(_) => panic!("Loader must fail-fast with invalid configurations"),
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.len() >= 4,
                "Expected at least 4 validation errors, got {}",
                issues.len()
            );

            // Kiểm tra sự hiện diện của các mã lỗi
            let codes: Vec<&str> = issues.iter().map(|i| i.code.as_str()).collect();
            assert!(codes.contains(&"range"));
            assert!(codes.contains(&"CFG_DB_POOL_BOUNDS"));
            assert!(codes.contains(&"CFG_TIMEOUT_HIERARCHY"));
            assert!(codes.contains(&"CFG_PROD_SWAGGER_UI_DISABLED"));

            // Kiểm tra các gợi ý biến môi trường OMNI__*
            let hints: Vec<&str> = issues.iter().map(|i| i.env_var_hint.as_str()).collect();
            assert!(hints.contains(&"OMNI__SERVER__PORT"));
            assert!(hints.contains(&"OMNI__DATABASE__MIN_CONNECTIONS"));
            assert!(hints.contains(&"OMNI__DATABASE__ACQUIRE_TIMEOUT_SECS"));
            assert!(hints.contains(&"OMNI__OBSERVABILITY__SWAGGER_UI_ENABLED"));

            // Đảm bảo thông điệp lỗi in ra đầy đủ
            let formatted = format!("{}", ConfigError::ValidationFailed(issues));
            assert!(formatted.contains("OMNI__SERVER__PORT"));
            assert!(formatted.contains("CFG_DB_POOL_BOUNDS"));
        }
        Err(other) => panic!("Expected ValidationFailed error, got: {other:?}"),
    }
}

#[test]
fn test_security_zero_leak_secrets_and_redaction() {
    let secret_db_pass = "super_secret_db_pass_2026_xyz";
    let secret_redis_pass = "redis_auth_top_secret_token";
    let secret_jwt = "prod_jwt_secret_signing_key_at_least_32_bytes_long";
    let secret_broker = "amqp://app_user:app_pass_9988@rabbitmq.broker.svc:5672/%2f";

    let config: AppConfig = ConfigLoader::for_test("auth-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([
            ("OMNI__DATABASE__PASSWORD", secret_db_pass),
            ("OMNI__REDIS__PASSWORD", secret_redis_pass),
            ("OMNI__AUTH__JWT_SECRET", secret_jwt),
            ("OMNI__MESSAGING__BROKER_URL", secret_broker),
        ])
        .load()
        .expect("Configuration with secrets must load cleanly");

    // 1. fmt::Debug không bao giờ để lộ mật khẩu
    let debug_repr = format!("{config:?}");
    assert!(!debug_repr.contains(secret_db_pass));
    assert!(!debug_repr.contains(secret_redis_pass));
    assert!(!debug_repr.contains(secret_jwt));
    assert!(!debug_repr.contains("app_pass_9988"));

    // 2. Redacted URL methods
    let db_redacted = config.database.redacted_url();
    assert!(db_redacted.contains("[REDACTED]"));
    assert!(!db_redacted.contains(secret_db_pass));

    let redis_redacted = config.redis.redacted_url();
    assert!(redis_redacted.contains("[REDACTED]"));
    assert!(!redis_redacted.contains(secret_redis_pass));

    let broker_redacted = config.messaging.redacted_broker_url();
    assert!(broker_redacted.contains("[REDACTED]"));
    assert!(!broker_redacted.contains("app_pass_9988"));

    // 3. Startup Diagnostic Summary không để lộ mật khẩu
    let summary = config.startup_summary();
    let summary_str = summary.to_string();
    assert!(!summary_str.contains(secret_db_pass));
    assert!(!summary_str.contains(secret_redis_pass));
    assert!(!summary_str.contains(secret_jwt));
    assert!(!summary_str.contains("app_pass_9988"));
    assert!(summary_str.contains("[REDACTED]"));

    // 4. Giá trị thật chỉ được lấy khi gọi tường minh expose_secret()
    assert_eq!(config.database.password.expose_secret(), secret_db_pass);
    assert_eq!(
        config
            .redis
            .password
            .as_ref()
            .map(ExposeSecret::expose_secret),
        Some(secret_redis_pass)
    );
    assert_eq!(config.auth.jwt_secret.expose_secret(), secret_jwt);
}

#[test]
fn test_production_guardrails_cors_and_jwt_and_passwords() {
    // 1. Prod từ chối Wildcard CORS khi allow_credentials = true
    let cors_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("prod-service")
        .with_environment(AppEnvironment::Prod)
        .with_override("server.cors.allow_origins", vec!["*".to_string()])
        .with_override("server.cors.allow_credentials", true)
        .load();

    match cors_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.iter().any(|i| i.code == "CFG_PROD_CORS_STRICT"),
                "Must reject wildcard origin with allow_credentials in Prod"
            );
        }
        other => panic!("Expected CFG_PROD_CORS_STRICT, got: {other:?}"),
    }

    // 2. Prod từ chối mật khẩu mặc định khi database bật
    let db_pass_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("prod-service")
        .with_environment(AppEnvironment::Prod)
        .with_env_vars([
            ("OMNI__DATABASE__ENABLED", "true"),
            ("OMNI__DATABASE__PASSWORD", "postgres"),
        ])
        .load();

    match db_pass_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.iter().any(|i| i.code == "CFG_PROD_SECRET_REQUIRED"),
                "Must reject default database password in Prod"
            );
        }
        other => panic!("Expected CFG_PROD_SECRET_REQUIRED, got: {other:?}"),
    }

    // 3. auth bật nhưng jwt_secret < 32 bytes
    let jwt_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("auth-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([
            ("OMNI__AUTH__ENABLED", "true"),
            ("OMNI__AUTH__JWT_SECRET", "short_secret_key"),
        ])
        .load();

    match jwt_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.iter().any(|i| i.code == "CFG_PROD_JWT_ENTROPY"),
                "Must reject JWT secret shorter than 32 bytes"
            );
        }
        other => panic!("Expected CFG_PROD_JWT_ENTROPY, got: {other:?}"),
    }

    // 4. i18n default locale không nằm trong supported locales
    let i18n_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("i18n-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([("OMNI__I18N__DEFAULT_LOCALE", "fr-FR")])
        .load();

    match i18n_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues
                    .iter()
                    .any(|i| i.code == "CFG_I18N_DEFAULT_IN_SUPPORTED"),
                "Must reject default locale not present in supported locales"
            );
        }
        other => panic!("Expected CFG_I18N_DEFAULT_IN_SUPPORTED, got: {other:?}"),
    }
}

#[test]
fn test_logging_config_bridge_and_conversion() {
    let config: AppConfig = ConfigLoader::for_test("notification-service")
        .with_environment(AppEnvironment::Staging)
        .with_env_vars([
            ("OMNI__LOGGING__LOG_LEVEL", "debug"),
            ("OMNI__LOGGING__BUFFERED_LINES_LIMIT", "30000"),
            (
                "OMNI__LOGGING__OTLP_ENDPOINT",
                "http://otel.monitoring.svc:4317",
            ),
        ])
        .load()
        .expect("Configuration must load cleanly");

    let logging_config = config.to_logging_config();

    assert_eq!(logging_config.service_name, "notification-service");
    assert_eq!(logging_config.environment, "staging");
    assert_eq!(logging_config.log_level, "debug");
    assert_eq!(logging_config.buffered_lines_limit, 30000);
    assert_eq!(
        logging_config.otlp_endpoint.as_deref(),
        Some("http://otel.monitoring.svc:4317")
    );
}

/// Struct cấu hình nghiệp vụ mở rộng đặc thù của Order Service.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(default)]
struct OrderDomainConfig {
    #[validate(range(min = 5, max = 1440))]
    pub reservation_ttl_minutes: u32,

    #[validate(range(min = 1, max = 100))]
    pub max_items_per_order: usize,
}

impl Default for OrderDomainConfig {
    fn default() -> Self {
        Self {
            reservation_ttl_minutes: 15,
            max_items_per_order: 50,
        }
    }
}

#[test]
fn test_custom_service_config_extension() {
    // 1. Nạp thành công cấu hình mở rộng mặc định
    let config: AppConfig<OrderDomainConfig> = ConfigLoader::for_test("order-service")
        .with_environment(AppEnvironment::Dev)
        .load()
        .expect("AppConfig with OrderDomainConfig must load cleanly");

    assert_eq!(config.custom.reservation_ttl_minutes, 15);
    assert_eq!(config.custom.max_items_per_order, 50);

    // 2. Ghi đè cấu hình mở rộng qua biến môi trường OMNI__CUSTOM__*
    let custom_override_config: AppConfig<OrderDomainConfig> =
        ConfigLoader::for_test("order-service")
            .with_environment(AppEnvironment::Dev)
            .with_env_vars([
                ("OMNI__CUSTOM__RESERVATION_TTL_MINUTES", "45"),
                ("OMNI__CUSTOM__MAX_ITEMS_PER_ORDER", "80"),
            ])
            .load()
            .expect("Custom env override must load cleanly");

    assert_eq!(custom_override_config.custom.reservation_ttl_minutes, 45);
    assert_eq!(custom_override_config.custom.max_items_per_order, 80);

    // 3. Kiểm tra validation của trường custom mở rộng
    let custom_invalid: Result<AppConfig<OrderDomainConfig>, ConfigError> =
        ConfigLoader::for_test("order-service")
            .with_environment(AppEnvironment::Dev)
            .with_env_vars([("OMNI__CUSTOM__RESERVATION_TTL_MINUTES", "2")])
            .load();

    match custom_invalid {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues
                    .iter()
                    .any(|i| i.field_path == "custom.reservation_ttl_minutes"),
                "Validation should catch errors in nested custom config"
            );
            assert!(
                issues
                    .iter()
                    .any(|i| i.env_var_hint == "OMNI__CUSTOM__RESERVATION_TTL_MINUTES"),
                "Environment variable hint should match custom field path"
            );
        }
        other => panic!("Expected validation failure for custom field, got: {other:?}"),
    }
}

#[test]
fn test_database_advanced_guardrails() {
    // 1. Timeout Budget: acquire (5s) + statement (30s) = 35s >= request_timeout (30s)
    let budget_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("budget-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([
            ("OMNI__DATABASE__ENABLED", "true"),
            ("OMNI__DATABASE__ACQUIRE_TIMEOUT_SECS", "5"),
            ("OMNI__DATABASE__STATEMENT_TIMEOUT_MS", "30000"),
            ("OMNI__SERVER__REQUEST_TIMEOUT_SECS", "30"),
        ])
        .load();

    match budget_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.iter().any(|i| i.code == "CFG_DB_TIMEOUT_BUDGET"),
                "Must reject timeout sum exceeding request timeout budget"
            );
        }
        other => panic!("Expected CFG_DB_TIMEOUT_BUDGET, got: {other:?}"),
    }

    // 2. Lifetime Order: idle (1800s) >= max_lifetime (1800s)
    let order_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("order-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([
            ("OMNI__DATABASE__ENABLED", "true"),
            ("OMNI__DATABASE__IDLE_TIMEOUT_SECS", "1800"),
            ("OMNI__DATABASE__MAX_LIFETIME_SECS", "1800"),
        ])
        .load();

    match order_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.iter().any(|i| i.code == "CFG_DB_LIFETIME_ORDER"),
                "Must reject idle_timeout >= max_lifetime"
            );
        }
        other => panic!("Expected CFG_DB_LIFETIME_ORDER, got: {other:?}"),
    }

    // 3. Staging/Prod requires non-zero statement_timeout_ms
    let stmt_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("staging-service")
        .with_environment(AppEnvironment::Staging)
        .with_env_vars([
            ("OMNI__DATABASE__ENABLED", "true"),
            ("OMNI__DATABASE__STATEMENT_TIMEOUT_MS", "0"),
        ])
        .load();

    match stmt_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues
                    .iter()
                    .any(|i| i.code == "CFG_DB_STATEMENT_TIMEOUT_REQUIRED"),
                "Must require non-zero statement_timeout in staging/prod"
            );
        }
        other => panic!("Expected CFG_DB_STATEMENT_TIMEOUT_REQUIRED, got: {other:?}"),
    }

    // 4. Invalid application_name (contains invalid characters or > 30 chars)
    let app_name_err: Result<AppConfig, ConfigError> = ConfigLoader::for_test("app-service")
        .with_environment(AppEnvironment::Dev)
        .with_env_vars([
            ("OMNI__DATABASE__ENABLED", "true"),
            (
                "OMNI__DATABASE__APPLICATION_NAME",
                "INVALID_NAME_WITH_CAPS!",
            ),
        ])
        .load();

    match app_name_err {
        Err(ConfigError::ValidationFailed(issues)) => {
            assert!(
                issues.iter().any(|i| i.code == "CFG_DB_APPLICATION_NAME"),
                "Must reject invalid application_name format"
            );
        }
        other => panic!("Expected CFG_DB_APPLICATION_NAME, got: {other:?}"),
    }
}
