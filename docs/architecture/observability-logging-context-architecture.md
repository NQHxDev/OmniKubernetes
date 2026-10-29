# OmniKubernetes Architecture Specification: Enterprise Observability, Structured Logging & Distributed Context Architecture

- **Document Version:** `1.0.0`
- **Status:** Approved / Enterprise Architecture Standard
- **Author:** System Architecture Team
- **Target Platform:** Rust (Axum Framework), Tokio Runtime, OpenTelemetry W3C, Envoy Gateway, Kubernetes & CNCF Observability Stack (Grafana Loki, Tempo, OpenTelemetry Collector, Prometheus)

---

## 1. Executive Summary & Core Principles

Trong hệ sinh thái phân tán quy mô doanh nghiệp (**Enterprise Microservices on Kubernetes**) của **OmniKubernetes**, khả năng quan sát (Observability - gồm 3 trụ cột: **Logs, Traces, Metrics**) là yếu tố cốt lõi quyết định năng lực vận hành, phát hiện sự cố sớm (MTTD - Mean Time To Detect) và xử lý sự cố nhanh (MTTR - Mean Time To Resolve).

Khi hàng trăm microservices giao tiếp đồng thời, một lỗi xảy ra ở downstream service có thể dẫn đến hiệu ứng domino làm gián đoạn toàn bộ luồng nghiệp vụ. Do đó, việc thiết lập chuẩn ngữ cảnh phân tán (**Distributed Context**) và ghi log có cấu trúc (**Structured Logging**) là điều kiện tiên quyết.

Tiêu chuẩn kiến trúc này xác lập **6 nguyên lý cốt lõi (Core Architectural Tenets)**:

1. **Chuẩn hóa Ngữ cảnh Kép (Unified Dual-Identifier Correlation):**
    - **`request_id` (UUIDv7 Time-Ordered):** Đảm bảo tính duy nhất trên **100%** HTTP requests để gom log (Log Aggregation trên Loki/ELK) và phục vụ tra cứu sự cố nghiệp vụ/CSKH.
    - **`trace_id` (W3C Trace ID 32-hex):** Đồng bộ xuyên suốt toàn bộ chuỗi phân tán (Distributed APM Spans trên Tempo/Jaeger) tuân theo chuẩn W3C Trace Context.
    - Cả hai định danh bắt buộc phải được gắn kết tự động vào từng dòng log (Log Correlation) và phản hồi về cho Client qua Response Header / Response Metadata.
2. **Ghi log có cấu trúc Không-Cấp-Phát-Thừa (Structured Zero-Allocation Logging):**
    - Mọi log xuất ra `stdout` trong môi trường Kubernetes bắt buộc định dạng **JSON một dòng (Newline-Delimited JSON)** tuân thủ quy chuẩn OpenTelemetry / Elastic Common Schema (ECS).
    - Quá trình format log sử dụng luồng ghi bất đồng bộ non-blocking (`tracing-appender`) tách biệt hoàn toàn khỏi Tokio worker threads để đạt chi phí overhead tiệm cận 0, không gây nghẽn Tokio Reactor.
3. **Tuân thủ triệt để chuẩn W3C Trace Context (Standards-First):**
    - Kế thừa và lan truyền ngữ cảnh phân tán chuẩn hóa thông qua header `traceparent` (W3C Recommendation) và `tracestate`.
    - Tương thích 100% với Envoy Gateway, Istio Service Mesh, OpenTelemetry Collector và Kubernetes Ingress Controller.
4. **Lan truyền ngữ cảnh hai chiều an toàn (Safe Context Propagation):**
    - **Nội bộ tiến trình (In-Process):** Lưu trữ trong Axum Request Extensions và liên kết chặt chẽ với `tracing::Span` của luồng xử lý.
    - **Liên tiến trình (Cross-Process Egress):** Tự động inject `traceparent` và `X-Request-Id` vào HTTP/gRPC headers khi gọi sang các dịch vụ hạ tầng downstream (Database, Redis, Microservices khác).
5. **Lọc nhiễu thông minh (Intelligent Noise Filtering & Low Overhead):**
    - Tự động bỏ qua việc ghi log truy cập (Access Log) và trace span đối với các endpoint thăm dò hạ tầng nội bộ của Kubernetes (`/healthz`, `/livez`, `/readyz`) và Prometheus scraping (`/metrics`) ở mức tải thông thường, loại trừ 80–90% rác dữ liệu trên hệ thống lưu trữ log.
