//! Axum RED metrics middleware and RAII in-flight request tracker.

use std::{borrow::Cow, time::Instant};

use axum::{
    body::Body,
    extract::{MatchedPath, Request},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use metrics::{counter, gauge, histogram};
use omni_common::middleware::is_health_probe_path;
use omni_errors::{
    codes::{
        ERR_BAD_REQUEST, ERR_CONFLICT, ERR_FORBIDDEN, ERR_INTERNAL_SERVER, ERR_METHOD_NOT_ALLOWED,
        ERR_NOT_FOUND, ERR_RATE_LIMITED, ERR_SERVICE_UNAVAILABLE, ERR_UNAUTHORIZED,
        ERR_VALIDATION_FAILED,
    },
    problem::ErrorCodeExtension,
};

use crate::sanitizer::{UNMATCHED_ROUTE_LABEL, normalize_matched_path, sanitize_uri_path};

/// Tên các chỉ số RED chuẩn hóa.
pub const METRIC_HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
pub const METRIC_HTTP_REQUEST_ERRORS_TOTAL: &str = "http_request_errors_total";
pub const METRIC_HTTP_REQUEST_DURATION_SECONDS: &str = "http_request_duration_seconds";
pub const METRIC_HTTP_REQUESTS_IN_FLIGHT: &str = "http_requests_in_flight";

/// RAII Guard bảo đảm chỉ số `http_requests_in_flight` luôn được giảm trừ chính xác
/// ngay cả khi Future bị hủy (Client disconnect / Timeout) hoặc Handler gặp Panic.
#[derive(Debug)]
pub struct InFlightGuard {
    method: String,
    route: String,
}

impl InFlightGuard {
    /// Khởi tạo guard và tăng chỉ số in-flight.
    pub fn new(method: String, route: String) -> Self {
        gauge!(
            METRIC_HTTP_REQUESTS_IN_FLIGHT,
            "method" => method.clone(),
            "route" => route.clone()
        )
        .increment(1.0);
        Self { method, route }
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        gauge!(
            METRIC_HTTP_REQUESTS_IN_FLIGHT,
            "method" => self.method.clone(),
            "route" => self.route.clone()
        )
        .decrement(1.0);
    }
}

/// Axum Middleware thu thập bộ chỉ số RED (Rate, Errors, Duration) và In-Flight Requests.
pub async fn red_metrics_middleware(req: Request<Body>, next: Next) -> Response {
    let raw_path = req.uri().path();

    // 1. Bỏ qua các endpoint hạ tầng (/livez, /readyz, /startupz, /healthz, /metrics)
    if is_health_probe_path(raw_path) {
        return next.run(req).await;
    }

    let method = req.method().as_str().to_string();

    // 2. Trích xuất MatchedPath nếu có sẵn, hoặc chuẩn hóa sơ bộ cho InFlightGuard
    let matched_route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|mp| normalize_matched_path(mp.as_str()).into_owned());

    let in_flight_route = matched_route
        .clone()
        .unwrap_or_else(|| sanitize_uri_path(raw_path));

    // 3. Kích hoạt RAII Guard đếm số request đang xử lý đồng thời
    let _in_flight_guard = InFlightGuard::new(method.clone(), in_flight_route.clone());
    let start_instant = Instant::now();

    // 4. Thực thi chuỗi Middleware & Handler phía sau
    let response = next.run(req).await;

    // 5. Tính toán thời lượng thực thi (đơn vị: giây) và mã trạng thái HTTP
    let duration_secs = start_instant.elapsed().as_secs_f64();
    let status = response.status();
    let status_code_str = status.as_u16().to_string();

    // 6. Chốt nhãn route cuối cùng: Nếu không khớp route nào (MatchedPath == None) và trả về 404,
    // gán vào "__unmatched__" để chống tấn công quét URL ngẫu nhiên (404 Cardinality DoS).
    let final_route: Cow<'static, str> = match matched_route {
        Some(route) => Cow::Owned(route),
        None if status == StatusCode::NOT_FOUND => Cow::Borrowed(UNMATCHED_ROUTE_LABEL),
        None => Cow::Owned(in_flight_route),
    };

    // 7. Ghi nhận Rate (Counter) và Duration (Histogram)
    counter!(
        METRIC_HTTP_REQUESTS_TOTAL,
        "method" => method.clone(),
        "route" => final_route.clone(),
        "status_code" => status_code_str.clone()
    )
    .increment(1);

    histogram!(
        METRIC_HTTP_REQUEST_DURATION_SECONDS,
        "method" => method.clone(),
        "route" => final_route.clone(),
        "status_code" => status_code_str.clone()
    )
    .record(duration_secs);

    // 8. Ghi nhận Errors (Counter) nếu HTTP Status >= 400
    if status.is_client_error() || status.is_server_error() {
        let error_code: Cow<'static, str> = response
            .extensions()
            .get::<ErrorCodeExtension>()
            .map_or_else(
                || Cow::Borrowed(fallback_error_code_for_status(status)),
                |ext| ext.0.clone(),
            );

        let error_class: &'static str = if status.is_server_error() {
            "5xx"
        } else {
            "4xx"
        };

        counter!(
            METRIC_HTTP_REQUEST_ERRORS_TOTAL,
            "method" => method,
            "route" => final_route,
            "status_code" => status_code_str,
            "error_code" => error_code,
            "error_class" => error_class
        )
        .increment(1);
    }

    response
}

/// Ánh xạ dự phòng từ HTTP `StatusCode` sang mã lỗi chuẩn của `omni-errors` khi lỗi phát sinh ngoài `ApiErrorResponse`.
#[inline]
#[must_use]
pub const fn fallback_error_code_for_status(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 => ERR_BAD_REQUEST,
        401 => ERR_UNAUTHORIZED,
        403 => ERR_FORBIDDEN,
        404 => ERR_NOT_FOUND,
        405 => ERR_METHOD_NOT_ALLOWED,
        409 => ERR_CONFLICT,
        422 => ERR_VALIDATION_FAILED,
        429 => ERR_RATE_LIMITED,
        503 => ERR_SERVICE_UNAVAILABLE,
        _ => ERR_INTERNAL_SERVER,
    }
}
