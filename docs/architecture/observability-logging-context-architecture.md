# OmniKubernetes Architecture Specification: Enterprise Observability, Structured Logging & Distributed Context Architecture

- **Document Version:** `1.1.0`
- **Status:** Verified
- **Author:** System Architecture Team
- **Target Platform:** Rust (Axum Framework 0.8+, Tokio Runtime 1.43+), OpenTelemetry W3C, Envoy Gateway, Kubernetes & CNCF Observability Stack (Grafana Loki, Tempo, OpenTelemetry Collector, Prometheus)

---

## 1. Executive Summary & Core Principles

Trong hệ sinh thái phân tán quy mô doanh nghiệp (**Enterprise Microservices on Kubernetes**) của **OmniKubernetes**, khả năng quan sát (Observability - gồm 3 trụ cột: **Logs, Traces, Metrics**) là yếu tố cốt lõi quyết định năng lực vận hành, phát hiện sự cố sớm (MTTD - Mean Time To Detect) và xử lý sự cố nhanh (MTTR - Mean Time To Resolve).

Khi hàng trăm microservices giao tiếp đồng thời, một lỗi xảy ra ở downstream service có thể dẫn đến hiệu ứng domino làm gián đoạn toàn bộ luồng nghiệp vụ. Do đó, việc thiết lập chuẩn ngữ cảnh phân tán (**Distributed Context**) và ghi log có cấu trúc (**Structured Logging**) là điều kiện tiên quyết.

Tiêu chuẩn kiến trúc này xác lập **6 nguyên lý cốt lõi (Core Architectural Tenets)**:

1. **Chuẩn hóa Ngữ cảnh Kép (Unified Dual-Identifier Correlation):**
   - **`request_id` (UUIDv7 Time-Ordered):** Đảm bảo tính duy nhất trên **100%** HTTP requests để gom log (Log Aggregation trên Loki/ELK) và phục vụ tra cứu sự cố nghiệp vụ/CSKH.
   - **`trace_id` (W3C Trace ID 32-hex):** Đồng bộ xuyên suốt toàn bộ chuỗi phân tán (Distributed APM Spans trên Tempo/Jaeger) tuân theo chuẩn W3C Trace Context.
   - Cả hai định danh bắt buộc phải được gắn kết tự động vào từng dòng log (Log Correlation) và phản hồi về cho Client qua Response Header / Response Metadata (`ResponseMeta`).
2. **Ghi log có cấu trúc Tối ưu Cấp phát (Structured Heap-Minimized Logging) & Non-blocking I/O:**
   - Mọi log xuất ra `stdout` trong môi trường Kubernetes bắt buộc định dạng **JSON một dòng (Newline-Delimited JSON)** tuân thủ quy chuẩn OpenTelemetry / Elastic Common Schema (ECS).
   - Quá trình format log sử dụng luồng ghi bất đồng bộ non-blocking (`tracing-appender`) tách biệt hoàn toàn khỏi Tokio worker threads qua bounded buffer channel, bảo đảm không bao giờ gây nghẽn (block) Tokio Reactor.
3. **Tuân thủ triệt để chuẩn W3C Trace Context (Standards-First):**
   - Kế thừa và lan truyền ngữ cảnh phân tán chuẩn hóa thông qua header `traceparent` (W3C Recommendation) và `tracestate` (bảo toàn vendor metadata và routing tags).
   - Tương thích 100% với Envoy Gateway, Istio Service Mesh, OpenTelemetry Collector và Kubernetes Ingress Controller.
4. **Lan truyền ngữ cảnh hai chiều an toàn (Safe Context Propagation):**
   - **Nội bộ tiến trình (In-Process):** Lưu trữ trong Axum Request Extensions và liên kết chặt chẽ với `tracing::Span` của luồng xử lý. Kế thừa chính xác OTel Parent Context để bảo toàn liên kết cha-con (Parent-Child Span Hierarchy).
   - **Liên tiến trình (Cross-Process Egress):** Tự động inject `traceparent` (sử dụng Span ID hiện tại làm `parent_id`, cờ sampling thực tế) và `X-Request-Id` vào HTTP/gRPC headers khi gọi sang các dịch vụ hạ tầng downstream (Database, Redis, Microservices khác). Tuyệt đối không sinh ngẫu nhiên `parent_id` làm gãy cây APM trace.
5. **Lọc nhiễu thông minh (Intelligent Noise Filtering & Low Overhead):**
   - Tự động bỏ qua việc tạo APM Root Span và ghi log truy cập (Access Log) đối với các endpoint thăm dò hạ tầng nội bộ của Kubernetes (`/healthz`, `/livez`, `/readyz`) và Prometheus scraping (`/metrics`) ở mức tải thông thường, loại trừ 80–90% rác dữ liệu trên hệ thống lưu trữ log và APM backend.
6. **Bảo mật mặc định & Chống rò rỉ dữ liệu (Zero-Leak Security & Sanitization):**
   - Tự động lọc bỏ hoặc che giấu (Masking/Scrubbing) các thông tin nhạy cảm: `Authorization` Bearer tokens, Cookie phiên, mật khẩu, API keys, số thẻ tín dụng, CCCD/CMND theo Nghị định 13/2023/NĐ-CP và GDPR trước khi ghi log ra đĩa hoặc mạng.
   - Chống giả mạo địa chỉ IP (IP Spoofing) bằng cách thiết lập mô hình tin cậy (Trusted Proxies / Envoy Gateway) thay vì đọc mù quáng `X-Forwarded-For`.

