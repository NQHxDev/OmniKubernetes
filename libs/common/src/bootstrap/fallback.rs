//! Fallback 404 Not Found handler producing RFC 9457 Problem Details.

use std::borrow::Cow;

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, Uri, header},
    response::Response,
};

use crate::{resilience::PROBLEM_JSON_MIME, response::ResponseMeta};

#[derive(serde::Serialize)]
struct NotFoundProblemDetails<'a> {
    #[serde(rename = "type")]
    problem_type: &'a str,
    title: &'a str,
    status: u16,
    detail: Cow<'a, str>,
    instance: Cow<'a, str>,
    code: &'a str,
    meta: ResponseMeta,
}

/// Default fallback handler returning RFC 9457 404 Problem Details.
pub async fn default_not_found_handler(uri: Uri) -> Response {
    let path = uri.path();
    let detail = format!("The requested endpoint '{path}' was not found on this server.");

    let payload = NotFoundProblemDetails {
        problem_type: "https://errors.omnikubernetes.com/not-found",
        title: "Not Found",
        status: StatusCode::NOT_FOUND.as_u16(),
        detail: Cow::Owned(detail),
        instance: Cow::Borrowed(path),
        code: "ERR_NOT_FOUND",
        meta: ResponseMeta::default(),
    };

    let body = serde_json::to_string(&payload).unwrap_or_else(|_| {
        r#"{"type":"about:blank","title":"Not Found","status":404,"detail":"Endpoint not found","code":"ERR_NOT_FOUND"}"#.to_string()
    });

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static(PROBLEM_JSON_MIME),
        )
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}