6. **Bảo mật mặc định & Chống rò rỉ dữ liệu (Zero-Leak Security & Sanitization):**
    - Tự động lọc bỏ hoặc che giấu (Masking/Scrubbing) các thông tin nhạy cảm: `Authorization` Bearer tokens, Cookie phiên, mật khẩu, API keys, số thẻ tín dụng, CCCD/CMND theo Nghị định 13/2023/NĐ-CP và GDPR trước khi ghi log ra đĩa hoặc mạng.

---

## 2. Scope of Applicability (Phạm vi áp dụng)

### 2.1. Nằm trong phạm vi (In-Scope)

- **Crate nền tảng `libs/logging`:** Quản lý toàn bộ cấu hình `tracing-subscriber`, non-blocking appender, formatting layers (JSON production vs ANSI console local) và OpenTelemetry tracing bridge.
- **Middleware ngữ cảnh `libs/common/src/middleware/context.rs`:** Bắt chặn mọi Ingress request, trích xuất hoặc khởi tạo `request_id`, parse chuẩn W3C `traceparent`, tạo root span và đóng gói vào `RequestContext`.
- **Axum Extractor `RequestContext`:** Cung cấp định danh an toàn để các Handler nghiệp vụ dễ dàng sử dụng và truyền vào `ResponseMeta` của `base-response-architecture.md`.
- **Egress Propagation Utilities:** Client wrappers (Reqwest, Tower services) tự động mang theo ngữ cảnh sang các downstream calls.

### 2.2. Ngoại lệ ngoài phạm vi (Out-of-Scope / Exclusions)

- **Tầng lưu trữ & trực quan hóa backend:** Hạ tầng triển khai của Grafana Loki, Tempo, OpenTelemetry Collector DaemonSet được quy định trong tài liệu Hạ tầng DevOps (`infra/kubernetes/observability`).
- **Nội dung Business Metrics:** Việc định nghĩa các chỉ số đo lường nghiệp vụ (Business KPIs, Custom Gauges/Counters) do crate `omni-metrics` đảm nhiệm.

---

## 3. Standards & Compliance References

| Tiêu chuẩn / Đặc tả kỹ thuật | Cơ quan ban hành | Ứng dụng cụ thể trong hệ thống OmniKubernetes |
| :--- | :--- | :--- |
| **W3C Trace Context (Level 1)** | W3C Recommendation | Đặc tả cấu trúc header `traceparent` (`00-{trace_id}-{span_id}-{trace_flags}`) và `tracestate`. |
| **RFC 9562 (UUIDv7)** | IETF | Định danh `request_id` sắp xếp theo thời gian (Time-Ordered), tối ưu hóa index trên Log Search Engines (Loki, Elasticsearch). |
| **OpenTelemetry Semantic Conventions (v1.28+)** | CNCF / OpenTelemetry | Chuẩn hóa tên trường log và attributes: `http.request.method`, `http.response.status_code`, `service.name`, `network.peer.address`. |
| **ISO 8601 / RFC 3339** | ISO / IETF | Định dạng thời gian UTC có độ chính xác micro-giây (`YYYY-MM-DDTHH:mm:ss.ssssssZ`). |
| **OpenTracing / Distributed Context (Baggage)** | W3C Recommendation | Cơ chế truyền tải metadata nghiệp vụ phụ (ví dụ: `tenant_id`, `client_id`) qua header `baggage`. |

---

## 4. Distributed Context & Observability Data Contracts

### 4.1. Cấu trúc W3C `traceparent` Header

Chuẩn W3C Trace Context xác định header HTTP mang tên `traceparent` với định dạng 4 thành phần cố định:

```text
traceparent: {version}-{trace_id}-{parent_id}-{trace_flags}
Ví dụ:        00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
```

- **`version`** (`2 hex`): Hiện tại cố định là `00` theo W3C Level 1.
- **`trace_id`** (`32 hex`): Định danh duy nhất toàn cục của toàn bộ chuỗi giao dịch phân tán (16 bytes biểu diễn bằng chuỗi hex).
- **`parent_id`** (`16 hex`): Định danh của span trước đó (Caller Span ID - 8 bytes biểu diễn bằng chuỗi hex).
- **`trace_flags`** (`2 hex`): Cờ điều khiển tracing. Bit thấp nhất (`01`) xác định request này có được lấy mẫu ghi trace hay không (`Sampled = true`).

