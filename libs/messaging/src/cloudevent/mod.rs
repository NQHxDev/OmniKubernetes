//! CNCF CloudEvents v1.0.2 Module (`cloudevent`).
//!
//! Provides the core CloudEvents envelope, builder, extension context attributes,
//! and Structured vs Binary content mode serialization.

pub mod builder;
pub mod event;
pub mod extensions;
pub mod serde_mode;

pub use builder::CloudEventBuilder;
pub use event::{CloudEvent, default_content_type};
pub use extensions::CloudEventExtensions;
pub use serde_mode::{
    from_binary_message, from_raw_message, from_structured_bytes, to_binary_message,
    to_structured_bytes,
};