---

## 2. Scope of Applicability (Phạm vi áp dụng)

### 2.1. Nằm trong phạm vi (In-Scope)

- **Crate nền tảng `libs/logging` (`omni-logging`):** Quản lý cấu hình `tracing-subscriber`, non-blocking appender, formatting layers (JSON production vs ANSI console local), bộ lọc PII sanitizer, và OpenTelemetry tracing pipeline bridge (gRPC/OTLP) xuất sang Grafana Tempo.
- **Middleware ngữ cảnh `libs/common/src/middleware/context.rs` & `observability.rs`:** Bắt chặn Ingress request, trích xuất hoặc khởi tạo `request_id` (UUIDv7), parse chuẩn W3C `traceparent`, đồng bộ OTel Context, tạo tracing span, đo lường thời gian thực thi và đóng gói vào `RequestContext`.
- **Axum Extractor `RequestContext` & `ResponseMeta`:** Cung cấp định danh an toàn (Infallible Extraction) để các Handler nghiệp vụ dễ dàng sử dụng và truyền vào `ResponseMeta` theo quy chuẩn `base-response-architecture.md`.
- **Egress Propagation Utilities:** Client wrappers (Reqwest, Tower services) tự động mang theo ngữ cảnh phân tán sang downstream calls.

### 2.2. Phân định ranh giới (Single Source of Truth)

- **Headers phản hồi (Egress Headers):** `observability_middleware` đóng vai trò là điểm kiểm soát duy nhất (Single Source of Truth) chịu trách nhiệm tự động tiêm `x-request-id`, `traceparent`, `server-timing`, `content-language` vào HTTP Response. Handler nghiệp vụ chỉ cần trả về dữ liệu nghiệp vụ và `ResponseMeta` trong body JSON mà không cần tự chèn các header này thủ công.
- **Tầng lưu trữ & trực quan hóa backend:** Hạ tầng triển khai của Grafana Loki, Tempo, OpenTelemetry Collector DaemonSet được quy định trong tài liệu Hạ tầng DevOps (`infra/kubernetes/observability`).
- **Nội dung Business Metrics:** Việc định nghĩa các chỉ số đo lường nghiệp vụ (Business KPIs, Custom Gauges/Counters) do crate `omni-metrics` đảm nhiệm.

---

## 3. Standards & Compliance References

| Tiêu chuẩn / Đặc tả kỹ thuật                    | Cơ quan ban hành     | Ứng dụng cụ thể trong hệ thống OmniKubernetes                                                                                                           |
| :---------------------------------------------- | :------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **W3C Trace Context (Level 1)**                 | W3C Recommendation   | Đặc tả cấu trúc header `traceparent` (`00-{trace_id}-{parent_id}-{trace_flags}`) và `tracestate`.                                                       |
| **RFC 9562 (UUIDv7)**                           | IETF                 | Định danh `request_id` sắp xếp theo thời gian (Time-Ordered), tối ưu hóa index trên Log Search Engines (Loki, Elasticsearch).                           |
| **OpenTelemetry Semantic Conventions (v1.28+)** | CNCF / OpenTelemetry | Chuẩn hóa tên trường log và span attributes: `http.request.method`, `http.route`, `http.response.status_code`, `client.address`, `user_agent.original`. |
| **RFC 9457 (Problem Details)**                  | IETF                 | Đặc tả cấu trúc trả về mã lỗi HTTP chuẩn hóa (`ApiErrorResponse`) khi trích xuất hoặc xử lý ngữ cảnh gặp ngoại lệ.                                      |
| **ISO 8601 / RFC 3339**                         | ISO / IETF           | Định dạng thời gian UTC có độ chính xác micro-giây (`YYYY-MM-DDTHH:mm:ss.ssssssZ`).                                                                     |
| **BCP 47 / RFC 4647**                           | IETF                 | Chuẩn hóa mã ngôn ngữ quốc tế (`locale`) được tích hợp đồng bộ từ Request Context sang Response Meta.                                                   |

---

## 4. Distributed Context & Observability Data Contracts

### 4.1. Cấu trúc W3C `traceparent` Header

Chuẩn W3C Trace Context xác định header HTTP mang tên `traceparent` với định dạng 4 thành phần cố định:

```text
traceparent: {version}-{trace_id}-{parent_id}-{trace_flags}
Ví dụ:        00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
```

- **`version`** (`2 hex`): Hiện tại cố định là `00` theo W3C Level 1. Phiên bản `ff` là bất hợp lệ. Nếu phiên bản là `00`, chuỗi bắt buộc phải có đúng 4 thành phần.
- **`trace_id`** (`32 hex`): Định danh duy nhất toàn cục của toàn bộ chuỗi giao dịch phân tán (16 bytes biểu diễn bằng chuỗi hex thường). **Bắt buộc khác toàn bộ số 0** (`00000000000000000000000000000000` là bất hợp lệ).
- **`parent_id`** (`16 hex`): Định danh của caller span (8 bytes biểu diễn bằng chuỗi hex thường). **Bắt buộc khác toàn bộ số 0** (`0000000000000000` là bất hợp lệ).
- **`trace_flags`** (`2 hex`): Cờ điều khiển tracing dưới dạng 8-bit hex. Bit thấp nhất (`flags & 0x01 != 0`) xác định cờ ghi nhận (`Sampled = true`).