#### Thuật toán Parsing & Fallback an toàn:

1. Trích xuất header `traceparent` từ Ingress HTTP request.
2. Kiểm tra độ dài chính xác (55 ký tự) và phân tách bằng dấu gạch ngang (`-`).
3. Nếu hợp lệ: Kế thừa `trace_id`, coi `parent_id` là caller span.
4. Nếu header không tồn tại hoặc sai định dạng:
   - Hệ thống tự sinh mới một `trace_id` 32 ký tự ngẫu nhiên (hoặc dựa trên OpenTelemetry Context Generator).
   - Thiết lập span mới làm Root Span của hệ thống.

---

### 4.2. Cấu trúc định danh `request_id` (UUIDv7 Time-Ordered)

Thay vì dùng UUIDv4 hoàn toàn ngẫu nhiên, OmniKubernetes tiêu chuẩn hóa sử dụng **UUIDv7**:

```text
0192d4b8-f2e1-7e83-a4c3-b8162e24cf81
│──────────────│ │───────────────│
  Unix Epoch MS     Randomness
   (48 bits)         (74 bits)
```

- **Khả năng sắp xếp tự nhiên theo thời gian:** Thuận tiện truy vấn và sắp xếp nhật ký theo thứ tự phát sinh trên các công cụ tìm kiếm log.
- **Header chuẩn hóa:** Tiếp nhận từ Gateway qua `X-Request-Id` (nếu Envoy đã sinh), hoặc Microservice đầu tiên sẽ tự sinh nếu chưa có.

---

### 4.3. Đối tượng dữ liệu `RequestContext`

Đây là hạt nhân dữ liệu được đính kèm vào mỗi request trong tiến trình:

```rust
use chrono::{DateTime, Utc};

/// Ngữ cảnh toàn diện của một request đang được xử lý
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Định danh request duy nhất phục vụ gom log (100% requests có giá trị này)
    pub request_id: String,
    /// W3C Distributed Trace ID (32 hex characters)
    pub trace_id: String,
    /// ID của Span hiện tại xử lý request
    pub span_id: String,
    /// Cờ chỉ định request có được lấy mẫu Trace phân tán hay không
    pub is_sampled: bool,
    /// Thời điểm request bắt đầu đi vào middleware của Service
    pub start_time: DateTime<Utc>,
    /// Mã BCP 47 của ngôn ngữ yêu cầu (tích hợp trực tiếp với i18n)
    pub locale: String,
    /// Địa chỉ IP thực tế của client (đã qua bóc tách X-Forwarded-For / Ingress)
    pub client_ip: Option<String>,
    /// User-Agent của Client
    pub user_agent: Option<String>,
    /// Định danh người dùng (được điền sau khi qua tầng Authenticate Token)
    pub user_id: Option<String>,
    /// Định danh tổ chức/Tenant nếu là hệ thống Multi-tenant
    pub tenant_id: Option<String>,
}
```

---

## 5. Structured JSON Logging Contract (Production Schema)

Trong môi trường Kubernetes, nhật ký xuất ra `stdout` theo chuẩn JSON một dòng (NDJSON). Mỗi log record phản ánh đầy đủ thông tin ngữ cảnh để Loki/ELK dễ dàng trích xuất labels:

```json
{
    "timestamp": "2026-10-29T08:30:00.125432Z",
    "level": "INFO",
    "message": "HTTP request completed successfully",
    "target": "omni_common::middleware::observability",
    "service": {
        "name": "order-service",
        "version": "1.2.0",
        "environment": "production"
    },
    "k8s": {
        "pod_name": "order-service-7f4c979d9-x8w9m",
        "namespace": "ecommerce",
        "node_name": "k8s-worker-node-04"
    },
    "trace": {
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "span_id": "00f067aa0ba902b7",
        "sampled": true
    },
    "request": {
        "request_id": "0192d4b8-f2e1-7e83-a4c3-b8162e24cf81",
        "method": "POST",
        "route": "/api/v1/orders",
        "status_code": 201,
        "duration_ms": 14.85,
        "client_ip": "10.244.2.1",
        "user_agent": "OmniClient-iOS/2.4.0",
        "locale": "vi-VN",
        "user_id": "usr_01HZX87Q1K7"
    }
}
```

#### Quy chuẩn môi trường phát triển (Local Development vs Production):

