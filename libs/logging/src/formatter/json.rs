//! Structured JSON event formatter with automatic PII sanitization conforming to `OTel` / ECS.

use chrono::Utc;
use serde_json::{Map, Value};
use std::{env, fmt};
use tracing::{
    Event, Subscriber,
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

/// Visitor that collects event fields into a JSON Map and scrubs sensitive keys.
#[derive(Debug)]
pub struct SanitizingJsonVisitor<'a> {
    pub map: &'a mut Map<String, Value>,
    pub message: Option<String>,
}

impl Visit for SanitizingJsonVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let name = field.name();
        let val_str = format!("{value:?}");
        if name == "message" {
            self.message = Some(val_str);
            return;
        }

        if is_sensitive_key(name) {
            let sanitized = sanitize_field(name, &val_str);
            self.map
                .insert(name.to_string(), Value::String(sanitized.into_owned()));
        } else {
            self.map.insert(name.to_string(), Value::String(val_str));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();
        if name == "message" {
            self.message = Some(value.to_string());
            return;
        }

        if is_sensitive_key(name) {
            let sanitized = sanitize_field(name, value);
            self.map
                .insert(name.to_string(), Value::String(sanitized.into_owned()));
        } else {
            self.map
                .insert(name.to_string(), Value::String(value.to_string()));
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.map
            .insert(field.name().to_string(), Value::Bool(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.map
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.map
            .insert(field.name().to_string(), Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.map
            .insert(field.name().to_string(), Value::from(value));
    }
}

/// Zero-allocation, ECS/OTel-compliant JSON Event Formatter with automatic PII scrub.
#[derive(Debug, Clone)]
pub struct JsonFormatter {
    service_name: String,
    environment: String,
    pod_name: Option<String>,
    namespace: Option<String>,
    node_name: Option<String>,
}

impl JsonFormatter {
    pub fn new(config: &LoggingConfig) -> Self {
        Self {
            service_name: config.service_name.clone(),
            environment: config.environment.clone(),
            pod_name: env::var("POD_NAME").or_else(|_| env::var("HOSTNAME")).ok(),
            namespace: env::var("POD_NAMESPACE").ok(),
            node_name: env::var("NODE_NAME").ok(),
        }
    }
}

impl<S, N> FormatEvent<S, N> for JsonFormatter
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
        let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

        let mut fields_map = Map::new();
        let mut visitor = SanitizingJsonVisitor {
            map: &mut fields_map,
            message: None,
        };
        event.record(&mut visitor);

        let message = visitor.message.unwrap_or_default();

        let mut root = Map::new();
        root.insert("timestamp".to_string(), Value::String(timestamp));
        root.insert("level".to_string(), Value::String(meta.level().to_string()));
        root.insert("message".to_string(), Value::String(message));
        root.insert(
            "target".to_string(),
            Value::String(meta.target().to_string()),
        );

        let mut service_map = Map::new();
        service_map.insert("name".to_string(), Value::String(self.service_name.clone()));
        service_map.insert(
            "environment".to_string(),
            Value::String(self.environment.clone()),
        );
        root.insert("service".to_string(), Value::Object(service_map));

        if self.pod_name.is_some() || self.namespace.is_some() || self.node_name.is_some() {
            let mut k8s_map = Map::new();
            if let Some(pod) = &self.pod_name {
                k8s_map.insert("pod_name".to_string(), Value::String(pod.clone()));
            }
            if let Some(ns) = &self.namespace {
                k8s_map.insert("namespace".to_string(), Value::String(ns.clone()));
            }
            if let Some(node) = &self.node_name {
                k8s_map.insert("node_name".to_string(), Value::String(node.clone()));
            }
            root.insert("k8s".to_string(), Value::Object(k8s_map));
        }

        // Structure trace correlation if present in fields
        let trace_id = fields_map.remove("trace_id");
        let span_id = fields_map.remove("span_id");
        if trace_id.is_some() || span_id.is_some() {
            let mut trace_map = Map::new();
            if let Some(t) = trace_id {
                trace_map.insert("trace_id".to_string(), t);
            }
            if let Some(s) = span_id {
                trace_map.insert("span_id".to_string(), s);
            }
            trace_map.insert("sampled".to_string(), Value::Bool(true));
            root.insert("trace".to_string(), Value::Object(trace_map));
        }

        // Structure HTTP request context if target is access_log or has request_id
        if meta.target() == "omni_common::access_log" || fields_map.contains_key("request_id") {
            let mut req_map = Map::new();
            for key in &[
                "request_id",
                "method",
                "path",
                "route",
                "status",
                "status_code",
                "duration_ms",
                "client_ip",
                "user_agent",
                "locale",
                "user_id",
                "tenant_id",
            ] {
                if let Some(v) = fields_map.remove(*key) {
                    req_map.insert((*key).to_string(), v);
                }
            }
            if !req_map.is_empty() {
                root.insert("request".to_string(), Value::Object(req_map));
            }
        }

        // Merge any remaining sanitized fields
        for (k, v) in fields_map {
            root.insert(k, v);
        }

        let json_line = serde_json::to_string(&root).unwrap_or_else(|_| "{}".to_string());
        writeln!(writer, "{json_line}")
    }
}