#### Thuật toán Parsing & Fallback an toàn (W3C Compliant):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub version: String,
    pub trace_id: String,
    pub parent_id: String,
    pub trace_flags: String,
    pub is_sampled: bool,
    pub tracestate: Option<String>,
}

pub fn parse_traceparent(raw: &str) -> Option<TraceContext> {
    let trimmed = raw.trim();
    let mut parts = trimmed.split('-');

    let version = parts.next()?;
    let trace_id = parts.next()?;
    let parent_id = parts.next()?;
    let trace_flags = parts.next()?;

    // 1. Kiểm tra Version
    if version.len() != 2 || !is_lowercase_hex(version) || version == "ff" {
        return None;
    }
    // Với version 00, cấm tuyệt đối các trường bổ sung phía sau
    if version == "00" && parts.next().is_some() {
        return None;
    }

    // 2. Kiểm tra trace_id: 32 hex thường và không được là toàn bộ số 0
    if trace_id.len() != 32 || !is_lowercase_hex(trace_id) || trace_id.bytes().all(|b| b == b'0') {
        return None;
    }

    // 3. Kiểm tra parent_id: 16 hex thường và không được là toàn bộ số 0
    if parent_id.len() != 16 || !is_lowercase_hex(parent_id) || parent_id.bytes().all(|b| b == b'0') {
        return None;
    }

    // 4. Kiểm tra trace_flags: 2 hex thường, kiểm tra cờ lấy mẫu (bit 0x01)
    if trace_flags.len() != 2 || !is_lowercase_hex(trace_flags) {
        return None;
    }
    let flags_byte = u8::from_str_radix(trace_flags, 16).ok()?;
    let is_sampled = (flags_byte & 0x01) == 0x01;

    Some(TraceContext {
        version: version.to_string(),
        trace_id: trace_id.to_string(),
        parent_id: parent_id.to_string(),
        trace_flags: trace_flags.to_string(),
        is_sampled,
        tracestate: None,
    })
}

#[inline]
fn is_lowercase_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
```

---

### 4.2. Cấu trúc định danh `request_id` (UUIDv7 Time-Ordered)

Thay vì dùng UUIDv4 hoàn toàn ngẫu nhiên, OmniKubernetes tiêu chuẩn hóa sử dụng **UUIDv7**:

```text
0192d4b8-f2e1-7e83-a4c3-b8162e24cf81
│──────────────│ │───────────────│
  Unix Epoch MS     Randomness
   (48 bits)         (74 bits)
```

- **Khả năng sắp xếp tự nhiên theo thời gian:** Giảm thiểu độ phân mảnh chỉ mục B-Tree/LSM-Tree trên các hệ thống cơ sở dữ liệu và công cụ tìm kiếm log (Loki, Elasticsearch).
- **Header chuẩn hóa:** Tiếp nhận từ Gateway qua `X-Request-Id` (nếu Envoy đã sinh), hoặc Microservice đầu tiên sẽ tự sinh qua `uuid::Uuid::now_v7()` nếu chưa có.

---

### 4.3. Đối tượng dữ liệu `RequestContext`

Hạt nhân dữ liệu được đính kèm vào mỗi request trong tiến trình với mô hình Zero-Cost static borrowing (`Cow<'static, str>`):

```rust
use std::borrow::Cow;
use chrono::{DateTime, Utc};
use omni_common::response::ResponseMeta;

/// Ngữ cảnh toàn diện của một request đang được xử lý trong tiến trình microservice
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestContext {
    /// Định danh request duy nhất phục vụ gom log (100% requests có giá trị này)
    pub request_id: Cow<'static, str>,
    /// W3C Distributed Trace ID (32 hex characters)
    pub trace_id: Cow<'static, str>,
    /// Span ID của Span hiện tại xử lý request
    pub span_id: Cow<'static, str>,
    /// Parent Span ID nếu request được gọi từ upstream service
    pub parent_span_id: Option<Cow<'static, str>>,
    /// W3C tracestate header nếu có (vendor-specific metadata)
    pub tracestate: Option<Cow<'static, str>>,
    /// Cờ chỉ định request có được lấy mẫu Trace phân tán hay không
    pub is_sampled: bool,
    /// Thời điểm request bắt đầu đi vào middleware của Service
    pub timestamp: DateTime<Utc>,
    /// Mã BCP 47 của ngôn ngữ yêu cầu (tích hợp trực tiếp từ 5-stage waterfall của base-response)
    pub locale: Cow<'static, str>,
    /// Request URI Path (dùng cho instance của RFC 9457)
    pub instance: Cow<'static, str>,
    /// Địa chỉ IP thực tế của client (được trích xuất an toàn từ trusted proxy)
    pub client_ip: Option<Cow<'static, str>>,
    /// User-Agent của Client
    pub user_agent: Option<Cow<'static, str>>,
    /// Định danh người dùng (được điền sau khi qua tầng Authenticate Token)
    pub user_id: Option<Cow<'static, str>>,
    /// Định danh tổ chức/Tenant nếu là hệ thống Multi-tenant
    pub tenant_id: Option<Cow<'static, str>>,
}

impl RequestContext {
    /// Chuyển đổi nhanh RequestContext sang ResponseMeta để trả về trong response body
    pub fn to_meta(&self) -> ResponseMeta {
        ResponseMeta {
            request_id: self.request_id.clone(),
            trace_id: self.trace_id.clone(),
            timestamp: Utc::now(),
            locale: self.locale.clone(),
        }
    }
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
  "target": "omni_common::access_log",
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

### Quy chuẩn môi trường phát triển (Local Development vs Production):

- **Local Development (`APP_ENV=local` hoặc `APP_LOG_FORMAT=pretty`):**
  - Xuất ra terminal dưới định dạng ANSI màu sắc, dễ đọc cho lập trình viên:

    ```text
    2026-10-29T08:30:00.125Z [INFO] [req:0192d4b8... | trace:4bf92f35...] HTTP POST /api/v1/orders -> 201 (14.85ms)
    ```

- **Staging / Production (`APP_ENV=production` hoặc `APP_LOG_FORMAT=json`):**
  - Bắt buộc xuất định dạng JSON tối giản hóa, không màu sắc, cấu trúc một dòng phẳng, tối ưu hóa parsing tốc độ cao cho Vector / Promtail.

---

## 6. Rust Implementation Architecture Specification

### 6.1. Kiến trúc Crate `libs/logging` & Khởi tạo Toàn cục

Thư viện `omni-logging` chịu trách nhiệm khởi tạo logging subscriber và đồng thời thiết lập bridge xuất APM Traces sang OpenTelemetry Collector qua OTLP gRPC.

```text
libs/logging/
├── Cargo.toml
└── src/
    ├── lib.rs              <-- Entrypoint init_logging() & LoggingGuard (với Graceful Shutdown)
    ├── config.rs           <-- LoggingConfig (Level, Format, Service Info, OTLP Endpoint)
    ├── formatter/
    │   ├── mod.rs
    │   ├── json.rs         <-- Zero-allocation JSON formatter tuân thủ OTel/ECS
    │   └── pretty.rs       <-- Terminal color formatter cho Local Dev
    ├── sanitizer.rs        <-- Logic Redaction & Masking PII/Tokens
    └── otel.rs             <-- OTLP Tracing Pipeline (gRPC Exporter tới Tempo)
