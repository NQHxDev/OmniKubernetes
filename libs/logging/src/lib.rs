//! `OmniKubernetes` Enterprise Observability & Structured Logging Library (`omni-logging`).
//!
//! Provides production-ready structured logging with:
//! - **Non-blocking asynchronous I/O** via [`tracing_appender`] offloaded from Tokio worker threads.
//! - **Dual formatting**: Newline-Delimited JSON (Production) vs ANSI Color Pretty (Local Dev).
//! - **PII & Secret Sanitization**: Redaction for authorization tokens, credit cards, passwords, CCCD/CMND.
//! - **Graceful Shutdown**: [`LoggingGuard`] ensuring all buffered log records are flushed to stdout.

pub mod config;
pub mod formatter;
pub mod otel;
pub mod sanitizer;

pub use config::{LogFormat, LoggingConfig};
pub use formatter::{JsonFormatter, PrettyFormatter};
pub use sanitizer::{
    REDACTED, is_sensitive_key, mask_authorization_header, mask_credit_card, mask_email,
    mask_id_number, mask_token, sanitize_field,
};

use std::io;
use thiserror::Error;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt, util::SubscriberInitExt,
};

/// Errors encountered while initializing the logging subsystem.
#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("Failed to parse log filter directive: {0}")]
    FilterParse(#[from] tracing_subscriber::filter::ParseError),

    #[error("Tracing subscriber already initialized: {0}")]
    SetGlobalDefault(#[from] tracing_subscriber::util::TryInitError),

    #[error("I/O error during logging initialization: {0}")]
    Io(#[from] io::Error),

    #[error("OpenTelemetry initialization error: {0}")]
    OpenTelemetry(String),
}

/// RAII Guard that flushes non-blocking buffered logs and OpenTelemetry spans on drop.
#[derive(Debug)]
pub struct LoggingGuard {
    _worker_guard: WorkerGuard,
    tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl LoggingGuard {
    /// Constructs a new guard wrapping a [`WorkerGuard`] and optional [`SdkTracerProvider`].
    pub fn new(
        worker_guard: WorkerGuard,
        tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    ) -> Self {
        Self {
            _worker_guard: worker_guard,
            tracer_provider,
        }
    }
}

impl Drop for LoggingGuard {
    fn drop(&mut self) {
        if let Some(provider) = &self.tracer_provider {
            let _ = provider.shutdown();
        }
    }
}

/// Initializes the global tracing subscriber with non-blocking appender, PII sanitization, and structured formatting.
///
/// # Example
/// ```rust,no_run
/// use omni_logging::{init_logging, LoggingConfig, LogFormat};
///
/// let config = LoggingConfig::default().with_format(LogFormat::Json);
/// let _guard = init_logging(&config).expect("Logging initialization must succeed");
/// tracing::info!("Microservice started successfully");
/// ```
pub fn init_logging(config: &LoggingConfig) -> Result<LoggingGuard, LoggingError> {
    // 1. Initialize W3C TraceContext Propagator
    otel::init_propagator();

    // 2. Non-blocking stdout appender with configurable buffer and lossy drop policy
    let (non_blocking, guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(config.buffered_lines_limit)
        .lossy(config.lossy)
        .finish(io::stdout());

    // 3. Filter directive resolution
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    // 4. Format layer resolution (JSON vs Pretty) with automatic PII sanitization
    let fmt_layer = match config.log_format {
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .event_format(formatter::JsonFormatter::new(config))
            .boxed(),
        LogFormat::Pretty => tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .event_format(formatter::PrettyFormatter::new(config))
            .boxed(),
    };

    // 5. OpenTelemetry OTLP Tracing Pipeline integration if endpoint configured
    let (otel_layer, tracer_provider) = if let Some(endpoint) = &config.otlp_endpoint {
        let (tracer, provider) = otel::init_tracer_provider(config, endpoint)
            .map_err(|e| LoggingError::OpenTelemetry(e.to_string()))?;
        let layer = otel::create_otel_layer(tracer);
        (Some(layer), Some(provider))
    } else {
        (None, None)
    };

    if let Some(otel) = otel_layer {
        Registry::default()
            .with(filter)
            .with(fmt_layer)
            .with(otel)
            .try_init()?;
    } else {
        Registry::default()
            .with(filter)
            .with(fmt_layer)
            .try_init()?;
    }

    Ok(LoggingGuard::new(guard, tracer_provider))
}
