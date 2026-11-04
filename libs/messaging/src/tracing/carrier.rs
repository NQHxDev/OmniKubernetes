//! W3C Distributed Trace Context Carrier Injection (`carrier.rs`).
//!
//! Injects W3C `traceparent` and `tracestate` from the active [`tracing::Span`]
//! into message metadata headers for cross-network APM correlation.

use std::collections::HashMap;

use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderInjector<'a>(&'a mut HashMap<String, String>);

impl opentelemetry::propagation::Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

/// Tiêm ngữ cảnh W3C Distributed Trace Context từ Span hiện tại vào bảng headers.
pub fn inject_trace_context(headers: &mut HashMap<String, String>) {
    let current_span = Span::current();
    let cx = current_span.context();

    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut HeaderInjector(headers));
    });
}