- **Local Development (`APP_ENV=local` hoặc `APP_LOG_FORMAT=pretty`):**
  - Xuất ra terminal dưới định dạng ANSI màu sắc, dễ đọc cho lập trình viên:
    ```text
    2026-10-29T08:30:00.125Z [INFO] [req:0192d4b8... | trace:4bf92f35...] HTTP POST /api/v1/orders -> 201 (14.85ms)
    ```
- **Staging / Production (`APP_ENV=production` hoặc `APP_LOG_FORMAT=json`):**
  - Bắt buộc xuất định dạng JSON tối giản hóa, không màu sắc, tối ưu hóa parsing tốc độ cao cho Vector / Promtail.

---

## 6. Rust Implementation Architecture Specification

### 6.1. Kiến trúc Crate `libs/logging`

Thư viện chịu trách nhiệm khởi tạo subscriber toàn cục cho mỗi process microservice:

```text
libs/logging/
├── Cargo.toml
└── src/
    ├── lib.rs              <-- Entrypoint init_logging() & LoggingGuard
    ├── config.rs           <-- LogConfig (Level, Format, Service Info)
    ├── formatter/
    │   ├── mod.rs
    │   ├── json.rs         <-- Zero-allocation JSON formatter tuân thủ OTel
    │   └── pretty.rs       <-- Terminal color formatter cho Dev
    ├── sanitizer.rs        <-- Logic Redaction & Masking PII/Tokens
    └── otel.rs             <-- Bridge OpenTelemetry Tracing Pipeline (gRPC/OTLP)
```

#### Code mẫu khởi tạo Logger (`libs/logging/src/lib.rs`):

```rust
use std::io;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer, Registry,
};

pub struct LoggingGuard {
    _worker_guard: WorkerGuard,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub service_name: String,
    pub environment: String,
    pub log_level: String,
    pub log_format: LogFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Pretty,
}

/// Khởi tạo hệ thống Tracing Subscriber chuẩn Enterprise
pub fn init_logging(config: &LoggingConfig) -> LoggingGuard {
    // 1. Cấu hình Non-blocking Appender cho stdout
    let (non_blocking, guard) = tracing_appender::non_blocking(io::stdout());

    // 2. Thiết lập EnvFilter lọc log linh hoạt
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    // 3. Định hình Layer tùy theo môi trường
    let registry = Registry::default().with(filter);

    match config.log_format {
        LogFormat::Json => {
            let json_layer = tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .boxed();
            registry.with(json_layer).init();
        }
        LogFormat::Pretty => {
            let pretty_layer = tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .pretty()
                .with_thread_ids(false)
                .boxed();
            registry.with(pretty_layer).init();
        }
    }

    LoggingGuard { _worker_guard: guard }
}
```

---

### 6.2. Middleware Ngữ cảnh & Observability Ingress (`libs/common/src/middleware/observability.rs`)

Middleware này được gắn tại tầng cao nhất của Axum Router để đảm bảo bọc trọn chu kỳ sống của request:

