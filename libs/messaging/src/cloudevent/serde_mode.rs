//! CloudEvents Structured and Binary Content Modes (`serde_mode.rs`).
//!
//! Implements serialization and deserialization for CNCF CloudEvents v1.0.2 in both:
//! 1. **Structured Content Mode:** The entire envelope is encapsulated in a JSON payload.
//! 2. **Binary Content Mode:** Event context attributes are mapped to transport headers,
//!    leaving the raw data payload untouched in the message body.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{
    broker::RawBrokerMessage,
    cloudevent::{
        event::{CloudEvent, default_content_type},
        extensions::CloudEventExtensions,
    },
    error::MessagingError,
};

/// Chuẩn hóa toàn bộ CloudEvent thành mảng bytes JSON theo Structured Content Mode.
pub fn to_structured_bytes<T: Serialize>(event: &CloudEvent<T>) -> Result<Vec<u8>, MessagingError> {
    serde_json::to_vec(event).map_err(|e| MessagingError::SerializationFailed(e.to_string()))
}

/// Giải tuần tự hóa CloudEvent từ mảng bytes JSON theo Structured Content Mode.
pub fn from_structured_bytes<T: DeserializeOwned>(
    bytes: &[u8],
) -> Result<CloudEvent<T>, MessagingError> {
    serde_json::from_slice(bytes).map_err(|e| MessagingError::DeserializationFailed(e.to_string()))
}

/// Đóng gói CloudEvent thành [`RawBrokerMessage`] theo Binary Content Mode.
pub fn to_binary_message<T: Serialize>(
    event: &CloudEvent<T>,
    topic: impl Into<String>,
) -> Result<RawBrokerMessage, MessagingError> {
    let mut headers = HashMap::new();

    // Thuộc tính bắt buộc của CloudEvents v1.0.2
    headers.insert("ce-specversion".to_string(), event.specversion.clone());
    headers.insert("ce-id".to_string(), event.id.to_string());
    headers.insert("ce-source".to_string(), event.source.clone());
    headers.insert("ce-type".to_string(), event.event_type.clone());

    // Thuộc tính tùy chọn
    headers.insert("ce-time".to_string(), event.time.to_rfc3339());
    headers.insert("content-type".to_string(), event.datacontenttype.clone());

    if let Some(ref schema) = event.dataschema {
        headers.insert("ce-dataschema".to_string(), schema.clone());
    }
    if let Some(ref subject) = event.subject {
        headers.insert("ce-subject".to_string(), subject.clone());
    }

    // Các thuộc tính mở rộng chuẩn hóa
    if let Some(ref tp) = event.extensions.traceparent {
        headers.insert("traceparent".to_string(), tp.clone());
        headers.insert("ce-traceparent".to_string(), tp.clone());
    }
    if let Some(ref ts) = event.extensions.tracestate {
        headers.insert("tracestate".to_string(), ts.clone());
        headers.insert("ce-tracestate".to_string(), ts.clone());
    }
    if let Some(ref tenant) = event.extensions.tenant_id {
        headers.insert("ce-tenant_id".to_string(), tenant.to_string());
    }
    if let Some(ref actor) = event.extensions.actor_id {
        headers.insert("ce-actor_id".to_string(), actor.to_string());
    }
    if let Some(ref pkey) = event.extensions.partition_key {
        headers.insert("ce-partition_key".to_string(), pkey.clone());
    }
    if let Some(ref ikey) = event.extensions.idempotency_key {
        headers.insert("ce-idempotency_key".to_string(), ikey.clone());
    }

    // Custom extensions
    for (k, v) in &event.extensions.custom {
        headers.insert(format!("ce-{k}"), v.clone());
    }

    let payload = serde_json::to_vec(&event.data)
        .map_err(|e| MessagingError::SerializationFailed(e.to_string()))?;

    Ok(RawBrokerMessage {
        topic: topic.into(),
        partition_key: event.extensions.partition_key.clone(),
        payload,
        headers,
    })
}

