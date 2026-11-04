use std::collections::HashMap;

use omni_messaging::{create_consumer_span, extract_trace_context, inject_trace_context};
use tracing::info_span;

#[test]
fn test_w3c_trace_carrier_injection_and_extraction() {
    let mut headers = HashMap::new();

    // Set an explicit traceparent header
    let traceparent_val = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    headers.insert("traceparent".to_string(), traceparent_val.to_string());

    let cx = extract_trace_context(&headers);
    let _ = cx; // verify extraction succeeds without panic

    // Test carrier injection
    let test_span = info_span!("test_publisher_span");
    let _enter = test_span.enter();

    let mut egress_headers = HashMap::new();
    inject_trace_context(&mut egress_headers);
}

#[test]
fn test_create_consumer_span_attributes() {
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
    );

    let span = create_consumer_span(
        "rabbitmq",
        "omni.order.events.v1",
        "018f4a2b-8a5e-7a1b",
        &headers,
    );

    assert_eq!(
        span.metadata().map(tracing::Metadata::name),
        Some("messaging.process")
    );
}