```

#### Code mẫu khởi tạo Logger và OpenTelemetry Pipeline (`libs/logging/src/lib.rs`):

```rust
use std::io;
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace as sdktrace, Resource};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer, Registry,
};

/// Guard bảo toàn quá trình Flush cả log đệm và OTel trace spans khi tiến trình kết thúc
pub struct LoggingGuard {
    _worker_guard: WorkerGuard,
}

impl Drop for LoggingGuard {
    fn drop(&mut self) {
        // Đảm bảo xả toàn bộ Tracing Spans đang gom đệm trước khi thoát hẳn
        global::shutdown_tracer_provider();
    }
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub service_name: String,
    pub environment: String,
    pub log_level: String,
    pub log_format: LogFormat,
    pub otlp_endpoint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Pretty,
}

/// Khởi tạo hệ thống Tracing Subscriber & OpenTelemetry chuẩn Enterprise
pub fn init_logging(config: &LoggingConfig) -> Result<LoggingGuard, Box<dyn std::error::Error + Send + Sync>> {
    // 1. Cấu hình W3C TraceContext Propagator toàn cục
    global::set_text_map_propagator(TraceContextPropagator::new());

    // 2. Cấu hình Non-blocking Appender cho stdout
    let (non_blocking, guard) = tracing_appender::non_blocking(io::stdout());

    // 3. Thiết lập EnvFilter lọc log linh hoạt
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    // 4. Khởi tạo Registry nền tảng
    let registry = Registry::default().with(filter);

    // 5. Cấu hình Formatter Layer (JSON vs Pretty)
    let fmt_layer = match config.log_format {
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .boxed(),
        LogFormat::Pretty => tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .pretty()
            .with_thread_ids(false)
            .boxed(),
    };

    // 6. Tích hợp OpenTelemetry Tracing Layer nếu có cấu hình endpoint
    if let Some(endpoint) = &config.otlp_endpoint {
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint),
            )
            .with_trace_config(
                sdktrace::config().with_resource(Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", config.service_name.clone()),
                    opentelemetry::KeyValue::new("service.environment", config.environment.clone()),
                ])),
            )
            .install_batch(opentelemetry_sdk::runtime::Tokio)?;

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        registry.with(fmt_layer).with(otel_layer).init();
    } else {
        registry.with(fmt_layer).init();
    }

    Ok(LoggingGuard { _worker_guard: guard })
}
```

---

### 6.2. Middleware Ngữ cảnh & Observability Ingress (`libs/common/src/middleware/observability.rs`)

Middleware này được gắn tại tầng cao nhất của Axum Router để đảm bảo bọc trọn chu kỳ sống của request, gắn kết chặt chẽ với OpenTelemetry Span:

```rust
use std::{borrow::Cow, time::Instant};
use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use opentelemetry::trace::TraceContextExt;
use tracing::{info_span, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

use crate::{
    context::RequestContext,
    i18n::extractor::RequestLocale,
    middleware::traceparent::{parse_traceparent, TRACEPARENT_HEADER},
    response::X_REQUEST_ID_HEADER,
};

/// Middleware chính khởi tạo RequestContext, Span Tracing, đo lường và ghi Access Log
pub async fn observability_middleware(mut req: Request<Body>, next: Next) -> Response {
    let start_instant = Instant::now();
    let path = req.uri().path().to_string();

    // 1. Kiểm tra lọc nhiễu Kubernetes Probes & Metrics (Không tạo root trace span nặng)
    let is_health_check = path == "/healthz" || path == "/livez" || path == "/readyz" || path == "/metrics";
    if is_health_check {
        return next.run(req).await;
    }

    // 2. Trích xuất hoặc sinh mới X-Request-Id (UUIDv7)
    let request_id = extract_or_generate_request_id(req.headers());

    // 3. Trích xuất hoặc kế thừa W3C TraceContext & tracestate
    let (trace_id, parent_span_id, is_sampled, tracestate) = extract_or_generate_trace_context(req.headers());
    let current_span_id = format!("{:016x}", rand::random::<u64>());

    // 4. Trích xuất Client IP an toàn (Chống IP Spoofing qua Trusted Proxies)
    let client_ip = extract_trusted_client_ip(req.headers());
    let user_agent = req.headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 5. Xác định Locale yêu cầu theo 5-stage waterfall của base-response architecture
    let request_locale = RequestLocale::resolve(req.uri(), req.headers(), req.extensions());
    let locale_str = request_locale.0.clone();

    // 6. Đóng gói RequestContext và đưa vào Extensions
    let context = RequestContext {
        request_id: Cow::Owned(request_id.clone()),
        trace_id: Cow::Owned(trace_id.clone()),
        span_id: Cow::Owned(current_span_id.clone()),
        parent_span_id: parent_span_id.map(Cow::Owned),
        tracestate: tracestate.clone().map(Cow::Owned),
        is_sampled,
        timestamp: Utc::now(),
        locale: locale_str.clone(),
        instance: Cow::Owned(path.clone()),
        client_ip: client_ip.map(Cow::Owned),
        user_agent: user_agent.map(Cow::Owned),
        user_id: None,
        tenant_id: None,
    };
    req.extensions_mut().insert(context.clone());
    req.extensions_mut().insert(request_locale.clone());

    // 7. Tạo Root Tracing Span tuân thủ OTel Semantic Conventions v1.28+
    let method = req.method().to_string();
    let span = info_span!(
        "http_request",
        "http.request.method" = %method,
        "http.route" = %path,
        "url.path" = %path,
        "request_id" = %request_id,
        "trace_id" = %trace_id,
        "span_id" = %current_span_id,
        "http.response.status_code" = tracing::field::Empty,
        "user_id" = tracing::field::Empty,
        "tenant_id" = tracing::field::Empty,
    );

    // Kế thừa Parent Context từ OTel nếu có header W3C hợp lệ
    let parent_cx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(req.headers()))
    });
    span.set_parent(parent_cx);

    // 8. Thực thi downstream handler bên trong Span Context (Instrumented Future)
    // TUYỆT ĐỐI KHÔNG dùng span.enter() qua điểm .await vì sẽ gây ô nhiễm Span trên Tokio worker threads
    let mut response = next.run(req).instrument(span.clone()).await;

    // 9. Cập nhật kết quả vào Span & tính toán thời gian thực thi
    let status_code = response.status().as_u16();
    span.record("http.response.status_code", status_code);

    let elapsed = start_instant.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    // Đánh dấu lỗi trên Span nếu mã phản hồi >= 500
    if response.status().is_server_error() {
        span.record("error.type", "INTERNAL_SERVER_ERROR");
    }

    // 10. Gắn Header phản hồi chuẩn hóa (Single Source of Truth)
    inject_response_headers(&mut response, &request_id, &trace_id, &current_span_id, is_sampled, duration_ms, &locale_str, tracestate.as_deref());

    // 11. Ghi Access Log có cấu trúc gắn kết với Span
    tracing::info!(
        parent: &span,
        target: "omni_common::access_log",
        method = %method,
        path = %path,
        status = status_code,
        duration_ms = duration_ms,
        request_id = %request_id,
        trace_id = %trace_id,
        "Handled HTTP request"
    );

    response
}

