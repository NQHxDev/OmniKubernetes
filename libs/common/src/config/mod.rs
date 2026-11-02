//! Enterprise Type-Safe Configuration & Environment Binding (`omni_common::config`).
//!
//! Provides a production-grade 5-layer hierarchical configuration loader with fail-fast validation,
//! zero-leak secret protection with memory zeroization, and Kubernetes Downward API integration.

pub mod auth;
pub mod database;
pub mod env;
pub mod error;
pub mod i18n;
pub mod loader;
pub mod messaging;
pub mod observability;
pub mod redis;
pub mod resilience;
pub mod secret;
pub mod server;

pub use auth::{AuthConfig, JwtAlgorithm};
pub use database::{
    ConnectRetryConfig, DatabaseConfig, DbSslMode, MigrationConfig, MigrationMode, SentinelConfig,
    TraceCorrelation,
};
pub use env::{AppEnvironment, K8sRuntimeMeta};
pub use error::{ConfigError, ConfigValidationIssue, field_path_to_env_var};
pub use i18n::I18nConfig;
pub use loader::ConfigLoader;
pub use messaging::MessagingConfig;
pub use observability::{LoggingSectionConfig, ObservabilityConfig};
pub use redis::RedisConfig;
pub use resilience::ResilienceConfig;
pub use secret::{
    ExposeSecret, SecretString, redact_connection_url, serialize_opt_redacted_secret,
    serialize_redacted_secret, validate_min_secret_entropy, validate_non_empty_secret,
};
pub use server::{CorsConfig, HttpServerConfig, ServiceMetaConfig};
pub use validator::Validate;

use chrono::Utc;
use omni_logging::LoggingConfig;
use serde::{Deserialize, Serialize};

/// Struct rỗng mặc định cho các Microservices không yêu cầu cấu hình nghiệp vụ mở rộng riêng.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyCustomConfig;

impl Validate for EmptyCustomConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

/// Cấu trúc cấu hình tổng thể kiểu tĩnh cho một Microservice trong `OmniKubernetes`.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AppConfig<C = EmptyCustomConfig>
where
    C: Validate,
{
    /// Thông tin định danh dịch vụ (Tên, phiên bản, môi trường).
    #[validate(nested)]
    #[serde(default)]
    pub service: ServiceMetaConfig,

    /// Thông tin định danh Pod từ Kubernetes Downward API.
    #[serde(default)]
    pub k8s: K8sRuntimeMeta,

    /// Cấu hình máy chủ HTTP (Axum) và gRPC nội bộ (Tonic).
    #[validate(nested)]
    #[serde(default)]
    pub server: HttpServerConfig,

    /// Cấu hình hệ thống Structured Logging, PII Masking & OpenTelemetry OTLP.
    #[validate(nested)]
    #[serde(default)]
    pub logging: LoggingSectionConfig,

    /// Cấu hình quan sát bổ sung (Prometheus Metrics & `OpenAPI` Specification).
    #[validate(nested)]
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// Cấu hình kết nối PostgreSQL (`omni-database`).
    #[validate(nested)]
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Cấu hình kết nối Redis (`omni-cache`).
    #[validate(nested)]
    #[serde(default)]
    pub redis: RedisConfig,

    /// Cấu hình xác thực JWT & băm mật khẩu Argon2 (`omni-auth`).
    #[validate(nested)]
    #[serde(default)]
    pub auth: AuthConfig,

    /// Cấu hình giao tiếp hướng sự kiện & Transactional Outbox (`omni-messaging`).
    #[validate(nested)]
    #[serde(default)]
    pub messaging: MessagingConfig,

    /// Cấu hình khả năng chịu lỗi mạng, Rate Limiting & Circuit Breaker.
    #[validate(nested)]
    #[serde(default)]
    pub resilience: ResilienceConfig,

    /// Cấu hình đa ngôn ngữ Mozilla Fluent (`omni-common::i18n`).
    #[validate(nested)]
    #[serde(default)]
    pub i18n: I18nConfig,

    /// Cấu hình mở rộng chuyên biệt của từng Domain Microservice.
    #[validate(nested)]
    #[serde(default)]
    pub custom: C,
}

impl<C> Default for AppConfig<C>
where
    C: Validate + Default,
{
    fn default() -> Self {
        Self {
            service: ServiceMetaConfig::default(),
            k8s: K8sRuntimeMeta::default(),
            server: HttpServerConfig::default(),
            logging: LoggingSectionConfig::default(),
            observability: ObservabilityConfig::default(),
            database: DatabaseConfig::default(),
            redis: RedisConfig::default(),
            auth: AuthConfig::default(),
            messaging: MessagingConfig::default(),
            resilience: ResilienceConfig::default(),
            i18n: I18nConfig::default(),
            custom: C::default(),
        }
    }
}

