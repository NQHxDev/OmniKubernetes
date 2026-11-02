//! Distributed Tracing & W3C Context Propagation Module (`tracing`).
//!
//! Provides helpers for bidirectional carrier injection and extraction of
//! W3C Trace Context across message boundaries.

pub mod carrier;
pub mod extractor;

pub use carrier::inject_trace_context;
pub use extractor::{create_consumer_span, extract_trace_context};
