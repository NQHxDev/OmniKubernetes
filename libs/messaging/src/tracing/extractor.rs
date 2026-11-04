//! W3C Distributed Trace Context Extraction & Consumer Span Creation (`extractor.rs`).
//!
//! Extracts W3C `traceparent` and `tracestate` from transport headers to link
//! asynchronous consumer spans with the originating producer span.

use std::collections::HashMap;

use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderExtractor<'a>(&'a HashMap<String, String>);

impl opentelemetry::propagation::Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        let key_lower = key.to_ascii_lowercase();
        self.0.iter().find_map(|(k, v)| {
            if k.to_ascii_lowercase() == key_lower {
                Some(v.as_str())
            } else {
                None
            }
        })
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(String::as_str).collect()
    }
}

/// Trích xuất ngữ cảnh [`opentelemetry::Context`] từ bảng headers đính kèm bản tin.
#[must_use]
pub fn extract_trace_context(headers: &HashMap<String, String>) -> opentelemetry::Context {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(headers))
    })
}

/// Khởi tạo một [`tracing::Span`] Consumer liên kết với ngữ cảnh vết phân tán của bản tin.
pub fn create_consumer_span(
    system: &str,
    destination: &str,
    message_id: &str,
    headers: &HashMap<String, String>,
) -> Span {
    let extracted_cx = extract_trace_context(headers);

    let span = tracing::info_span!(
        "messaging.process",
        "otel.kind" = "Consumer",
        "messaging.system" = %system,
        "messaging.destination.name" = %destination,
        "messaging.operation.type" = "process",
        "messaging.message.id" = %message_id,
    );

    let _ = span.set_parent(extracted_cx);
    span
}
