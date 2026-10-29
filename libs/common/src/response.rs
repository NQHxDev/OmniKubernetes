//! Enterprise Base Response Data Contracts (`ApiResponse<T>`, `PaginatedResponse<T>`, `ResponseMeta`).
//!
//! Implements the `OmniKubernetes` Base Response Architecture:
//! - Zero-Cost Static String Borrowing via [`Cow<'static, str>`]
//! - Deterministic ISO 8601 / RFC 3339 millisecond UTC timestamps (`YYYY-MM-DDTHH:mm:ss.sssZ`)
//! - Dual Pagination (`Offset` & `Cursor` modes via internally tagged enum)
//! - Strict HTTP Semantics (RFC 9110): `200 OK`, `201 Created` with `Location`, `204 No Content` with empty body

use std::borrow::Cow;

use axum::{
    Json,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, Serializer};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::i18n::DEFAULT_LOCALE;

/// Standard HTTP header name for request correlation.
pub const X_REQUEST_ID_HEADER: &str = "x-request-id";

/// Default machine-readable success code.
pub const CODE_OK: &str = "OK";

/// Default machine-readable created code.
pub const CODE_CREATED: &str = "CREATED";

/// Serializes [`DateTime<Utc>`] into deterministic ISO 8601 / RFC 3339 format with millisecond precision (`YYYY-MM-DDTHH:mm:ss.sssZ`).
fn serialize_rfc3339_millis<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let formatted = dt.to_rfc3339_opts(SecondsFormat::Millis, true);
    serializer.serialize_str(&formatted)
}

/// Builds standard correlation and localization headers (`Content-Language` and `X-Request-Id`).
pub fn build_standard_headers(request_id: &str, locale: &str) -> HeaderMap {
    let mut headers = HeaderMap::with_capacity(2);
    if let Ok(val) = HeaderValue::from_str(locale) {
        headers.insert(header::CONTENT_LANGUAGE, val);
    }
    if let Ok(val) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static(X_REQUEST_ID_HEADER), val);
    }
    headers
}

/// Metadata attached to every HTTP response (except HTTP 204 No Content).
///
/// Intentionally excludes `duration_ms` from the JSON payload to preserve zero-cost serialization;
/// execution latency is transmitted via W3C `Server-Timing` and `X-Response-Time` HTTP headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ResponseMeta {
    /// Unique request identifier (`UUIDv7`) present on 100% of HTTP requests for log correlation.
    #[schema(example = "01a185db-a70e-777a-8cee-9ad64ad10253", value_type = String)]
    pub request_id: Cow<'static, str>,
    /// W3C Distributed Trace ID (32 lowercase hex characters) linked to OpenTelemetry / APM.
    #[schema(example = "4bf92f3577b34da6a3ce929d0e0e4736", value_type = String)]
    pub trace_id: Cow<'static, str>,
    /// UTC timestamp when the response was generated in ISO 8601 / RFC 3339 format (`YYYY-MM-DDTHH:mm:ss.sssZ`).
    #[serde(serialize_with = "serialize_rfc3339_millis")]
    #[schema(example = "2026-10-29T08:30:00.125Z")]
    pub timestamp: DateTime<Utc>,
    /// Resolved BCP 47 locale applied to the response.
    #[schema(example = "vi-VN", value_type = String)]
    pub locale: Cow<'static, str>,
}

impl ResponseMeta {
    /// Creates a new [`ResponseMeta`] with the current UTC timestamp (`Utc::now()`).
    pub fn new(
        request_id: impl Into<Cow<'static, str>>,
        trace_id: impl Into<Cow<'static, str>>,
        locale: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            timestamp: Utc::now(),
            locale: locale.into(),
        }
    }

    /// Creates a new [`ResponseMeta`] with an explicit UTC timestamp (useful for deterministic testing).
    pub fn with_timestamp(
        request_id: impl Into<Cow<'static, str>>,
        trace_id: impl Into<Cow<'static, str>>,
        timestamp: DateTime<Utc>,
        locale: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            timestamp,
            locale: locale.into(),
        }
    }
}

impl Default for ResponseMeta {
    fn default() -> Self {
        Self {
            request_id: Cow::Owned(Uuid::now_v7().to_string()),
            trace_id: Cow::Owned(Uuid::now_v7().simple().to_string()),
            timestamp: Utc::now(),
            locale: Cow::Borrowed(DEFAULT_LOCALE),
        }
    }
}

