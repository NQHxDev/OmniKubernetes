//! RFC 9457 Problem Details for HTTP APIs (`ApiErrorResponse` & `InvalidParamError`).
//!
//! Implements the `OmniKubernetes` Error Contract Standard:
//! - Strict RFC 9457 structure (`type`, `title`, `status`, `detail`, `instance`) with `application/problem+json` MIME type.
//! - RFC 9457 Extension Members (`code`, `invalid_params`, `meta`).
//! - RFC 6901 JSON Pointer <-> Dot Notation field mapping (`items.0.quantity` <-> `/items/0/quantity`).
//! - Secure-by-Default PII & Reflected XSS protection (`rejected_value` defaults to `None` and is omitted).
//! - Dual-mode i18n/l10n (Server-rendered Fluent `reason` + Client-side `code` and `params`).
//! - Production Information Leakage Prevention for HTTP `500 Internal Server Error`.

use std::{borrow::Cow, collections::HashMap, fmt};

use axum::{
    Json,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use fluent_bundle::{FluentArgs, FluentValue};
use omni_common::{
    i18n::I18nManager,
    response::{ResponseMeta, X_REQUEST_ID_HEADER},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::codes::{
    ERR_BAD_REQUEST, ERR_CONFLICT, ERR_FORBIDDEN, ERR_INTERNAL_SERVER, ERR_METHOD_NOT_ALLOWED,
    ERR_NOT_FOUND, ERR_RATE_LIMITED, ERR_SERVICE_UNAVAILABLE, ERR_UNAUTHORIZED,
    ERR_VALIDATION_FAILED, TYPE_BAD_REQUEST, TYPE_CONFLICT, TYPE_FORBIDDEN,
    TYPE_INTERNAL_SERVER_ERROR, TYPE_METHOD_NOT_ALLOWED, TYPE_NOT_FOUND, TYPE_RATE_LIMITED,
    TYPE_SERVICE_UNAVAILABLE, TYPE_UNAUTHORIZED, TYPE_VALIDATION_ERROR,
};

/// Standard RFC 9457 MIME type for HTTP Problem Details (`application/problem+json`).
pub const PROBLEM_JSON_MEDIA_TYPE: &str = "application/problem+json";

static PROBLEM_JSON_MIME: HeaderValue = HeaderValue::from_static(PROBLEM_JSON_MEDIA_TYPE);

/// Converts an RFC 6901 JSON Pointer (e.g. `"/items/0/quantity"`) into Dot Notation (`"items.0.quantity"`).
///
/// Unescapes RFC 6901 tokens (`~1` -> `/` and `~0` -> `~`).
pub fn json_pointer_to_dot_notation(pointer: &str) -> String {
    let trimmed = pointer.strip_prefix('/').unwrap_or(pointer);
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>()
        .join(".")
}

/// Converts a Dot Notation field path (e.g. `"items.0.quantity"`) into an RFC 6901 JSON Pointer (`"/items/0/quantity"`).
///
/// Escapes RFC 6901 tokens (`~` -> `~0` and `/` -> `~1`).
pub fn dot_notation_to_json_pointer(dot_path: &str) -> String {
    let trimmed = dot_path.trim_matches('.');
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(trimmed.len() + 1);
    for segment in trimmed.split('.') {
        out.push('/');
        let escaped = segment.replace('~', "~0").replace('/', "~1");
        out.push_str(&escaped);
    }
    out
}

/// Field-level parameter validation violation adhering to RFC 9457 Appendix A.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[must_use]
pub struct InvalidParamError {
    /// Violating field path in standardized Dot Notation (e.g. `"email"`, `"items.0.quantity"`, `"shipping_address.postal_code"`).
    /// Maps 1-to-1 with RFC 6901 JSON Pointer (`/items/0/quantity`).
    #[schema(example = "items.0.quantity", value_type = String)]
    pub name: Cow<'static, str>,
    /// Server-localized human-readable explanation of the violation.
    #[schema(
        example = "Số lượng sản phẩm trong đơn hàng phải lớn hơn 0.",
        value_type = String
    )]
    pub reason: Cow<'static, str>,
    /// Machine-readable field validation error code (e.g. `"NUMBER_MIN_VALUE"`, `"INVALID_POSTAL_CODE"`).
    #[schema(example = "NUMBER_MIN_VALUE", value_type = String)]
    pub code: Cow<'static, str>,
    /// Rejected value — **Secure-by-Default Policy**:
    /// Strictly `None` (omitted from JSON) for all PII fields and free-form user strings to prevent
    /// PII leakage in access logs/APM and Reflected XSS. Only populated via [`Self::with_rejected_enum_value`]
    /// for finite non-PII enum constants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_value: Option<serde_json::Value>,
    /// Safe dynamic interpolation parameters (`min`, `max`, `actual`, `pattern`, `allowed`) enabling
    /// client-side localization on Mobile/SPA without echoing raw user input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

impl InvalidParamError {
    /// Creates a new [`InvalidParamError`] following the Secure-by-Default policy (`rejected_value: None`).
    pub fn new(
        name: impl Into<Cow<'static, str>>,
        code: impl Into<Cow<'static, str>>,
        reason: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            name: name.into(),
            reason: reason.into(),
            code: code.into(),
            rejected_value: None,
            params: None,
        }
    }

    /// Creates a dual-mode localized [`InvalidParamError`]:
    /// - Renders `reason` on the server via [`I18nManager`] for the target `locale`.
    /// - Preserves `code` and `params` for client-side localization on Native Mobile / SPA.
    pub fn localized(
        name: impl Into<Cow<'static, str>>,
        code: impl Into<Cow<'static, str>>,
        fluent_key: &str,
        params: Option<HashMap<String, serde_json::Value>>,
        i18n: &I18nManager,
        locale: &str,
    ) -> Self {
        let name_cow = name.into();
        let code_cow = code.into();

        // Build interpolation map including `$name` if not explicitly provided in params
        let mut interpolation_map = params.clone().unwrap_or_default();
        interpolation_map
            .entry("name".to_string())
            .or_insert_with(|| serde_json::Value::String(name_cow.to_string()));

        let reason = i18n.translate_with_params(locale, fluent_key, Some(&interpolation_map));

        Self {
            name: name_cow,
            reason: Cow::Owned(reason),
            code: code_cow,
            rejected_value: None,
            params,
        }
    }

    /// Inserts a single safe quantitative or constraint parameter into `params` (e.g. `"min": 1`).
    pub fn with_param(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.params
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// Attaches a map of safe interpolation parameters (`min`, `max`, `actual`, `pattern`, `allowed`).
    pub fn with_params(mut self, params: HashMap<String, serde_json::Value>) -> Self {
        self.params = Some(params);
        self
    }

    /// Explicitly attaches a rejected **non-PII enum or scalar constant** (e.g. `role: "SUPER_ADMIN"`).
    ///
    /// # Security Policy (Section 4.4 & Section 8.2)
    /// **NEVER** pass PII (email, phone, national ID, address, OTP, password, tokens) or free-form user input
    /// to this method.
    pub fn with_rejected_enum_value(
        mut self,
        safe_enum_value: impl Into<serde_json::Value>,
    ) -> Self {
        self.rejected_value = Some(safe_enum_value.into());
        self
    }

    /// Returns the `name` field converted to an RFC 6901 JSON Pointer (e.g. `"/items/0/quantity"`).
    pub fn to_json_pointer(&self) -> String {
        dot_notation_to_json_pointer(&self.name)
    }
}

/// RFC 9457 Problem Details Error Contract with Zero-Cost Static String Borrowing.
///
/// Serialized with HTTP `Content-Type: application/problem+json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorResponse {
    /// URI reference identifying the problem type (RFC 9457 Section 3.1.1).
    #[serde(rename = "type")]
    #[schema(
        example = "https://errors.omnikubernetes.com/validation-error",
        value_type = String
    )]
    pub error_type: Cow<'static, str>,
    /// Short, localized human-readable summary of the problem type (RFC 9457 Section 3.1.2).
    #[schema(example = "Dữ liệu yêu cầu không hợp lệ", value_type = String)]
    pub title: Cow<'static, str>,
    /// HTTP status code generated by the origin server (Single Source of Truth, RFC 9457 Section 3.1.3).
    #[schema(example = 422)]
    pub status: u16,
    /// Localized human-readable explanation specific to this occurrence of the problem (RFC 9457 Section 3.1.4).
    #[schema(
        example = "Có 2 trường dữ liệu không vượt qua kiểm tra ràng buộc nghiệp vụ.",
        value_type = String
    )]
    pub detail: Cow<'static, str>,
    /// URI reference identifying the specific endpoint occurrence (RFC 9457 Section 3.1.5).
    #[schema(example = "/api/v1/orders", value_type = String)]
    pub instance: Cow<'static, str>,
    /// Machine-readable domain error code (RFC 9457 Extension Member).
    #[schema(example = "ERR_VALIDATION_FAILED", value_type = String)]
    pub code: Cow<'static, str>,
    /// Optional list of field-level validation violations (RFC 9457 Appendix A.1 Extension Member).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_params: Option<Vec<InvalidParamError>>,
    /// Correlation and localization metadata (`request_id`, `trace_id`, `timestamp`, `locale`).
    pub meta: ResponseMeta,
}