impl<C> AppConfig<C>
where
    C: Validate,
{
    /// Nạp cấu hình đầy đủ từ 5 tầng phân cấp cho một microservice.
    pub fn load(service_name: impl Into<String>) -> Result<Self, ConfigError>
    where
        C: serde::de::DeserializeOwned + Default,
    {
        ConfigLoader::new(service_name).load()
    }

    /// Chuyển đổi cấu hình sang [`LoggingConfig`], tự động kế thừa
    /// `service.name` và `service.environment` từ `ServiceMetaConfig`.
    #[must_use]
    pub fn to_logging_config(&self) -> LoggingConfig {
        LoggingConfig {
            service_name: self.service.name.clone(),
            environment: self.service.environment.as_str().to_string(),
            log_level: self.logging.log_level.clone(),
            log_format: self.logging.log_format,
            buffered_lines_limit: self.logging.buffered_lines_limit,
            lossy: self.logging.lossy,
            otlp_endpoint: self.logging.otlp_endpoint.clone(),
        }
    }

    /// Trả về bản tóm tắt cấu hình an toàn lúc khởi động (`Startup Diagnostic Summary`)
    /// với 100% mật khẩu đã được che giấu (`[REDACTED]`).
    #[must_use]
    pub fn startup_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
            "level": "INFO",
            "message": "Application configuration loaded and validated successfully",
            "target": "omni_common::config",
            "service": {
                "name": self.service.name,
                "version": self.service.version,
                "environment": self.service.environment.as_str(),
            },
            "config_summary": {
                "http_addr": format!("{}:{}", self.server.host, self.server.port),
                "grpc_addr": format!("{}:{}", self.server.host, self.server.grpc_port),
                "database_url": self.database.redacted_url(),
                "database_pool": format!(
                    "min={}, max={}, acquire_timeout={}s",
                    self.database.min_connections,
                    self.database.max_connections,
                    self.database.acquire_timeout_secs
                ),
                "redis_url": self.redis.redacted_url(),
                "jwt_secret": "[REDACTED]",
                "openapi_swagger_ui_enabled": self.observability.swagger_ui_enabled,
            }
        })
    }

    /// Thực hiện quy trình kiểm tra hợp lệ 2 pha (Fail-Fast Startup Validation):
    /// - Pha 1: Kiểm tra ràng buộc cấp trường ([`validator::Validate`]).
    /// - Pha 2: Kiểm tra bất biến liên trường & chốt chặn môi trường Production (Cross-Field Guardrails).
    pub fn validate_all(&self) -> Result<(), ConfigError> {
        let mut issues = Vec::new();

        // Pha 1: Kiểm tra Ràng buộc Cấp độ Trường
        if let Err(errors) = self.validate() {
            collect_validation_errors("", &errors, &mut issues);
        }

        // Pha 2: Kiểm tra Bất biến Liên trường & Chốt chặn Production
        self.validate_database_guardrails(&mut issues);
        self.validate_auth_guardrails(&mut issues);
        self.validate_environment_guardrails(&mut issues);

        if !issues.is_empty() {
            return Err(ConfigError::ValidationFailed(issues));
        }

        Ok(())
    }

    fn validate_database_guardrails(&self, issues: &mut Vec<ConfigValidationIssue>) {
        if !self.database.enabled {
            return;
        }

        self.validate_database_pool_and_timeouts(issues);
        self.validate_database_application_name(issues);
        self.validate_database_security(issues);
    }

    fn validate_database_pool_and_timeouts(&self, issues: &mut Vec<ConfigValidationIssue>) {
        if self.database.min_connections > self.database.max_connections {
            issues.push(ConfigValidationIssue {
                field_path: "database.min_connections".to_string(),
                code: "CFG_DB_POOL_BOUNDS".to_string(),
                message: format!(
                    "min_connections ({}) cannot be greater than max_connections ({})",
                    self.database.min_connections, self.database.max_connections
                ),
                env_var_hint: "OMNI__DATABASE__MIN_CONNECTIONS".to_string(),
            });
        }

        if self.database.acquire_timeout_secs > self.server.request_timeout_secs {
            issues.push(ConfigValidationIssue {
                field_path: "database.acquire_timeout_secs".to_string(),
                code: "CFG_TIMEOUT_HIERARCHY".to_string(),
                message: format!(
                    "acquire_timeout_secs ({}s) must be less than or equal to server.request_timeout_secs ({}s)",
                    self.database.acquire_timeout_secs, self.server.request_timeout_secs
                ),
                env_var_hint: "OMNI__DATABASE__ACQUIRE_TIMEOUT_SECS".to_string(),
            });
        }

        if self.database.statement_timeout_ms > 0
            && (self.database.acquire_timeout_secs * 1000 + self.database.statement_timeout_ms)
                >= (self.server.request_timeout_secs * 1000)
        {
            issues.push(ConfigValidationIssue {
                field_path: "database.statement_timeout_ms".to_string(),
                code: "CFG_DB_TIMEOUT_BUDGET".to_string(),
                message: format!(
                    "acquire_timeout ({}ms) + statement_timeout ({}ms) must be less than server.request_timeout ({}ms)",
                    self.database.acquire_timeout_secs * 1000,
                    self.database.statement_timeout_ms,
                    self.server.request_timeout_secs * 1000
                ),
                env_var_hint: "OMNI__DATABASE__STATEMENT_TIMEOUT_MS".to_string(),
            });
        }

        if self.database.idle_timeout_secs >= self.database.max_lifetime_secs {
            issues.push(ConfigValidationIssue {
                field_path: "database.idle_timeout_secs".to_string(),
                code: "CFG_DB_LIFETIME_ORDER".to_string(),
                message: format!(
                    "idle_timeout_secs ({}s) must be strictly less than max_lifetime_secs ({}s)",
                    self.database.idle_timeout_secs, self.database.max_lifetime_secs
                ),
                env_var_hint: "OMNI__DATABASE__IDLE_TIMEOUT_SECS".to_string(),
            });
        }

        if self.service.environment != AppEnvironment::Dev
            && (self.database.statement_timeout_ms == 0
                || self.database.idle_in_transaction_timeout_ms == 0)
        {
            issues.push(ConfigValidationIssue {
                field_path: "database.statement_timeout_ms".to_string(),
                code: "CFG_DB_STATEMENT_TIMEOUT_REQUIRED".to_string(),
                message: "statement_timeout_ms and idle_in_transaction_timeout_ms must be non-zero in staging and production".to_string(),
                env_var_hint: "OMNI__DATABASE__STATEMENT_TIMEOUT_MS".to_string(),
            });
        }
    }

    fn validate_database_application_name(&self, issues: &mut Vec<ConfigValidationIssue>) {
        if let Some(app_name) = &self.database.application_name
            && (app_name.trim().is_empty()
                || app_name.len() > 30
                || !app_name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
        {
            issues.push(ConfigValidationIssue {
                field_path: "database.application_name".to_string(),
                code: "CFG_DB_APPLICATION_NAME".to_string(),
                message: "database.application_name must be non-empty, max 30 characters and contain only lowercase alphanumeric characters and hyphens".to_string(),
                env_var_hint: "OMNI__DATABASE__APPLICATION_NAME".to_string(),
            });
        }
    }

    fn validate_database_security(&self, issues: &mut Vec<ConfigValidationIssue>) {
        if self.database.password.expose_secret().trim().is_empty() {
            issues.push(ConfigValidationIssue {
                field_path: "database.password".to_string(),
                code: "CFG_PROD_SECRET_REQUIRED".to_string(),
                message: "database.password must not be empty when database is enabled".to_string(),
                env_var_hint: "OMNI__DATABASE__PASSWORD".to_string(),
            });
        } else if self.service.environment.is_prod() {
            let pwd = self
                .database
                .password
                .expose_secret()
                .trim()
                .to_ascii_lowercase();
            if matches!(
                pwd.as_str(),
                "postgres"
                    | "password"
                    | "dev_only_password_change_me"
                    | "root"
                    | "admin"
                    | "123456"
            ) {
                issues.push(ConfigValidationIssue {
                    field_path: "database.password".to_string(),
                    code: "CFG_PROD_SECRET_REQUIRED".to_string(),
                    message: "database.password must not use insecure default password in production environment".to_string(),
                    env_var_hint: "OMNI__DATABASE__PASSWORD".to_string(),
                });
            }

            if !matches!(
                self.database.ssl_mode,
                DbSslMode::Require | DbSslMode::VerifyCa | DbSslMode::VerifyFull
            ) {
                issues.push(ConfigValidationIssue {
                    field_path: "database.ssl_mode".to_string(),
                    code: "CFG_PROD_DB_TLS_REQUIRED".to_string(),
                    message: "database.ssl_mode must be 'require', 'verify-ca', or 'verify-full' in production".to_string(),
                    env_var_hint: "OMNI__DATABASE__SSL_MODE".to_string(),
                });
            }

            if self.database.effective_migration_mode() == MigrationMode::Apply {
                issues.push(ConfigValidationIssue {
                    field_path: "database.migration.mode".to_string(),
                    code: "CFG_PROD_DB_MIGRATION_MODE".to_string(),
                    message: "database.migration.mode should be 'verify' in production pods; execute migrations via Kubernetes PreSync Job".to_string(),
                    env_var_hint: "OMNI__DATABASE__MIGRATION__MODE".to_string(),
                });
            }
        }
    }

    fn validate_auth_guardrails(&self, issues: &mut Vec<ConfigValidationIssue>) {
        if !self.auth.enabled {
            return;
        }

        let secret_len = self.auth.jwt_secret.expose_secret().len();
        if secret_len < 32 {
            issues.push(ConfigValidationIssue {
                field_path: "auth.jwt_secret".to_string(),
                code: "CFG_PROD_JWT_ENTROPY".to_string(),
                message: format!(
                    "auth.jwt_secret length ({secret_len} bytes) is shorter than required minimum of 32 bytes (256 bits)"
                ),
                env_var_hint: "OMNI__AUTH__JWT_SECRET".to_string(),
            });
        } else if self.service.environment.is_prod()
            && self.auth.jwt_secret.expose_secret().contains("dev_only")
        {
            issues.push(ConfigValidationIssue {
                field_path: "auth.jwt_secret".to_string(),
                code: "CFG_PROD_JWT_ENTROPY".to_string(),
                message: "auth.jwt_secret must not use insecure development secret in production environment".to_string(),
                env_var_hint: "OMNI__AUTH__JWT_SECRET".to_string(),
            });
        }
    }

    fn validate_environment_guardrails(&self, issues: &mut Vec<ConfigValidationIssue>) {
        if self.service.environment.is_prod() && self.observability.swagger_ui_enabled {
            issues.push(ConfigValidationIssue {
                field_path: "observability.swagger_ui_enabled".to_string(),
                code: "CFG_PROD_SWAGGER_UI_DISABLED".to_string(),
                message: "Swagger UI HTML must be disabled in 'prod' environment".to_string(),
                env_var_hint: "OMNI__OBSERVABILITY__SWAGGER_UI_ENABLED".to_string(),
            });
        }

        if self.service.environment.is_prod()
            && self.server.cors.allow_origins.iter().any(|o| o == "*")
            && self.server.cors.allow_credentials
        {
            issues.push(ConfigValidationIssue {
                field_path: "server.cors.allow_origins".to_string(),
                code: "CFG_PROD_CORS_STRICT".to_string(),
                message: "Wildcard origin '*' cannot be combined with allow_credentials = true in 'prod' environment".to_string(),
                env_var_hint: "OMNI__SERVER__CORS__ALLOW_ORIGINS".to_string(),
            });
        }

        if !self
            .i18n
            .supported_locales
            .contains(&self.i18n.default_locale)
        {
            issues.push(ConfigValidationIssue {
                field_path: "i18n.default_locale".to_string(),
                code: "CFG_I18N_DEFAULT_IN_SUPPORTED".to_string(),
                message: format!(
                    "default_locale '{}' is not present in supported_locales: {:?}",
                    self.i18n.default_locale, self.i18n.supported_locales
                ),
                env_var_hint: "OMNI__I18N__DEFAULT_LOCALE".to_string(),
            });
        }
    }
}

fn collect_validation_errors(
    prefix: &str,
    errors: &validator::ValidationErrors,
    issues: &mut Vec<ConfigValidationIssue>,
) {
    for (field, kind) in errors.errors() {
        let field_path = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };

        match kind {
            validator::ValidationErrorsKind::Field(field_errors) => {
                for err in field_errors {
                    let env_var_hint = field_path_to_env_var(&field_path);
                    let message = err.message.as_ref().map_or_else(
                        || format!("Validation failed for field '{field_path}'"),
                        ToString::to_string,
                    );

                    issues.push(ConfigValidationIssue {
                        field_path: field_path.clone(),
                        code: err.code.to_string(),
                        message,
                        env_var_hint,
                    });
                }
            }
            validator::ValidationErrorsKind::Struct(nested) => {
                collect_validation_errors(&field_path, nested, issues);
            }
            validator::ValidationErrorsKind::List(list) => {
                for (idx, nested) in list {
                    let list_path = format!("{field_path}[{idx}]");
                    collect_validation_errors(&list_path, nested, issues);
                }
            }
        }
    }
}