/// Offset-based pagination metadata for Admin Portal / tabular UIs requiring random page access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OffsetPaginationMeta {
    /// Current 1-based page number.
    #[schema(example = 1)]
    pub page: u64,
    /// Number of items requested per page.
    #[schema(example = 20)]
    pub page_size: u64,
    /// Total number of matching items across all pages.
    #[schema(example = 142)]
    pub total_items: u64,
    /// Total number of available pages.
    #[schema(example = 8)]
    pub total_pages: u64,
    /// Indicates whether a subsequent page exists.
    #[schema(example = true)]
    pub has_next: bool,
    /// Indicates whether a preceding page exists.
    #[schema(example = false)]
    pub has_prev: bool,
}

impl OffsetPaginationMeta {
    /// Calculates offset pagination metadata from `page`, `page_size`, and `total_items`.
    ///
    /// Automatically normalizes `page` and `page_size` to at least `1` to prevent division-by-zero.
    pub fn new(page: u64, page_size: u64, total_items: u64) -> Self {
        let normalized_page = page.max(1);
        let normalized_page_size = page_size.max(1);
        let total_pages = if total_items == 0 {
            0
        } else {
            total_items.div_ceil(normalized_page_size)
        };
        let has_next = total_pages > 0 && normalized_page < total_pages;
        let has_prev = total_items > 0 && normalized_page > 1;

        Self {
            page: normalized_page,
            page_size: normalized_page_size,
            total_items,
            total_pages,
            has_next,
            has_prev,
        }
    }
}

/// Cursor-based / Keyset pagination metadata for $O(1)$ large dataset scanning, infinite scroll, and audit streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CursorPaginationMeta {
    /// Opaque cursor pointing to the next page of items, if available.
    #[schema(example = "ZXhwb3J0X2lkOjAxSkFaWjgwMDE=", value_type = Option<String>)]
    pub next_cursor: Option<Cow<'static, str>>,
    /// Opaque cursor pointing to the previous page of items, if available.
    #[schema(value_type = Option<String>)]
    pub prev_cursor: Option<Cow<'static, str>>,
    /// Indicates whether more items exist beyond the current batch.
    #[schema(example = true)]
    pub has_more: bool,
    /// Maximum number of items returned per batch.
    #[schema(example = 50)]
    pub limit: u32,
}

impl CursorPaginationMeta {
    /// Creates a new [`CursorPaginationMeta`].
    pub fn new(
        limit: u32,
        has_more: bool,
        next_cursor: Option<Cow<'static, str>>,
        prev_cursor: Option<Cow<'static, str>>,
    ) -> Self {
        Self {
            next_cursor,
            prev_cursor,
            has_more,
            limit,
        }
    }

    /// Creates a forward-only cursor pagination metadata instance.
    pub fn forward(
        limit: u32,
        has_more: bool,
        next_cursor: Option<impl Into<Cow<'static, str>>>,
    ) -> Self {
        Self {
            next_cursor: next_cursor.map(Into::into),
            prev_cursor: None,
            has_more,
            limit,
        }
    }
}

/// Internally tagged pagination discriminator supporting both `offset` and `cursor` modes.
///
/// Uses `#[serde(tag = "mode", rename_all = "snake_case")]` for $O(1)$ compile-time type-safe serialization
/// without trial deserialization overhead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PaginationMeta {
    /// Offset-based pagination (`"mode": "offset"`).
    Offset(OffsetPaginationMeta),
    /// Cursor-based / Keyset pagination (`"mode": "cursor"`).
    Cursor(CursorPaginationMeta),
}

/// Combined response metadata for paginated responses (`ResponseMeta` flattened with `pagination`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponseMeta {
    /// Base correlation and localization metadata.
    #[serde(flatten)]
    pub base: ResponseMeta,
    /// Mode-specific pagination metadata (`offset` or `cursor`).
    pub pagination: PaginationMeta,
}

impl PaginatedResponseMeta {
    /// Creates a new [`PaginatedResponseMeta`] from base metadata and pagination details.
    pub fn new(base: ResponseMeta, pagination: PaginationMeta) -> Self {
        Self { base, pagination }
    }

