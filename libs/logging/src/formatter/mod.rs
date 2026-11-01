//! Custom event formatters for structured JSON and ANSI color pretty printing.

pub mod json;
pub mod pretty;

pub use json::JsonFormatter;
pub use pretty::PrettyFormatter;