/// Bóc tách W3C traceparent hợp lệ hoặc tự sinh Trace ID chuẩn mới
fn extract_or_generate_trace_context(headers: &HeaderMap) -> (String, Option<String>, bool, Option<String>) {
    let tracestate = headers
        .get("tracestate")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(raw) = headers.get(TRACEPARENT_HEADER).and_then(|v| v.to_str().ok()) {
        if let Some(ctx) = parse_traceparent(raw) {
            return (ctx.trace_id, Some(ctx.parent_id), ctx.is_sampled, tracestate);
        }
    }
    // Fallback: Sinh mới Trace ID 32 ký tự hex (16 bytes)
    let new_trace_id = Uuid::now_v7().simple().to_string();
    (new_trace_id, None, true, tracestate)
}

fn extract_or_generate_request_id(headers: &HeaderMap) -> String {
    if let Some(val) = headers.get(X_REQUEST_ID_HEADER).and_then(|v| v.to_str().ok()) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    Uuid::now_v7().to_string()
}

/// Trích xuất IP Client an toàn dựa trên mô hình tin cậy Envoy Gateway (được parse xác thực kiểu IpAddr)
fn extract_trusted_client_ip(headers: &HeaderMap) -> Option<String> {
    use std::net::IpAddr;

    // 1. Header do Envoy Gateway tiêm vào sau khi xác thực kết nối ingress
    if let Some(envoy_ip) = headers.get("x-envoy-external-address").and_then(|v| v.to_str().ok()) {
        let trimmed = envoy_ip.trim();
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            return Some(ip.to_string());
        }
    }
    // 2. X-Real-IP từ Kubernetes Ingress Controller tin cậy
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = real_ip.trim();
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            return Some(ip.to_string());
        }
    }
    // 3. Fallback X-Forwarded-For: Lấy IP ngoài cùng phía Client (chỉ tin cậy nếu có Gateway lọc)
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first_ip) = xff.split(',').next() {
            let trimmed = first_ip.trim();
            if let Ok(ip) = trimmed.parse::<IpAddr>() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn inject_response_headers(
    res: &mut Response,
    request_id: &str,
    trace_id: &str,
    span_id: &str,
    is_sampled: bool,
    duration_ms: f64,
    locale: &str,
    tracestate: Option<&str>,
) {
    let headers = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static(X_REQUEST_ID_HEADER), v);
    }
    let flags = if is_sampled { "01" } else { "00" };
    let traceparent_resp = format!("00-{}-{}-{}", trace_id, span_id, flags);
    if let Ok(v) = HeaderValue::from_str(&traceparent_resp) {
        headers.insert(HeaderName::from_static(TRACEPARENT_HEADER), v);
    }
    if let Some(state) = tracestate {
        if let Ok(v) = HeaderValue::from_str(state) {
            headers.insert(HeaderName::from_static("tracestate"), v);
        }
    }
    let timing = format!("total;dur={:.2}", duration_ms);
    if let Ok(v) = HeaderValue::from_str(&timing) {
        headers.insert(HeaderName::from_static("server-timing"), v);
    }
    let x_time = format!("{:.2}ms", duration_ms);
    if let Ok(v) = HeaderValue::from_str(&x_time) {
        headers.insert(HeaderName::from_static("x-response-time"), v);
    }
    if let Ok(v) = HeaderValue::from_str(locale) {
        headers.insert(header::CONTENT_LANGUAGE, v);
    }
}