```rust
use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use std::time::Instant;
use tracing::{info_span, Instrument};
use uuid::Uuid;

use crate::context::RequestContext;

static X_REQUEST_ID: &str = "x-request-id";
static TRACEPARENT: &str = "traceparent";

/// Middleware chính khởi tạo RequestContext, Span Tracing và ghi Access Log
pub async fn observability_middleware(mut req: Request<Body>, next: Next) -> Response {
    let start_instant = Instant::now();
    let start_time = Utc::now();
    let path = req.uri().path().to_string();

    // 1. Kiểm tra lọc nhiễu Kubernetes Probes
    let is_health_check = path == "/healthz" || path == "/livez" || path == "/readyz" || path == "/metrics";

    // 2. Trích xuất hoặc sinh mới X-Request-Id (UUIDv7)
    let request_id = extract_or_generate_request_id(req.headers());

    // 3. Trích xuất hoặc sinh mới W3C TraceContext
    let (trace_id, parent_span_id, is_sampled) = extract_or_generate_trace_context(req.headers());
    let current_span_id = generate_span_id();

    // 4. Trích xuất Client IP & User Agent
    let client_ip = extract_client_ip(req.headers());
    let user_agent = req.headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 5. Xác định Locale yêu cầu
    let locale = req.headers()
        .get("x-locale")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("vi-VN")
        .to_string();

    // 6. Đóng gói RequestContext và đưa vào Extensions
    let context = RequestContext {
        request_id: request_id.clone(),
        trace_id: trace_id.clone(),
        span_id: current_span_id.clone(),
        is_sampled,
        start_time,
        locale: locale.clone(),
        client_ip,
        user_agent,
        user_id: None,
        tenant_id: None,
    };
    req.extensions_mut().insert(context);

    // 7. Tạo Root Tracing Span liên kết định danh
    let method = req.method().to_string();
    let span = info_span!(
        "http_request",
        http.method = %method,
        http.route = %path,
        request_id = %request_id,
        trace_id = %trace_id,
        span_id = %current_span_id,
    );

    // 8. Thực thi downstream handler bên trong Span Context
    let mut response = next.run(req).instrument(span).await;

    // 9. Tính toán thời gian thực thi
    let elapsed = start_instant.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    // 10. Gắn Header phản hồi chuẩn hóa
    inject_response_headers(&mut response, &request_id, &trace_id, &current_span_id, duration_ms, &locale);

    // 11. Ghi Access Log có cấu trúc (Bỏ qua health check nếu thành công)
    if !is_health_check || !response.status().is_success() {
        tracing::info!(
            target: "omni_common::access_log",
            method = %method,
            path = %path,
            status = response.status().as_u16(),
            duration_ms = duration_ms,
            request_id = %request_id,
            trace_id = %trace_id,
            "Handled HTTP request"
        );
    }

    response
}

/// Helper trích xuất hoặc sinh mới UUIDv7
fn extract_or_generate_request_id(headers: &HeaderMap) -> String {
    if let Some(val) = headers.get(X_REQUEST_ID).and_then(|v| v.to_str().ok()) {
        if !val.trim().is_empty() {
            return val.to_string();
        }
    }
    Uuid::now_v7().to_string()
}

/// Helper bóc tách W3C traceparent (format: 00-trace_id-parent_id-flags)
fn extract_or_generate_trace_context(headers: &HeaderMap) -> (String, Option<String>, bool) {
    if let Some(val) = headers.get(TRACEPARENT).and_then(|v| v.to_str().ok()) {
        let parts: Vec<&str> = val.split('-').collect();
        if parts.len() == 4 && parts[0] == "00" && parts[1].len() == 32 && parts[2].len() == 16 {
            let trace_id = parts[1].to_string();
            let parent_id = Some(parts[2].to_string());
            let sampled = parts[3] == "01";
            return (trace_id, parent_id, sampled);
        }
    }
    // Fallback: Sinh mới 32 ký tự hex (16 bytes)
    let new_trace_id = format!("{:032x}", rand::random::<u128>());
    (new_trace_id, None, true)
}

fn generate_span_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    // 1. Kiểm tra X-Forwarded-For từ Gateway / Ingress
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first_ip) = xff.split(',').next() {
            return Some(first_ip.trim().to_string());
        }
    }
    // 2. Kiểm tra X-Real-IP
    headers.get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
}

fn inject_response_headers(
    res: &mut Response,
    request_id: &str,
    trace_id: &str,
    span_id: &str,
    duration_ms: f64,
    locale: &str,
) {
    let headers = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static("x-request-id"), v);
    }
    let traceparent_resp = format!("00-{}-{}-01", trace_id, span_id);
    if let Ok(v) = HeaderValue::from_str(&traceparent_resp) {
        headers.insert(HeaderName::from_static("traceparent"), v);
    }
    let timing = format!("total;dur={:.2}", duration_ms);
    if let Ok(v) = HeaderValue::from_str(&timing) {
        headers.insert(HeaderName::from_static("server-timing"), v);
    }
    if let Ok(v) = HeaderValue::from_str(locale) {
        headers.insert(header::CONTENT_LANGUAGE, v);
    }
}
```

---

### 6.3. Axum FromRequestParts Extractor (`RequestContext`)

Giúp các Handler trích xuất ngữ cảnh chỉ với một khai báo tham số duy nhất:

```rust
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

#[async_trait]
impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<RequestContext>()
            .cloned()
            .ok_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                "RequestContext missing. Ensure ObservabilityMiddleware is configured on router.",
            ))
    }
}
```

#### Áp dụng thực tế trong Handler:

```rust
use axum::Json;
use omni_common::context::RequestContext;
use omni_common::response::{ApiResponse, ResponseMeta};

pub async fn get_user_profile(
    ctx: RequestContext,
) -> (axum::http::StatusCode, axum::http::HeaderMap, Json<ApiResponse<UserProfileDto>>) {
    let meta = ResponseMeta {
        request_id: ctx.request_id,
        trace_id: ctx.trace_id,
        timestamp: chrono::Utc::now(),
        locale: ctx.locale,
    };

    let user_data = UserProfileDto { /* ... */ };
    ApiResponse::ok(user_data, "Lấy thông tin người dùng thành công", meta)
}
```

---

### 6.4. Lan truyền ngữ cảnh sang Downstream (Egress HTTP Client Propagation)

Khi microservice cần gọi sang một dịch vụ khác (ví dụ: `order-service` gọi `payment-service`), ngữ cảnh bắt buộc phải được lan truyền tiếp tục (Distributed Tracing Propagation):

```rust
use reqwest::{Client, RequestBuilder};
use crate::context::RequestContext;

/// Tiêm W3C traceparent và X-Request-Id vào HTTP Outgoing Request
pub fn inject_egress_context(builder: RequestBuilder, ctx: &RequestContext) -> RequestBuilder {
    let next_span_id = format!("{:016x}", rand::random::<u64>());
    let traceparent = format!("00-{}-{}-01", ctx.trace_id, next_span_id);

    builder
        .header("x-request-id", &ctx.request_id)
        .header("traceparent", traceparent)
        .header("x-locale", &ctx.locale)
}
```

---

## 7. Distributed Tracing Flow & Kubernetes Ingress Pipeline

Sơ đồ tuần tự minh họa dòng chảy của ngữ cảnh quan sát xuyên suốt cụm Kubernetes:

```text
  Client Browser / Mobile
             │
             │ HTTPS Request (Optional: X-Request-Id, traceparent)
             ▼
┌─────────────────────────────────────────────────────────────┐
│  Kubernetes Ingress / Envoy Gateway                         │
│  - Tạo X-Request-Id (UUIDv7) nếu Client chưa có             │
│  - Khởi tạo W3C traceparent nếu Client chưa có              │
│  - Ghi Access Log của Envoy kèm cả 2 định danh              │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               │ Forward: Hostname ClusterIP
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  OmniKubernetes Pod (e.g., auth-service)                    │
│                                                             │
│  [Observability Middleware]                                 │
│  ├── 1. Trích xuất X-Request-Id & traceparent               │
│  ├── 2. Khởi tạo RequestContext lưu vào Extensions          │
│  ├── 3. Tạo Span 'http_request'                             │
│  │                                                          │
│  ▼ [Business Logic & SQL Query]                             │
│  ├── Log: "Bắt đầu xác thực token..." (Có trace_id & req_id)│
│  ├── Database Query: span 'db.query SELECT users'           │
│  │                                                          │
│  ▼ [Egress Call: notification-service]                      │
│  ├── Tiêm traceparent & X-Request-Id vào Reqwest            │
│  │                                                          │
│  ▼ [Middleware Egress]                                      │
│  ├── Tính duration_ms = start.elapsed()                     │
│  ├── Gắn Header: Server-Timing, X-Request-Id, traceparent   │
│  └── Ghi structured JSON Access Log                         │
└──────────────────────────────┬──────────────────────────────┘
                               │
               Stdout NDJSON   │   OTLP gRPC Spans
         ┌─────────────────────┴──────────────────────┐
         ▼                                            ▼
┌──────────────────────────────┐        ┌──────────────────────────────┐
│  Log Collector (Vector/Loki) │        │  OpenTelemetry Collector     │
│  - Parser JSON labels        │        │  - Batch Processor           │
│  - Lưu trữ trên Grafana Loki │        │  - Xuất sang Grafana Tempo   │
└──────────────────────────────┘        └──────────────────────────────┘
```

---

## 8. Security, PII Masking & Production Hardening

### 8.1. Quy chuẩn che giấu dữ liệu nhạy cảm (PII & Secrets Scrubbing)

Tuyệt đối không để rò rỉ các thông tin cá nhân và chứng thực bí mật vào hệ thống Log tập trung.

Crate `libs/logging` cài đặt bộ lọc tự động (**Log Sanitizer**) dựa trên danh sách đen các trường nhạy cảm:

| Tên trường / Header | Hành vi che giấu (Masking Policy) | Ví dụ giá trị hiển thị |
| :--- | :--- | :--- |
| `authorization` / `token` | Giữ 6 ký tự đầu, ẩn phần còn lại | `Bearer eyJhbG...[REDACTED]` |
| `password` / `secret` / `pin` | Ẩn hoàn toàn | `[REDACTED]` |
| `credit_card` / `cvv` | Chỉ giữ 4 số cuối | `****-****-****-1234` |
| `id_number` (CCCD/CMND) | Chỉ giữ 3 số đầu và 3 số cuối | `001***982` |
| `email` | Mask ký tự tên hòm thư | `n***n@example.com` |

### 8.2. Cơ chế Non-blocking Buffer & Chống tràn bộ nhớ (Backpressure)

- Sử dụng `tracing_appender::non_blocking` với kích thước buffer định trước.
- Khi tải tăng đột biến hàng trăm nghìn log/giây, luồng ghi chạy nền trong thread riêng biệt. Nếu buffer đầy:
  - Mặc định áp dụng chiến lược **Drop Logs** đối với log `DEBUG`/`INFO` thay vì block Tokio runtime threads, bảo vệ tính sẵn sàng và sống còn của microservice.
  - Các log `ERROR` và `WARN` được ưu tiên giữ lại trong bộ đệm.

### 8.3. Graceful Shutdown & Buffer Flush

Khi Kubernetes gửi tín hiệu `SIGTERM` để hủy Pod, ứng dụng phải xả toàn bộ buffer log còn tồn dư ra stdout trước khi tiến trình tắt hoàn toàn. 

Đối tượng `LoggingGuard` đảm bảo thực hiện thao tác flush trong hàm `Drop`:
```rust
#[tokio::main]
async fn main() {
    let logging_config = LoggingConfig::from_env();
    let _guard = omni_logging::init_logging(&logging_config);

    // Khi nhận tín hiệu SIGTERM, luồng kết thúc, _guard sẽ bị drop
    // và flush toàn bộ log còn lại trong non_blocking worker thread ra stdout.
}
```

---

## 9. Workspace Organization in OmniKubernetes

```text
OmniKubernetes/
├── libs/
│   ├── logging/                           <-- Crate omni-logging
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                     <-- init_logging, LoggingGuard
│   │       ├── config.rs                  <-- LoggingConfig, LogFormat
│   │       ├── sanitizer.rs               <-- PII / Sensitive masking
│   │       └── visitor.rs                 <-- Field extractor cho tracing
│   │
│   ├── common/                            <-- Crate omni-common
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── context.rs                 <-- RequestContext struct & Extractor
│   │       ├── response.rs                <-- ApiResponse, ResponseMeta
│   │       └── middleware/
│   │           ├── mod.rs
│   │           ├── observability.rs       <-- Ingress Context & Access Log Middleware
│   │           └── timing.rs              <-- Server-Timing calculation
│   │
│   └── errors/                            <-- Crate omni-errors
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           └── problem.rs                 <-- RFC 9457 ApiErrorResponse
│
└── docs/
    └── architecture/
        ├── base-response-architecture.md  <-- Đặc tả Base Response Envelope
        ├── api-documentation-architecture.md <-- Đặc tả OpenAPI Aggregation
        └── observability-logging-context-architecture.md <-- Tài liệu đặc tả chuẩn này
```

---

## 10. Verification & Quality Acceptance Criteria

Để hoàn thành triển khai theo đặc tả này, mã nguồn cần thỏa mãn 100% các tiêu chí nghiệm thu sau:

1. **Unit Test Phân tách Trace Context:**
   - Parse chính xác `traceparent` hợp lệ của W3C.
   - Xử lý mượt mà và tự sinh fallback an toàn khi header `traceparent` bị lỗi hoặc thiếu.
2. **Unit Test Định danh UUIDv7:**
   - `request_id` sinh mới phải là UUIDv7 chuẩn RFC 9562 và có tính chất tăng dần theo thời gian.
3. **Integration Test Axum Middleware:**
   - Mọi response trả về từ Router đều sở hữu các headers: `x-request-id`, `traceparent`, `server-timing`, `content-language`.
   - `RequestContext` được trích xuất thành công trong các handlers.
4. **Noise Filter Verification:**
   - Các requests gửi tới `/healthz`, `/livez`, `/readyz`, `/metrics` với mã HTTP 200 không phát sinh Access Log trên stdout.
5. **PII Masking Verification:**
   - Các trường mang nhãn nhạy cảm (`password`, `authorization`) tuyệt đối không xuất hiện nguyên bản dạng plain text trên log output.
