//! RFC 9457 Catch Panic handler for Axum applications.

use std::{any::Any, borrow::Cow};

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use tracing::error;

use crate::{resilience::PROBLEM_JSON_MIME, response::ResponseMeta};

#[derive(serde::Serialize)]
struct PanicProblemDetails<'a> {
    #[serde(rename = "type")]
    problem_type: &'a str,
    title: &'a str,
    status: u16,
    detail: Cow<'a, str>,
    instance: &'a str,
    code: &'a str,
    meta: ResponseMeta,
}

/// Converts a caught Rust panic into an RFC 9457 HTTP 500 Problem Details response.
#[allow(clippy::needless_pass_by_value)]
pub fn handle_panic(err: Box<dyn Any + Send + 'static>) -> Response {
    let message = if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else {
        "A critical unhandled panic occurred in worker thread execution.".to_string()
    };

    error!(
        panic_message = %message,
        "Worker thread panic caught by Tower CatchPanicLayer. Transforming into RFC 9457 500 Internal Server Error."
    );

    let payload = PanicProblemDetails {
        problem_type: "https://errors.omnikubernetes.com/internal-server-error",
        title: "Internal Server Error",
        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        detail: Cow::Borrowed("An unexpected internal server error occurred."),
        instance: "/",
        code: "ERR_INTERNAL_SERVER",
        meta: ResponseMeta::default(),
    };

    let body = serde_json::to_string(&payload).unwrap_or_else(|_| {
        r#"{"type":"about:blank","title":"Internal Server Error","status":500,"detail":"Internal error","code":"ERR_INTERNAL_SERVER"}"#.to_string()
    });

    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static(PROBLEM_JSON_MIME),
        )
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}