impl ApiErrorResponse {
    /// Returns a fluent [`ApiErrorResponseBuilder`] for constructing an [`ApiErrorResponse`].
    pub fn builder() -> ApiErrorResponseBuilder {
        ApiErrorResponseBuilder::default()
    }

    /// Constructs a localized `400 Bad Request` problem response.
    pub fn bad_request(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-bad-request-title");
        let detail = i18n.translate_simple(&meta.locale, "err-bad-request-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_BAD_REQUEST),
            title: Cow::Owned(title),
            status: StatusCode::BAD_REQUEST.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_BAD_REQUEST),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `401 Unauthorized` problem response.
    pub fn unauthorized(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-unauthorized-title");
        let detail = i18n.translate_simple(&meta.locale, "err-unauthorized-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_UNAUTHORIZED),
            title: Cow::Owned(title),
            status: StatusCode::UNAUTHORIZED.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_UNAUTHORIZED),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `403 Forbidden` problem response.
    pub fn forbidden(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-forbidden-title");
        let detail = i18n.translate_simple(&meta.locale, "err-forbidden-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_FORBIDDEN),
            title: Cow::Owned(title),
            status: StatusCode::FORBIDDEN.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_FORBIDDEN),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `404 Not Found` problem response.
    pub fn not_found(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-not-found-title");
        let detail = i18n.translate_simple(&meta.locale, "err-not-found-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_NOT_FOUND),
            title: Cow::Owned(title),
            status: StatusCode::NOT_FOUND.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_NOT_FOUND),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `405 Method Not Allowed` problem response.
    pub fn method_not_allowed(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-method-not-allowed-title");
        let detail = i18n.translate_simple(&meta.locale, "err-method-not-allowed-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_METHOD_NOT_ALLOWED),
            title: Cow::Owned(title),
            status: StatusCode::METHOD_NOT_ALLOWED.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_METHOD_NOT_ALLOWED),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `409 Conflict` problem response.
    pub fn conflict(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-conflict-title");
        let detail = i18n.translate_simple(&meta.locale, "err-conflict-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_CONFLICT),
            title: Cow::Owned(title),
            status: StatusCode::CONFLICT.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_CONFLICT),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `422 Unprocessable Entity` validation problem response.
    ///
    /// Automatically interpolates `$count` (`invalid_params.len()`) into `err-validation-failed-detail`.
    pub fn validation_failed(
        invalid_params: Vec<InvalidParamError>,
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-validation-failed-title");
        let mut args = FluentArgs::new();
        args.set("count", FluentValue::from(invalid_params.len()));
        let detail = i18n.translate(&meta.locale, "err-validation-failed-detail", Some(&args));

        Self {
            error_type: Cow::Borrowed(TYPE_VALIDATION_ERROR),
            title: Cow::Owned(title),
            status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_VALIDATION_FAILED),
            invalid_params: Some(invalid_params),
            meta,
        }
    }

    /// Constructs a localized `429 Too Many Requests` problem response with `$retry_after` interpolation.
    pub fn rate_limited(
        retry_after_secs: u64,
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-rate-limited-title");
        let mut args = FluentArgs::new();
        args.set("retry_after", FluentValue::from(retry_after_secs));
        let detail = i18n.translate(&meta.locale, "err-rate-limited-detail", Some(&args));

        Self {
            error_type: Cow::Borrowed(TYPE_RATE_LIMITED),
            title: Cow::Owned(title),
            status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_RATE_LIMITED),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a hardened `500 Internal Server Error` problem response (Section 8.1).
    ///
    /// Logs the underlying internal cause with `request_id` and `trace_id` via `tracing::error!`,
    /// while strictly returning a sanitized, localized generic message (`err_internal_server`) to prevent
    /// leaking SQL queries, stack traces, or infrastructure details to clients.
    pub fn internal_server_error(
        internal_cause: impl fmt::Display,
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let instance_cow = instance.into();
        tracing::error!(
            request_id = %meta.request_id,
            trace_id = %meta.trace_id,
            instance = %instance_cow,
            internal_error = %internal_cause,
            "Unhandled internal server error suppressed from external client response"
        );

        let title = i18n.translate_simple(&meta.locale, "err-internal-server-title");
        let detail = i18n.translate_simple(&meta.locale, "err_internal_server");

        Self {
            error_type: Cow::Borrowed(TYPE_INTERNAL_SERVER_ERROR),
            title: Cow::Owned(title),
            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance_cow,
            code: Cow::Borrowed(ERR_INTERNAL_SERVER),
            invalid_params: None,
            meta,
        }
    }

    /// Constructs a localized `503 Service Unavailable` problem response.
    pub fn service_unavailable(
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> Self {
        let title = i18n.translate_simple(&meta.locale, "err-service-unavailable-title");
        let detail = i18n.translate_simple(&meta.locale, "err-service-unavailable-detail");
        Self {
            error_type: Cow::Borrowed(TYPE_SERVICE_UNAVAILABLE),
            title: Cow::Owned(title),
            status: StatusCode::SERVICE_UNAVAILABLE.as_u16(),
            detail: Cow::Owned(detail),
            instance: instance.into(),
            code: Cow::Borrowed(ERR_SERVICE_UNAVAILABLE),
            invalid_params: None,
            meta,
        }
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut headers = HeaderMap::with_capacity(3);
        headers.insert(header::CONTENT_TYPE, PROBLEM_JSON_MIME.clone());

        if let Ok(val) = HeaderValue::from_str(&self.meta.locale) {
            headers.insert(header::CONTENT_LANGUAGE, val);
        }
        if let Ok(val) = HeaderValue::from_str(&self.meta.request_id) {
            headers.insert(HeaderName::from_static(X_REQUEST_ID_HEADER), val);
        }

        (status, headers, Json(self)).into_response()
    }
}

/// Builder pattern for constructing customized [`ApiErrorResponse`] instances cleanly.
#[derive(Debug, Clone)]
#[must_use]
pub struct ApiErrorResponseBuilder {
    error_type: Cow<'static, str>,
    title: Cow<'static, str>,
    status: u16,
    detail: Cow<'static, str>,
    instance: Cow<'static, str>,
    code: Cow<'static, str>,
    invalid_params: Option<Vec<InvalidParamError>>,
    meta: Option<ResponseMeta>,
}

impl Default for ApiErrorResponseBuilder {
    fn default() -> Self {
        Self {
            error_type: Cow::Borrowed(TYPE_INTERNAL_SERVER_ERROR),
            title: Cow::Borrowed("Internal Server Error"),
            status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            detail: Cow::Borrowed(""),
            instance: Cow::Borrowed("/"),
            code: Cow::Borrowed(ERR_INTERNAL_SERVER),
            invalid_params: None,
            meta: None,
        }
    }
}

impl ApiErrorResponseBuilder {
    /// Sets the RFC 9457 `type` URI.
    pub fn error_type(mut self, error_type: impl Into<Cow<'static, str>>) -> Self {
        self.error_type = error_type.into();
        self
    }

    /// Sets the localized problem `title`.
    pub fn title(mut self, title: impl Into<Cow<'static, str>>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the HTTP `status` code (`u16`).
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Sets the HTTP `status` from an Axum [`StatusCode`].
    pub fn status_code(mut self, status: StatusCode) -> Self {
        self.status = status.as_u16();
        self
    }

    /// Sets the localized problem `detail`.
    pub fn detail(mut self, detail: impl Into<Cow<'static, str>>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Sets the endpoint URI `instance`.
    pub fn instance(mut self, instance: impl Into<Cow<'static, str>>) -> Self {
        self.instance = instance.into();
        self
    }

    /// Sets the machine-readable domain error `code`.
    pub fn code(mut self, code: impl Into<Cow<'static, str>>) -> Self {
        self.code = code.into();
        self
    }

    /// Sets the list of `invalid_params` field violations.
    pub fn invalid_params(mut self, invalid_params: Vec<InvalidParamError>) -> Self {
        self.invalid_params = Some(invalid_params);
        self
    }

    /// Appends a single [`InvalidParamError`] to `invalid_params`.
    pub fn add_invalid_param(mut self, param: InvalidParamError) -> Self {
        self.invalid_params.get_or_insert_with(Vec::new).push(param);
        self
    }

    /// Sets the correlation & localization [`ResponseMeta`].
    pub fn meta(mut self, meta: ResponseMeta) -> Self {
        self.meta = Some(meta);
        self
    }

    /// Builds the final [`ApiErrorResponse`].
    pub fn build(self) -> ApiErrorResponse {
        ApiErrorResponse {
            error_type: self.error_type,
            title: self.title,
            status: self.status,
            detail: self.detail,
            instance: self.instance,
            code: self.code,
            invalid_params: self.invalid_params,
            meta: self.meta.unwrap_or_default(),
        }
    }
}