/// Helper trích xuất Header phục vụ OpenTelemetry TextMapPropagator
struct HeaderExtractor<'a>(&'a HeaderMap);

impl<'a> opentelemetry::propagation::Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}
```

---

### 6.3. Axum FromRequestParts Extractor (`RequestContext` & `ResponseMeta`)

Để đảm bảo tính chịu lỗi cao nhất (High Resilience), Extractor được thiết kế theo mô hình **Infallible** với cơ chế tự phục hồi (Self-Healing Fallback). Nếu handler nghiệp vụ gọi `RequestContext` hoặc `ResponseMeta` mà quên khai báo middleware, hệ thống vẫn an toàn tạo context tối giản mà không bao giờ ném lỗi Runtime HTTP 500:

```rust
use std::convert::Infallible;
use std::future::Future;
use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use crate::{
    context::RequestContext,
    response::ResponseMeta,
};

impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = if let Some(existing) = parts.extensions.get::<RequestContext>() {
            existing.clone()
        } else {
            // Tự phục hồi an toàn từ Parts
            RequestContext::from_parts(parts)
        };
        std::future::ready(Ok(ctx))
    }
}

impl<S> FromRequestParts<S> for ResponseMeta
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = if let Some(existing) = parts.extensions.get::<RequestContext>() {
            existing.to_meta()
        } else {
            RequestContext::from_parts(parts).to_meta()
        };
        std::future::ready(Ok(ctx))
    }
}
```

#### Áp dụng thực tế trong Handler nghiệp vụ:

Nhờ `observability_middleware` tự động chèn các headers (`x-request-id`, `traceparent`, `server-timing`, `content-language`), handler chỉ cần trả về `(StatusCode, Json<ApiResponse<T>>)` cực kỳ ngắn gọn và trực quan:

```rust
use axum::{http::StatusCode, Json};
use omni_common::context::RequestContext;
use omni_common::response::ApiResponse;

pub async fn get_user_profile(
    ctx: RequestContext,
) -> (StatusCode, Json<ApiResponse<UserProfileDto>>) {
    let user_data = UserProfileDto { /* ... */ };
    ApiResponse::ok(user_data, "Lấy thông tin người dùng thành công", ctx.to_meta())
}
```

---

### 6.4. Lan truyền ngữ cảnh sang Downstream (Egress HTTP & gRPC Propagation)

Khi microservice cần gọi sang một dịch vụ khác (ví dụ: `order-service` gọi `payment-service` qua HTTP hoặc qua gRPC), ngữ cảnh phân tán bắt buộc phải được bảo toàn chính xác.

**Quy tắc bất biến:** `parent_id` truyền sang downstream bắt buộc phải là **Span ID hiện tại của dịch vụ gọi** (`ctx.span_id`), và cờ lấy mẫu (`trace_flags`) phải lấy từ `ctx.is_sampled`. Bảo toàn header `tracestate` nếu có. Tuyệt đối không sinh ngẫu nhiên Span ID mới làm hỏng cây phả hệ phân tán trên APM.

#### A. HTTP Egress Propagation (Reqwest):

```rust
use reqwest::RequestBuilder;
use crate::context::RequestContext;

/// Tiêm W3C traceparent, tracestate, X-Request-Id và X-Locale vào HTTP Outgoing Request
pub fn inject_egress_context(mut builder: RequestBuilder, ctx: &RequestContext) -> RequestBuilder {
    let flags = if ctx.is_sampled { "01" } else { "00" };
    // Sử dụng span_id hiện tại của caller làm parent_id cho downstream
    let traceparent = format!("00-{}-{}-{}", ctx.trace_id, ctx.span_id, flags);

    builder = builder
        .header("x-request-id", ctx.request_id.as_ref())
        .header("traceparent", traceparent)
        .header("x-locale", ctx.locale.as_ref());

    if let Some(state) = &ctx.tracestate {
        builder = builder.header("tracestate", state.as_ref());
    }

    builder
}
```

#### B. gRPC East-West Egress Propagation (Tonic Interceptor / MetadataMap):

Vì kiến trúc OmniKubernetes quy định toàn bộ giao tiếp đồng bộ nội bộ giữa các Microservices sử dụng **gRPC (HTTP/2)**, ta sử dụng helper tiêm metadata trực tiếp vào `tonic::Request<T>`:

```rust
use tonic::{metadata::MetadataValue, Request};
use crate::context::RequestContext;

