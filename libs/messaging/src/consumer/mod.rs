//! Resilient Consumer Framework Module (`consumer`).
//!
//! Provides declarative message handlers, resilient retry pipeline, exponential backoff with jitter,
//! and automatic Dead Letter Queue routing.

pub mod backoff;
pub mod dlq;
pub mod handler;
pub mod pipeline;

pub use backoff::BackoffPolicy;
pub use dlq::{DeadLetterHeaders, route_to_dlq};
pub use handler::{MessageContext, MessageHandler};
pub use pipeline::ConsumerPipeline;
