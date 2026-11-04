//! Observability, Metrics & Telemetry Module (`instrument`).
//!
//! Provides OpenTelemetry Semantic Conventions and Prometheus RED metrics facades.

pub mod metrics;

pub use metrics::*;
