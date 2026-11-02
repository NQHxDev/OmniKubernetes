//! Context bridging adapters for `RequestContext`, Tracing, and PostgreSQL `SessionContext`.

#[cfg(feature = "db-session")]
pub mod db_session;
pub mod request_context;

pub use request_context::{enrich_request_context, enrich_tracing_span};

#[cfg(feature = "db-session")]
pub use db_session::create_session_context;
