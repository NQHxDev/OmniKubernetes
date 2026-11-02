//! Human-readable ANSI color formatter for local development with automatic PII sanitization.

use chrono::Utc;
use std::fmt;
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

use crate::{
    config::LoggingConfig,
    sanitizer::{is_sensitive_key, sanitize_field},
};

/// Visitor that collects formatted key-value pairs with automatic PII masking.
#[derive(Debug)]
pub struct SanitizingPrettyVisitor<'a> {
    pub fields: Vec<String>,
    pub message: Option<String>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl SanitizingPrettyVisitor<'_> {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            message: None,
            request_id: None,
            trace_id: None,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl Default for SanitizingPrettyVisitor<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Visit for SanitizingPrettyVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let name = field.name();
        let val_str = format!("{value:?}");
        if name == "message" {
            self.message = Some(val_str);
            return;
        }
        if name == "request_id" {
            self.request_id = Some(val_str.clone());
        }
        if name == "trace_id" {
            self.trace_id = Some(val_str.clone());
        }

        if is_sensitive_key(name) {
            let sanitized = sanitize_field(name, &val_str);
            self.fields.push(format!("{name}={sanitized}"));
        } else {
            self.fields.push(format!("{name}={val_str}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();
        if name == "message" {
            self.message = Some(value.to_string());
            return;
        }
        if name == "request_id" {
            self.request_id = Some(value.to_string());
        }
        if name == "trace_id" {
            self.trace_id = Some(value.to_string());
        }

        if is_sensitive_key(name) {
            let sanitized = sanitize_field(name, value);
            self.fields.push(format!("{name}={sanitized}"));
        } else {
            self.fields.push(format!("{name}={value}"));
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push(format!("{}={value}", field.name()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push(format!("{}={value}", field.name()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push(format!("{}={value}", field.name()));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.push(format!("{}={value:.2}", field.name()));
    }
}

/// ANSI-colored terminal formatter for local development.
#[derive(Debug, Clone)]
pub struct PrettyFormatter {
    _service_name: String,
}

impl PrettyFormatter {
    pub fn new(config: &LoggingConfig) -> Self {
        Self {
            _service_name: config.service_name.clone(),
        }
    }
}

impl<S, N> FormatEvent<S, N> for PrettyFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");

        let mut visitor = SanitizingPrettyVisitor::new();
        event.record(&mut visitor);

        let level_str = match *meta.level() {
            Level::TRACE => "\x1b[35mTRACE\x1b[0m",
            Level::DEBUG => "\x1b[34mDEBUG\x1b[0m",
            Level::INFO => "\x1b[32m INFO\x1b[0m",
            Level::WARN => "\x1b[33m WARN\x1b[0m",
            Level::ERROR => "\x1b[31mERROR\x1b[0m",
        };

        // Correlation context prefix: [req:0192d4b8... | trace:4bf92f35...]
        let req_id = visitor.request_id.as_deref().unwrap_or("-");
        let trace_id = visitor.trace_id.as_deref().unwrap_or("-");
        let correlation = format!("[req:{req_id} | trace:{trace_id}]");

        let message = visitor.message.unwrap_or_default();
        let fields_str = if visitor.fields.is_empty() {
            String::new()
        } else {
            format!(" {}", visitor.fields.join(" "))
        };

        writeln!(
            writer,
            "\x1b[2m{timestamp}\x1b[0m {level_str} \x1b[36m{correlation}\x1b[0m \x1b[2m{}:\x1b[0m {message}{fields_str}",
            meta.target()
        )
    }
}
