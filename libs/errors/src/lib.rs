//! `OmniKubernetes` Enterprise Error Handling Library (`omni-errors`).
//!
//! Implements RFC 9457 Problem Details for HTTP APIs (`application/problem+json`):
//! - [`problem`]: [`ApiErrorResponse`], [`InvalidParamError`], [`ApiErrorResponseBuilder`],
//!   and RFC 6901 JSON Pointer <-> Dot Notation utilities.
//! - [`codes`]: Standardized machine-readable domain and validation error codes & problem type URIs.
//! - [`app_error`]: [`AppError`] adapter with automatic Fluent localization and Section 8 security hardening.

pub mod app_error;
pub mod codes;
pub mod problem;

pub use app_error::AppError;
pub use codes::*;
pub use omni_common::response;
pub use problem::{
    ApiErrorResponse, ApiErrorResponseBuilder, InvalidParamError, PROBLEM_JSON_MEDIA_TYPE,
    dot_notation_to_json_pointer, json_pointer_to_dot_notation,
};