    /// Creates offset-based [`PaginatedResponseMeta`].
    pub fn offset(base: ResponseMeta, offset: OffsetPaginationMeta) -> Self {
        Self {
            base,
            pagination: PaginationMeta::Offset(offset),
        }
    }

    /// Creates cursor-based [`PaginatedResponseMeta`].
    pub fn cursor(base: ResponseMeta, cursor: CursorPaginationMeta) -> Self {
        Self {
            base,
            pagination: PaginationMeta::Cursor(cursor),
        }
    }
}

/// Standard Success Response Envelope using [`Cow<'static, str>`] to eliminate heap allocations for static strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiResponse<T> {
    /// Always `true` for 2xx success envelopes (errors strictly use RFC 9457 `ApiErrorResponse`).
    #[schema(example = true)]
    pub success: bool,
    /// Machine-readable business code (e.g. `"OK"`, `"CREATED"`).
    #[schema(example = "OK", value_type = String)]
    pub code: Cow<'static, str>,
    /// Localized human-readable message rendered via Mozilla Fluent.
    #[schema(example = "Lấy thông tin người dùng thành công.", value_type = String)]
    pub message: Cow<'static, str>,
    /// Optional domain payload. Omitted from JSON if `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Correlation and localization metadata.
    pub meta: ResponseMeta,
}

impl<T> ApiResponse<T> {
    /// Creates an [`ApiResponse<T>`] envelope directly.
    pub fn new(
        data: Option<T>,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> Self {
        Self {
            success: true,
            code: code.into(),
            message: message.into(),
            data,
            meta,
        }
    }

    /// Returns a fluent [`ApiResponseBuilder<T>`] for constructing a customized response.
    pub fn builder() -> ApiResponseBuilder<T> {
        ApiResponseBuilder::default()
    }
}

impl<T: Serialize> ApiResponse<T> {
    /// Constructs a standard HTTP `200 OK` response with `Content-Language` and `X-Request-Id` headers.
    pub fn ok(
        data: T,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<Self>) {
        Self::ok_with_code(data, Cow::Borrowed(CODE_OK), message, meta)
    }

    /// Constructs an HTTP `200 OK` response with a custom domain status `code`.
    pub fn ok_with_code(
        data: T,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<Self>) {
        let headers = build_standard_headers(&meta.request_id, &meta.locale);
        let body = Self {
            success: true,
            code: code.into(),
            message: message.into(),
            data: Some(data),
            meta,
        };

        (StatusCode::OK, headers, Json(body))
    }

    /// Constructs a standard HTTP `201 Created` response with mandatory `Location`, `Content-Language`, and `X-Request-Id` headers.
    pub fn created(
        data: T,
        location_uri: &str,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<Self>) {
        let (_, mut headers, json_body) = Self::ok(data, message, meta);
        if let Ok(val) = HeaderValue::from_str(location_uri) {
            headers.insert(header::LOCATION, val);
        }
        (StatusCode::CREATED, headers, json_body)
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        let headers = build_standard_headers(&self.meta.request_id, &self.meta.locale);
        (StatusCode::OK, headers, Json(self)).into_response()
    }
}

/// Builder pattern for constructing [`ApiResponse<T>`] instances cleanly.
#[derive(Debug, Clone)]
#[must_use]
pub struct ApiResponseBuilder<T> {
    code: Cow<'static, str>,
    message: Cow<'static, str>,
    data: Option<T>,
    meta: Option<ResponseMeta>,
    status: StatusCode,
    location: Option<String>,
}

impl<T> Default for ApiResponseBuilder<T> {
    fn default() -> Self {
        Self {
            code: Cow::Borrowed(CODE_OK),
            message: Cow::Borrowed(""),
            data: None,
            meta: None,
            status: StatusCode::OK,
            location: None,
        }
    }
}

impl<T> ApiResponseBuilder<T> {
    /// Sets the machine-readable business status code (defaults to `"OK"`).
    pub fn code(mut self, code: impl Into<Cow<'static, str>>) -> Self {
        self.code = code.into();
        self
    }

    /// Sets the localized human-readable response message.
    pub fn message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = message.into();
        self
    }

    /// Sets the response data payload.
    pub fn data(mut self, data: T) -> Self {
        self.data = Some(data);
        self
    }

    /// Sets the response correlation & localization metadata.
    pub fn meta(mut self, meta: ResponseMeta) -> Self {
        self.meta = Some(meta);
        self
    }

