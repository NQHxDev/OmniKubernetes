//! Logging configuration module for `omni-logging`.

use std::env;

/// Output log format for `omni-logging`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Newline-delimited JSON format for Staging / Production Kubernetes environments.
    #[default]
    Json,
    /// Human-readable ANSI color format for Local Development.
    Pretty,
}

/// Configuration options for initializing the structured logging subsystem.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Logical name of the microservice (e.g. `"order-service"`).
    pub service_name: String,
    /// Deployment environment (`"production"`, `"staging"`, `"local"`).
    pub environment: String,
    /// Minimum log level directive (e.g. `"info"`, `"debug"`, `"warn"`).
    pub log_level: String,
    /// Destination log format (JSON or Pretty).
    pub log_format: LogFormat,
    /// Maximum capacity of the non-blocking ring buffer before dropping logs.
    pub buffered_lines_limit: usize,
    /// Whether to drop records when the buffer is full (`true` to protect Tokio reactor).
    pub lossy: bool,
    /// Optional OpenTelemetry Collector gRPC OTLP endpoint (e.g. `"http://otel-collector:4317"`).
    pub otlp_endpoint: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            service_name: "omni-service".to_string(),
            environment: "development".to_string(),
            log_level: "info".to_string(),
            log_format: LogFormat::Json,
            buffered_lines_limit: 10_000,
            lossy: true,
            otlp_endpoint: None,
        }
    }
}

impl LoggingConfig {
    /// Creates a new [`LoggingConfig`] with default values and specified service name.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// Reads configuration from environment variables with sensible defaults:
    /// - `SERVICE_NAME` or `APP_NAME` -> default `"omni-service"`
    /// - `APP_ENV` or `ENVIRONMENT` -> default `"development"`
    /// - `RUST_LOG` or `LOG_LEVEL` -> default `"info"`
    /// - `APP_LOG_FORMAT` -> `"pretty"` or `"json"` (defaults to `Pretty` if `APP_ENV=local|dev`, else `Json`)
    pub fn from_env() -> Self {
        let service_name = env::var("SERVICE_NAME")
            .or_else(|_| env::var("APP_NAME"))
            .unwrap_or_else(|_| "omni-service".to_string());

        let environment = env::var("APP_ENV")
            .or_else(|_| env::var("ENVIRONMENT"))
            .unwrap_or_else(|_| "development".to_string());

        let log_level = env::var("RUST_LOG")
            .or_else(|_| env::var("LOG_LEVEL"))
            .unwrap_or_else(|_| "info".to_string());

        let log_format = match env::var("APP_LOG_FORMAT").as_deref() {
            Ok("pretty") => LogFormat::Pretty,
            Ok("json") => LogFormat::Json,
            _ => {
                if environment == "local" || environment == "development" {
                    LogFormat::Pretty
                } else {
                    LogFormat::Json
                }
            }
        };

        let otlp_endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .or_else(|_| env::var("OTLP_ENDPOINT"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Self {
            service_name,
            environment,
            log_level,
            log_format,
            buffered_lines_limit: 10_000,
            lossy: true,
            otlp_endpoint,
        }
    }

    /// Sets the optional OTLP gRPC endpoint for OpenTelemetry APM traces export.
    #[must_use]
    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Sets the log format.
    #[must_use]
    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.log_format = format;
        self
    }

    /// Sets the log level.
    #[must_use]
    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    /// Sets the buffer capacity.
    #[must_use]
    pub fn with_buffer_limit(mut self, limit: usize) -> Self {
        self.buffered_lines_limit = limit;
        self
    }
}
