use std::collections::HashMap;

use omni_common::middleware::RequestContext;
use omni_messaging::{
    CloudEvent, CloudEventBuilder, from_binary_message, from_raw_message, from_structured_bytes,
    to_binary_message, to_structured_bytes,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OrderCreated {
    order_id: Uuid,
    amount: u64,
    currency: String,
}

#[test]
fn test_cloudevent_builder_defaults_and_attributes() {
    let order_id = Uuid::now_v7();
    let payload = OrderCreated {
        order_id,
        amount: 250_000,
        currency: "VND".to_string(),
    };

    let event = CloudEventBuilder::new(
        "urn:omni:service:order",
        "com.omni.order.created.v1",
        payload.clone(),
    )
    .subject(order_id.to_string())
    .partition_key(order_id.to_string())
    .build();

    assert_eq!(event.specversion, "1.0");
    assert_eq!(event.source, "urn:omni:service:order");
    assert_eq!(event.event_type, "com.omni.order.created.v1");
    assert_eq!(
        event.subject.as_deref(),
        Some(order_id.to_string().as_str())
    );
    assert_eq!(event.data, payload);
    assert_eq!(
        event.extensions.partition_key.as_deref(),
        Some(order_id.to_string().as_str())
    );
    assert_eq!(
        event.extensions.idempotency_key.as_deref(),
        Some(event.id.to_string().as_str())
    );
    assert_eq!(event.datacontenttype, "application/json");
}

#[test]
fn test_cloudevent_structured_mode_roundtrip() {
    let order_id = Uuid::now_v7();
    let payload = OrderCreated {
        order_id,
        amount: 100,
        currency: "USD".to_string(),
    };

    let event = CloudEventBuilder::new(
        "urn:omni:service:order",
        "com.omni.order.created.v1",
        payload,
    )
    .tenant_id(Uuid::now_v7())
    .custom_extension("custom_flag", "enabled")
    .build();

    let bytes =
        to_structured_bytes(&event).expect("Serialization to structured bytes must succeed");
    let deserialized: CloudEvent<OrderCreated> =
        from_structured_bytes(&bytes).expect("Deserialization from structured bytes must succeed");

    assert_eq!(event.id, deserialized.id);
    assert_eq!(event.source, deserialized.source);
    assert_eq!(event.event_type, deserialized.event_type);
    assert_eq!(event.data, deserialized.data);
    assert_eq!(
        event.extensions.tenant_id,
        deserialized.extensions.tenant_id
    );
    assert_eq!(
        deserialized.extensions.get_custom("custom_flag"),
        Some("enabled")
    );
}

#[test]
fn test_cloudevent_binary_mode_headers_and_payload_roundtrip() {
    let order_id = Uuid::now_v7();
    let payload = OrderCreated {
        order_id,
        amount: 500,
        currency: "EUR".to_string(),
    };

    let event = CloudEventBuilder::new(
        "urn:omni:service:order",
        "com.omni.order.created.v1",
        payload,
    )
    .partition_key(order_id.to_string())
    .idempotency_key(order_id.to_string())
    .custom_extension("app-version", "v1.2.3")
    .build();

    let raw_msg = to_binary_message(&event, "omni.order.events.v1")
        .expect("Packaging to binary message must succeed");

    assert_eq!(raw_msg.topic, "omni.order.events.v1");
    assert_eq!(raw_msg.header("ce-specversion"), Some("1.0"));
    assert_eq!(raw_msg.header("ce-id"), Some(event.id.to_string().as_str()));
    assert_eq!(raw_msg.header("ce-type"), Some("com.omni.order.created.v1"));
    assert_eq!(
        raw_msg.header("ce-partition_key"),
        Some(order_id.to_string().as_str())
    );
    assert_eq!(raw_msg.header("ce-app-version"), Some("v1.2.3"));

    let deserialized: CloudEvent<OrderCreated> =
        from_binary_message(&raw_msg).expect("Deserialization from binary message must succeed");

    assert_eq!(event.id, deserialized.id);
    assert_eq!(event.source, deserialized.source);
    assert_eq!(event.event_type, deserialized.event_type);
    assert_eq!(event.data, deserialized.data);
    assert_eq!(
        deserialized.extensions.partition_key,
        Some(order_id.to_string())
    );
    assert_eq!(
        deserialized.extensions.get_custom("app-version"),
        Some("v1.2.3")
    );
}

#[test]
fn test_from_raw_message_auto_detection() {
    let order_id = Uuid::now_v7();
    let payload = OrderCreated {
        order_id,
        amount: 999,
        currency: "JPY".to_string(),
    };
    let event = CloudEventBuilder::new(
        "urn:omni:service:order",
        "com.omni.order.created.v1",
        payload,
    )
    .build();

    // 1. Test binary message auto-detection
    let binary_msg = to_binary_message(&event, "test.topic").unwrap();
    let detected_binary: CloudEvent<OrderCreated> = from_raw_message(&binary_msg).unwrap();
    assert_eq!(detected_binary.id, event.id);

    // 2. Test structured message auto-detection (empty headers, full envelope in payload)
    let structured_bytes = to_structured_bytes(&event).unwrap();
    let structured_msg = omni_messaging::RawBrokerMessage {
        topic: "test.topic".to_string(),
        partition_key: None,
        payload: structured_bytes,
        headers: HashMap::new(),
    };
    let detected_structured: CloudEvent<OrderCreated> = from_raw_message(&structured_msg).unwrap();
    assert_eq!(detected_structured.id, event.id);
    assert_eq!(detected_structured.data, event.data);
}

#[test]
fn test_with_request_context_enrichment() {
    let ctx = RequestContext::new(
        "req-12345",
        "4bf92f3577b34da6a3ce929d0e0e4736",
        "en-US",
        "/orders",
    );

    let event = CloudEventBuilder::new("urn:omni:service:order", "com.omni.order.created.v1", 42)
        .with_request_context(&ctx)
        .build();

    let tp = event
        .extensions
        .traceparent
        .expect("traceparent must be set");
    assert!(tp.contains("4bf92f3577b34da6a3ce929d0e0e4736"));
}