    /// Sets the HTTP status code (defaults to `200 OK`).
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Configures the response as `201 Created` with the given `Location` header URI.
    pub fn location(mut self, location_uri: impl Into<String>) -> Self {
        self.status = StatusCode::CREATED;
        self.location = Some(location_uri.into());
        self
    }

    /// Builds the [`ApiResponse<T>`] struct.
    pub fn build(self) -> ApiResponse<T> {
        ApiResponse {
            success: true,
            code: self.code,
            message: self.message,
            data: self.data,
            meta: self.meta.unwrap_or_default(),
        }
    }
}

impl<T: Serialize> ApiResponseBuilder<T> {
    /// Builds the full Axum response tuple `(StatusCode, HeaderMap, Json<ApiResponse<T>>)`.
    pub fn build_response(self) -> (StatusCode, HeaderMap, Json<ApiResponse<T>>) {
        let status = self.status;
        let location = self.location.clone();
        let body = self.build();
        let mut headers = build_standard_headers(&body.meta.request_id, &body.meta.locale);
        if let Some(loc) = location
            && let Ok(val) = HeaderValue::from_str(&loc)
        {
            headers.insert(header::LOCATION, val);
        }
        (status, headers, Json(body))
    }
}

/// Paginated Success Response Envelope supporting both Offset and Cursor pagination modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponse<T> {
    /// Always `true` for 2xx success envelopes.
    #[schema(example = true)]
    pub success: bool,
    /// Machine-readable business code (defaults to `"OK"`).
    #[schema(example = "OK", value_type = String)]
    pub code: Cow<'static, str>,
    /// Localized human-readable message.
    #[schema(example = "Truy vấn danh sách đơn hàng thành công.", value_type = String)]
    pub message: Cow<'static, str>,
    /// Collection of items for the current page/cursor slice.
    pub data: Vec<T>,
    /// Correlation, localization, and pagination metadata.
    pub meta: PaginatedResponseMeta,
}

/// Type alias for Offset-based paginated response contract (Section 4.3.A).
pub type PaginatedOffsetResponse<T> = PaginatedResponse<T>;

/// Type alias for Cursor-based / Keyset paginated response contract (Section 4.3.B).
pub type PaginatedCursorResponse<T> = PaginatedResponse<T>;

impl<T: Serialize> PaginatedResponse<T> {
    /// Creates an HTTP `200 OK` paginated response with Offset-based pagination metadata.
    pub fn ok_offset(
        data: Vec<T>,
        pagination: OffsetPaginationMeta,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<Self>) {
        let headers = build_standard_headers(&meta.request_id, &meta.locale);
        let body = Self {
            success: true,
            code: Cow::Borrowed(CODE_OK),
            message: message.into(),
            data,
            meta: PaginatedResponseMeta::offset(meta, pagination),
        };
        (StatusCode::OK, headers, Json(body))
    }

    /// Creates an HTTP `200 OK` paginated response with Cursor-based / Keyset pagination metadata.
    pub fn ok_cursor(
        data: Vec<T>,
        pagination: CursorPaginationMeta,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<Self>) {
        let headers = build_standard_headers(&meta.request_id, &meta.locale);
        let body = Self {
            success: true,
            code: Cow::Borrowed(CODE_OK),
            message: message.into(),
            data,
            meta: PaginatedResponseMeta::cursor(meta, pagination),
        };
        (StatusCode::OK, headers, Json(body))
    }
}

impl<T: Serialize> IntoResponse for PaginatedResponse<T> {
    fn into_response(self) -> Response {
        let headers = build_standard_headers(&self.meta.base.request_id, &self.meta.base.locale);
        (StatusCode::OK, headers, Json(self)).into_response()
    }
}

/// Helper for HTTP `204 No Content` responses.
///
/// Strictly contains **no response body** in compliance with RFC 9110 Section 15.3.5,
/// while preserving `Content-Language` and `X-Request-Id` headers.
pub fn no_content(request_id: &str, locale: &str) -> (StatusCode, HeaderMap) {
    let headers = build_standard_headers(request_id, locale);
    (StatusCode::NO_CONTENT, headers)
}

/// Helper for HTTP `204 No Content` responses using an existing [`ResponseMeta`].
pub fn no_content_from_meta(meta: &ResponseMeta) -> (StatusCode, HeaderMap) {
    no_content(&meta.request_id, &meta.locale)
}
