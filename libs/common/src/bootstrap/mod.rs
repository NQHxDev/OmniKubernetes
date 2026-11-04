//! Microservice Scaffold & Application Bootstrap Standard (`omni-common::bootstrap`).
//!
//! Provides [`AppBuilder`] to construct Axum applications with a standardized
//! 9-layer middleware onion, RFC 9457 panic and 404 fallback handlers, and
//! automated infrastructure integrations.

pub mod builder;
pub mod fallback;
pub mod panic_handler;

pub use builder::AppBuilder;
pub use fallback::default_not_found_handler;
pub use panic_handler::handle_panic;
