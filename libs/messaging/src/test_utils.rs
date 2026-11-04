//! Integration & Unit Testing Utilities (`test_utils.rs`).
//!
//! Provides fixture factories, mock brokers, and test CloudEvents for fast unit testing.

use serde::Serialize;
use uuid::Uuid;

use crate::{
    broker::memory::InMemoryBroker,
    cloudevent::{builder::CloudEventBuilder, event::CloudEvent},
};

/// Dựng một CloudEvent mẫu phục vụ kiểm thử nhanh.
pub fn create_test_event<T: Serialize>(
    source: impl Into<String>,
    event_type: impl Into<String>,
    data: T,
) -> CloudEvent<T> {
    CloudEventBuilder::new(source, event_type, data)
        .partition_key(Uuid::now_v7().to_string())
        .build()
}

/// Dựng một CloudEvent với aggregate subject và traceparent phục vụ kiểm thử.
pub fn create_test_event_with_subject<T: Serialize>(
    source: impl Into<String>,
    event_type: impl Into<String>,
    subject: impl Into<String>,
    data: T,
) -> CloudEvent<T> {
    CloudEventBuilder::new(source, event_type, data)
        .subject(subject)
        .partition_key(Uuid::now_v7().to_string())
        .build()
}

/// Khởi tạo một [`InMemoryBroker`] dùng chung trong kiểm thử.
#[must_use]
pub fn create_test_broker() -> InMemoryBroker {
    InMemoryBroker::new()
}