/// Trích xuất CloudEvent từ [`RawBrokerMessage`] theo Binary Content Mode.
pub fn from_binary_message<T: DeserializeOwned>(
    msg: &RawBrokerMessage,
) -> Result<CloudEvent<T>, MessagingError> {
    let get_header = |key: &str| -> Option<&str> { msg.header(key) };

    let specversion = get_header("ce-specversion")
        .or_else(|| get_header("specversion"))
        .unwrap_or("1.0")
        .to_string();

    let id_str = get_header("ce-id")
        .or_else(|| get_header("id"))
        .ok_or_else(|| MessagingError::DeserializationFailed("Missing 'ce-id' header".into()))?;

    let id = Uuid::parse_str(id_str).map_err(|e| {
        MessagingError::DeserializationFailed(format!("Invalid UUID in 'ce-id': {e}"))
    })?;

    let source = get_header("ce-source")
        .or_else(|| get_header("source"))
        .ok_or_else(|| MessagingError::DeserializationFailed("Missing 'ce-source' header".into()))?
        .to_string();

    let event_type = get_header("ce-type")
        .or_else(|| get_header("type"))
        .ok_or_else(|| MessagingError::DeserializationFailed("Missing 'ce-type' header".into()))?
        .to_string();

    let time = if let Some(time_str) = get_header("ce-time").or_else(|| get_header("time")) {
        DateTime::parse_from_rfc3339(time_str)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
    } else {
        Utc::now()
    };

    let datacontenttype = get_header("content-type")
        .or_else(|| get_header("ce-datacontenttype"))
        .or_else(|| get_header("datacontenttype"))
        .unwrap_or(&default_content_type())
        .to_string();

    let dataschema = get_header("ce-dataschema")
        .or_else(|| get_header("dataschema"))
        .map(ToString::to_string);

    let subject = get_header("ce-subject")
        .or_else(|| get_header("subject"))
        .map(ToString::to_string);

    // Parse extensions
    let traceparent = get_header("traceparent")
        .or_else(|| get_header("ce-traceparent"))
        .map(ToString::to_string);

    let tracestate = get_header("tracestate")
        .or_else(|| get_header("ce-tracestate"))
        .map(ToString::to_string);

    let tenant_id = get_header("ce-tenant_id")
        .or_else(|| get_header("tenant_id"))
        .and_then(|s| Uuid::parse_str(s).ok());

    let actor_id = get_header("ce-actor_id")
        .or_else(|| get_header("actor_id"))
        .and_then(|s| Uuid::parse_str(s).ok());

    let partition_key = get_header("ce-partition_key")
        .or_else(|| get_header("partition_key"))
        .map(ToString::to_string)
        .or_else(|| msg.partition_key.clone());

    let idempotency_key = get_header("ce-idempotency_key")
        .or_else(|| get_header("idempotency_key"))
        .map(ToString::to_string)
        .or_else(|| Some(id.to_string()));

    let mut custom = HashMap::new();
    for (k, v) in &msg.headers {
        let k_lower = k.to_ascii_lowercase();
        if let Some(stripped) = k_lower.strip_prefix("ce-")
            && !matches!(
                stripped,
                "specversion"
                    | "id"
                    | "source"
                    | "type"
                    | "time"
                    | "datacontenttype"
                    | "dataschema"
                    | "subject"
                    | "traceparent"
                    | "tracestate"
                    | "tenant_id"
                    | "actor_id"
                    | "partition_key"
                    | "idempotency_key"
            )
        {
            custom.insert(stripped.to_string(), v.clone());
        }
    }

    let extensions = CloudEventExtensions {
        traceparent,
        tracestate,
        tenant_id,
        actor_id,
        partition_key,
        idempotency_key,
        custom,
    };

    let data = serde_json::from_slice(&msg.payload).map_err(|e| {
        MessagingError::DeserializationFailed(format!("Failed to parse payload: {e}"))
    })?;

    Ok(CloudEvent {
        id,
        source,
        specversion,
        event_type,
        time,
        datacontenttype,
        dataschema,
        subject,
        data,
        extensions,
    })
}

/// Tự động nhận diện chế độ Binary hoặc Structured để giải tuần tự hóa [`RawBrokerMessage`].
pub fn from_raw_message<T: DeserializeOwned>(
    msg: &RawBrokerMessage,
) -> Result<CloudEvent<T>, MessagingError> {
    let has_binary_headers = msg.header("ce-id").is_some()
        || msg.header("ce-specversion").is_some()
        || msg.header("ce-type").is_some();

    if has_binary_headers {
        from_binary_message(msg)
    } else {
        // Thử giải tuần tự hóa theo dạng Structured Envelope JSON
        match from_structured_bytes(&msg.payload) {
            Ok(event) => Ok(event),
            Err(structured_err) => {
                // Nếu Structured thất bại, thử Binary fallback một lần nữa
                from_binary_message(msg).map_err(|_| structured_err)
            }
        }
    }
}