/// Tiêm W3C traceparent, tracestate, X-Request-Id vào gRPC Outgoing Request Metadata
pub fn inject_grpc_egress_context<T>(req: &mut Request<T>, ctx: &RequestContext) {
    let metadata = req.metadata_mut();
    let flags = if ctx.is_sampled { "01" } else { "00" };
    let traceparent = format!("00-{}-{}-{}", ctx.trace_id, ctx.span_id, flags);

    if let Ok(val) = MetadataValue::try_from(ctx.request_id.as_ref()) {
        metadata.insert("x-request-id", val);
    }
    if let Ok(val) = MetadataValue::try_from(&traceparent) {
        metadata.insert("traceparent", val);
    }
    if let Some(state) = &ctx.tracestate {
        if let Ok(val) = MetadataValue::try_from(state.as_ref()) {
            metadata.insert("tracestate", val);
        }
    }
    if let Ok(val) = MetadataValue::try_from(ctx.locale.as_ref()) {
        metadata.insert("x-locale", val);
    }
}
```

---

### 6.5. Tích hợp Ngữ cảnh Xác thực (Auth Middleware & User/Tenant Enrichment)

Sau khi request vượt qua tầng kiểm tra JWT Token tại `auth_middleware`, ngữ cảnh người dùng (`user_id`, `tenant_id`) cần được bổ sung vào cả `RequestContext` trong Extensions và ghi trực tiếp vào active `tracing::Span`:

```rust
use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use crate::context::RequestContext;

pub async fn auth_middleware(mut req: Request<Body>, next: Next) -> Response {
    // Giả định giải mã JWT thành công
    let auth_user_id = "usr_01HZX87Q1K7";
    let auth_tenant_id = "tenant_enterprise_01";

    // 1. Cập nhật RequestContext trong Extensions
    if let Some(ctx) = req.extensions_mut().get_mut::<RequestContext>() {
        ctx.user_id = Some(auth_user_id.to_string().into());
        ctx.tenant_id = Some(auth_tenant_id.to_string().into());
    }

    // 2. Ghi nhận trực tiếp danh tính vào Tracing Span hiện tại
    // Mọi log nghiệp vụ phát sinh sau thời điểm này trong cùng request sẽ tự động mang user_id
    let current_span = tracing::Span::current();
    current_span.record("user_id", auth_user_id);
    current_span.record("tenant_id", auth_tenant_id);

    next.run(req).await
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
│  - Gán header x-envoy-external-address chống IP Spoofing    │
│  - Ghi Access Log của Envoy kèm cả 2 định danh              │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               │ Forward: Hostname ClusterIP
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  OmniKubernetes Pod (e.g., auth-service)                    │
│                                                             │
│  [Observability Middleware]                                 │
│  ├── 1. Trích xuất X-Request-Id & traceparent (W3C Check)   │
│  ├── 2. Bỏ qua APM root span cho /livez, /readyz            │
│  ├── 3. Tạo Span 'http_request' & liên kết OTel Context     │
│  │                                                          │
│  ▼ [Auth Middleware]                                        │
│  ├── Giải mã JWT -> record("user_id", "usr_123") vào Span   │
│  │                                                          │
│  ▼ [Business Logic & SQL Query]                             │
│  ├── Log: "Xác thực token thành công" (Có trace_id & req_id) │
│  ├── Database Query: span 'db.query SELECT users'           │
│  │                                                          │
│  ▼ [Egress Call: notification-service]                      │
│  ├── Tiêm traceparent (parent_id = current.span_id)         │
│  │                                                          │
│  ▼ [Middleware Egress]                                      │
│  ├── Record http.response.status_code vào Span              │
│  ├── Gắn Header: Server-Timing, X-Request-Id, traceparent   │
│  └── Ghi structured JSON Access Log bên trong Span Scope    │
└──────────────────────────────┬──────────────────────────────┘
                               │
               Stdout NDJSON   │   OTLP gRPC Spans
         ┌─────────────────────┴──────────────────────┐
         ▼                                            ▼
┌──────────────────────────────┐        ┌──────────────────────────────┐
│  Log Collector (Vector/Loki) │        │  OpenTelemetry Collector     │
│  - Parse JSON labels         │        │  - Batch Processor           │
│  - Lưu trữ trên Grafana Loki │        │  - Xuất sang Grafana Tempo   │
└──────────────────────────────┘        └──────────────────────────────┘
```

---

## 8. Security, PII Masking & Production Hardening

### 8.1. Quy chuẩn che giấu dữ liệu nhạy cảm (PII & Secrets Scrubbing)

Tuyệt đối không để rò rỉ các thông tin cá nhân và chứng thực bí mật vào hệ thống Log tập trung.

Crate `libs/logging` cài đặt bộ lọc tự động (**Log Sanitizer**) dựa trên danh sách đen các trường nhạy cảm:

| Tên trường / Header           | Hành vi che giấu (Masking Policy) | Ví dụ giá trị hiển thị       |
| :---------------------------- | :-------------------------------- | :--------------------------- |
| `authorization` / `token`     | Giữ 6 ký tự đầu, ẩn phần còn lại  | `Bearer eyJhbG...[REDACTED]` |
| `password` / `secret` / `pin` | Ẩn hoàn toàn                      | `[REDACTED]`                 |
| `credit_card` / `cvv`         | Chỉ giữ 4 số cuối                 | `****-****-****-1234`        |
| `id_number` (CCCD/CMND)       | Chỉ giữ 3 số đầu và 3 số cuối     | `001***982`                  |
| `email`                       | Mask ký tự tên hòm thư            | `n***n@example.com`          |

### 8.2. Cơ chế Non-blocking Buffer & Chống tràn bộ nhớ (Backpressure)

- Sử dụng `tracing_appender::non_blocking::NonBlockingBuilder` với dung lượng hàng đợi đệm định trước (`buffered_lines_limit`).
- Luồng ghi I/O chạy trên background thread chuyên dụng tách biệt khỏi Tokio Reactor:
  - Khi tải tăng đột biến, hàng đợi đệm cho phép duy trì thông lượng cao mà không chặn luồng xử lý async.
  - Khi buffer đạt giới hạn dung lượng, cấu hình `lossy(true)` áp dụng chiến lược drop các bản ghi log mới để bảo vệ tiến trình, ngăn chặn rò rỉ bộ nhớ (OOM) và giữ cho Tokio workers luôn sẵn sàng phục vụ traffic nghiệp vụ.
  - Ở tầng subscriber, cấu hình `EnvFilter` đảm bảo chỉ các mức log cần thiết (ví dụ: `INFO` trở lên trong Production) mới được định dạng và gửi vào queue, tối thiểu hóa áp lực lên bộ đệm.

### 8.3. Graceful Shutdown & Buffer Flush

Khi Kubernetes gửi tín hiệu `SIGTERM` để hủy Pod, ứng dụng phải xả toàn bộ buffer log còn tồn dư ra stdout và flush toàn bộ trace spans đang gom đệm sang OpenTelemetry Collector trước khi tiến trình tắt hoàn toàn.

Đối tượng `LoggingGuard` đảm bảo thực hiện thao tác flush trong hàm `Drop`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let logging_config = LoggingConfig::from_env();
    let _guard = omni_logging::init_logging(&logging_config)?;

    // Khi nhận tín hiệu SIGTERM, luồng ứng dụng kết thúc:
    // _guard bị drop -> tự động gọi WorkerGuard flush stdout và opentelemetry::global::shutdown_tracer_provider()
    Ok(())
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
│   │       ├── lib.rs                     <-- init_logging, LoggingGuard (Drop -> shutdown OTel)
│   │       ├── config.rs                  <-- LoggingConfig, LogFormat
│   │       ├── sanitizer.rs               <-- PII / Sensitive masking
│   │       └── otel.rs                    <-- OpenTelemetry OTLP pipeline config
│   │
│   ├── common/                            <-- Crate omni-common
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── response.rs                <-- ApiResponse, ResponseMeta
│   │       ├── i18n/
│   │       │   └── extractor.rs           <-- RequestLocale 5-stage waterfall
│   │       └── middleware/
│   │           ├── mod.rs
│   │           ├── traceparent.rs         <-- W3C traceparent parser & UUIDv7 generator
│   │           ├── context.rs             <-- RequestContext struct & Infallible Extractor
│   │           ├── observability.rs       <-- Ingress Context, Span & Access Log Middleware
│   │           └── timing.rs              <-- Server-Timing measurement
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

1. **Unit Test Phân tách Trace Context (W3C Trace Context Conformance):**
   - Parse chính xác `traceparent` hợp lệ của W3C Level 1.
   - Từ chối các header có `trace_id` hoặc `parent_id` toàn số 0, hoặc phiên bản `ff`.
   - Kiểm tra bitmask cờ sampling chính xác (`(flags & 0x01) == 1`).
   - Xử lý mượt mà và tự sinh fallback an toàn khi header `traceparent` bị lỗi hoặc thiếu.
2. **Unit Test Định danh UUIDv7:**
   - `request_id` sinh mới phải là UUIDv7 chuẩn RFC 9562 và có tính chất tăng dần theo thời gian.
3. **Integration Test Axum Middleware & Infallible Extractor:**
   - Mọi response trả về từ Router đều sở hữu các headers: `x-request-id`, `traceparent`, `server-timing`, `content-language`.
   - `RequestContext` và `ResponseMeta` trích xuất thành công và tự phục hồi an toàn trong mọi handlers mà không bao giờ gây hoảng loạn (panic) hay lỗi HTTP 500.
4. **Distributed Trace Tree Continuity Test:**
   - Khi Egress Client gọi downstream, `traceparent` truyền đi phải mang `trace_id` của request hiện tại và `parent_id` là Span ID hiện tại, bảo toàn liên kết phân tán trên Grafana Tempo.
5. **Noise Filter Verification:**
   - Các requests gửi tới `/healthz`, `/livez`, `/readyz`, `/metrics` không phát sinh APM trace root spans và không phát sinh Access Log trên stdout.
6. **PII Masking & Graceful Shutdown Verification:**
   - Các trường mang nhãn nhạy cảm (`password`, `authorization`) tuyệt đối không xuất hiện nguyên bản dạng plain text trên log output.
   - Khi tiến trình nhận `SIGTERM`, toàn bộ trace spans đang gom đệm được flush đầy đủ tới OTLP Collector trước khi process dừng.
