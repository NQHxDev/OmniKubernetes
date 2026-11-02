# OmniKubernetes Engineering Roadmap: Platform Foundation & Shared Libraries Implementation

- **Document Version:** `1.0.0`
- **Status:** Active / Engineering Standard
- **Author:** System Architecture Team
- **Target Platform:** Rust (2024 Edition, Rust 1.85+), Tokio Runtime (1.43+), Axum Framework (0.8+), Kubernetes & CNCF Ecosystem
- **Target Scope:** Tầng hạ tầng dùng chung (`libs/*`, toolchain, middleware, testing harness) trước khi triển khai các Domain Microservices (`services/*`)

---

## 1. Executive Summary & Triết lý Thiết kế (Design Philosophy)

Trong quá trình phát triển hệ sinh thái microservices quy mô doanh nghiệp (**Enterprise Microservices on Kubernetes**) tại **OmniKubernetes**, sự ổn định của từng service phụ thuộc trực tiếp vào tính đồng nhất và độ hoàn thiện của tầng nền tảng (**Platform Foundation**).

Thay vì để các nhóm phát triển nhảy ngay vào viết mã nghiệp vụ trong `services/*` (dẫn tới phân mảnh cấu hình, xử lý lỗi chắp vá, logging lệch chuẩn và rò rỉ kết nối hạ tầng), dự án thực thi nghiêm ngặt triết lý **"Foundation-First Architecture"**:

> **Nguyên tắc "Golden Path":**  
> Mọi thành phần nền tảng dùng chung (`libs/*`) phải được thiết kế thành các khối xây dựng cắm-rút (**Plug-and-Play Building Blocks**). Khi hoàn tất tầng này, việc khởi tạo một microservice mới (như `order`, `payment`, `user`) chỉ mất dưới 30 phút để thiết lập, và mặc định sở hữu 100% năng lực quan sát (Observability), bảo mật (Auth/PII Masking), khả năng chịu lỗi (Resilience), kết nối dữ liệu an toàn và tài liệu hóa chuẩn quốc tế.

---

## 2. Hiện trạng dự án & Các mốc đã hoàn thành (Current Status)

Hệ thống đã hoàn tất nghiệm thu hai trụ cột nền tảng đầu tiên tại `libs/common`, `libs/errors` và `libs/logging`:

| Mốc nền tảng                                  | Trạng thái  | Crate đảm nhiệm                                                        | Nội dung đã bàn giao                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| :-------------------------------------------- | :---------: | :--------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Base Response & i18n Protocol**             | **ĐÃ XONG** | `omni-common`<br/>`omni-errors`                                        | - Data contract `ApiResponse<T>`, `ResponseMeta` zero-allocation.<br/>- Phân trang kép `OffsetPaginationMeta` & `CursorPaginationMeta`.<br/>- Đa ngôn ngữ Mozilla Fluent RCU lock-free (vi-VN, en-US, ja-JP).<br/>- Chuẩn lỗi RFC 9457 Problem Details (`application/problem+json`).<br/>- Server-side translation kết hợp Client-side localized parameters.<br/>- Ánh xạ JSON Pointer (RFC 6901) hai chiều.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| **Distributed Context & Structured Logging**  | **ĐÃ XONG** | `omni-logging`<br/>`omni-common`                                       | - Định danh kép `request_id` (UUIDv7) và W3C `traceparent` (32-hex `trace_id`).<br/>- Structured Logging định dạng JSON một dòng (ECS/OTel standard).<br/>- Ghi log non-blocking qua `tracing-appender` tách biệt khỏi worker threads.<br/>- Bộ lọc PII Sanitizer bảo vệ dữ liệu cá nhân theo Nghị định 13/2023/NĐ-CP & GDPR.<br/>- Middleware tự động đo lường thời gian `Server-Timing` & `X-Response-Time`.<br/>- Cầu nối OpenTelemetry Tracing gRPC/OTLP xuất sang Grafana Tempo.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| **Type-Safe Configuration & Env Binding**     | **ĐÃ XONG** | `omni-common`                                                          | - Phân cấp nạp cấu hình 5 tầng: Safe Defaults -> `config/base.yaml` -> `config/{env}/default.yaml` -> System Environment `OMNI__*` / K8s Downward API -> Programmatic Overrides.<br/>- Fail-Fast 2 pha kiểm tra (field-level qua `validator` + bất biến liên trường & chốt chặn Production `CFG_*`).<br/>- Bảo vệ bí mật tuyệt đối Zero-Leak `SecretString` (che giấu `[REDACTED]`, tự động zeroize bộ nhớ heap, URL kết nối an toàn).<br/>- Cấu hình mở rộng kiểu tĩnh `AppConfig<C>` và cầu nối trực tiếp sang `omni_logging::LoggingConfig`.                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| **Metrics Telemetry & Kubernetes Probes**     | **ĐÃ XONG** | `omni-metrics`<br/>`omni-errors`<br/>`omni-common`                     | - Bộ chỉ số RED Middleware (Rate, Errors, Duration) & In-Flight requests cho Axum.<br/>- Bảo vệ Cardinality Explosion qua Zero-Regex Byte Scanner `sanitize_uri_path` & `normalize_matched_path`.<br/>- Phân loại lỗi Zero-Copy không đọc lại body qua `ErrorCodeExtension` tích hợp với `omni-errors`.<br/>- Bộ 3 đầu dò Kubernetes Tri-Probe (`/livez`, `/readyz`, `/startupz`, `/healthz`) kết hợp Graceful Shutdown Drain Gate.<br/>- Thu thập chỉ số Tokio Runtime Saturation, Watchdog Heartbeat và Linux `/proc/self` Telemetry không dùng mã `unsafe`.                                                                                                                                                                                                                                                                                                                                                                                                                       |
| **Relational Persistence Foundation**         | **ĐÃ XONG** | `omni-database`<br/>`omni-metrics`<br/>`omni-errors`<br/>`omni-common` | - Enterprise PgPool Factory với bounded backoff retry, keepalive ping 3 tầng và graceful shutdown.<br/>- Pool Sentinel chạy nền nuôi chỉ số Prometheus và cung cấp trạng thái sẵn sàng cho `/readyz` không gây thundering herd.<br/>- Zero-Round-Trip Unit of Work với `build_begin_statement` ghép `trace_id`, `actor_id`, `tenant_id`, `statement_timeout` vào câu lệnh `BEGIN`.<br/>- Phân loại lỗi SQLSTATE sang `DbError` và RFC 9457 `AppError` không rò rỉ cấu trúc cơ sở dữ liệu nội bộ.<br/>- Đo lường OTel Semantic Conventions `db.system.name = "postgresql"` kết hợp bộ chỉ số `db_client_*` và slow query logging.<br/>- Entity Auditing Model (`AuditColumns`, trigger `omni_set_audit_timestamps`), `Id<T>` UUIDv7 kiểu tĩnh an toàn, và Optimistic Locking.<br/>- Keyset pagination bridge (`KeysetCursor`, `paginate_keyset`) sang `CursorPaginationMeta`.<br/>- Migration Runner hỗ trợ 3 chế độ `Disabled`, `Apply`, `Verify` kiểm tra schema trước khi phục vụ. |
| **Distributed Caching & Concurrency Control** | **ĐÃ XONG** | `omni-cache`<br/>`omni-metrics`<br/>`omni-errors`<br/>`omni-common`    | - Redis/Valkey async multiplexed backend với auto-reconnect.<br/>- 6-layer stampede protection (Single-Flight, TTL Jitter, XFetch, SWR/SIE, Distributed Lease, Negative Caching).<br/>- Fail-closed distributed locking với fencing tokens & watchdog.<br/>- HMAC-SHA256 PII masking, cluster-ready keyspace với hash tags.<br/>- Binary protocol với zstd compression & decompression bomb protection.<br/>- TypedCache abstraction, fail-open reads với loader fallback.<br/>- Health indicator, metrics recorder và slow command logging.                                                                                                                                                                                                                                                                                                                                                                                                                                         |

---

## 3. Sơ đồ ma trận phụ thuộc lộ trình (Roadmap Dependency Graph)

Thứ tự triển khai các giai đoạn tiếp theo tuân thủ chuỗi phụ thuộc kỹ thuật tự nhiên: **Cấu hình -> Giám sát -> Dữ liệu & Cache -> Bảo mật -> Giao tiếp bất đồng bộ -> Khả năng phục hồi & Khung mẫu service**.

```mermaid
flowchart TD
    subgraph Done["ĐÃ HOÀN THÀNH"]
        M01["1. Base Response & i18n Protocol<br/>(omni-common, omni-errors)"]
        M02["2. Distributed Tracing & Structured Logging<br/>(omni-logging, W3C Context, PII Sanitizer)"]
        M03["3. Type-Safe Configuration & Env Binding<br/>(omni-common, Fail-Fast, Zero-Leak Secrets)"]
        M04["4. Metrics Telemetry & Kubernetes Probes<br/>(omni-metrics, Prometheus RED, Healthz/Readyz)"]
        M05["5. Relational Persistence Foundation<br/>(omni-database, SQLx PgPool, Migrations, Auditing)"]
        M06["6. Distributed Caching & Concurrency Control<br/>(omni-cache, 6-Layer Stampede, Redlock, PII Masking)"]
    

    subgraph Phase3["GIAI ĐOẠN 3: BẢO MẬT & TRUYỀN THÔNG BẤT ĐỒNG BỘ"]
        P5["Bước 5: Identity & Security Context<br/>(omni-auth, JWT Claims, Argon2, RBAC Extractors)"]
        P6["Bước 6: Event-Driven Messaging & Outbox Pattern<br/>(omni-messaging, CloudEvents, Outbox Relay, DLQ)"]
    end

    subgraph Phase4["GIAI ĐOẠN 4: KHẢ NĂNG PHỤC HỒI & SERVICE STARTER KIT"]
        P7["Bước 7: Resilience & Traffic Control<br/>(Rate Limiting, Circuit Breaker, Graceful Shutdown)"]
        P8["Bước 8: API Documentation Aggregation<br/>(utoipa OpenAPI Shared Schemas & Gateway Specs)"]
        P9["Bước 9: Standard Service Scaffold & Dev Harness<br/>(AppBuilder, Testcontainers, Local Compose Stack)"]
    end

    Done --> Phase3
    Phase3 --> Phase4
    Phase4 ==> Target["BẮT ĐẦU TRIỂN KHAI DOMAIN MICROSERVICES<br/>(gateway, auth, user, product, order, payment, notification)"]
```

---

## 4. Đặc tả chi tiết từng giai đoạn thực thi (Detailed Technical Roadmap)

### Bước 1: Quản lý cấu hình kiểu tĩnh & Ràng buộc môi trường (Type-Safe Configuration) - [ĐÃ HOÀN THÀNH]

- **Crate thực hiện:** `libs/common` (module `config`) kết hợp đặc tả [`configuration-management-architecture.md`](./docs/architecture/configuration-management-architecture.md).
- **Vấn đề giải quyết:** Mọi thành phần tiếp theo (Database connection pool, Redis client, Prometheus exporter, Auth keys) đều cần tham số khởi chạy. Không thể cho phép từng service tự `std::env::var` rải rác.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Cấu hình đa tầng phân cấp (Hierarchical Layering):**
     - Nạp theo thứ tự ưu tiên tăng dần: `config/base.yaml` -> `config/{env}.yaml` (`dev`, `staging`, `prod`) -> Biến môi trường hệ thống tiền tố `OMNI_*` -> Tham số dòng lệnh (CLI flags).
     - Sử dụng crate `config` kết hợp `serde` để deserialize trực tiếp vào các Rust struct kiểu tĩnh.
  2. **Fail-Fast Startup Validation:**
     - Sử dụng crate `validator` để kiểm tra ràng buộc ngay lúc nạp (port trong dải cho phép, chuỗi kết nối URL hợp lệ, thời hạn TTL lớn hơn 0).
     - Nếu thiếu biến môi trường bắt buộc hoặc sai định dạng, tiến trình dừng ngay lập tức kèm thông báo lỗi tường minh, ngăn chặn lỗi crash ngầm giữa chừng khi phục vụ traffic.
  3. **Bảo mật biến môi trường nhạy cảm (Zero-Leak Secrets):**
     - Bọc toàn bộ các trường nhạy cảm (Database Password, Redis Auth, JWT Secret Key, Webhook Token) bằng kiểu `SecretString` từ crate `secrecy`.
     - Vô hiệu hóa tính năng tự động in log hoặc serialize ra JSON khi debug cấu hình, ngăn chặn rò rỉ secret vào hệ thống lưu trữ log.

---

### Bước 2: Hoàn thiện Trụ cột Quan sát thứ 3: Metrics & Kubernetes Probes - [ĐÃ HOÀN THÀNH]

- **Crate thực hiện:** `libs/metrics` (`omni-metrics`) kết hợp đặc tả [`metrics-probes-telemetry-architecture.md`](./docs/architecture/metrics-probes-telemetry-architecture.md).
- **Vấn đề giải quyết:** Hệ thống đã có **Tracing** và **Logging**, cần hoàn thiện mảnh ghép **Metrics** để phục vụ cảnh báo tự động (Alerting), giám sát tải thời gian thực và điều phối Pod tự động trên Kubernetes (HPA - Horizontal Pod Autoscaler).
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Prometheus Metric Exposition:**
     - Tích hợp `metrics` facade và `metrics-exporter-prometheus`.
     - Phục vụ endpoint `/metrics` định dạng văn bản chuẩn Prometheus / OpenMetrics.
  2. **Bộ chỉ số RED Middleware cho Axum:**
     - **Rate (Tần suất):** Counter `http_requests_total` gắn nhãn `method`, `route`, `status_code`.
     - **Errors (Lỗi):** Counter `http_request_errors_total` phân loại theo mã lỗi nghiệp vụ từ `omni-errors`.
     - **Duration (Thời lượng):** Histogram `http_request_duration_seconds` với các bucket phân bố chuẩn từ 5ms đến 10s.
     - **Low Cardinality Sanitization:** Tự động chuẩn hóa URI path mẫu (ví dụ: chuyển `/api/v1/orders/018f4a2b-8a5e-7a1b` thành `/api/v1/orders/:id`) để ngăn chặn nổ bùng bộ nhớ lưu trữ Prometheus (Cardinality Explosion).
  3. **Bộ Endpoint Kubernetes Probes chuẩn:**
     - Endpoint `/livez` (Liveness Probe): Đảm bảo tiến trình Rust không rơi vào deadlock hoặc treo Tokio runtime.
     - Endpoint `/readyz` (Readiness Probe): Kiểm tra khả năng kết nối thực tế tới các phụ thuộc hạ tầng cốt lõi (Database pool còn kết nối, Redis ping phản hồi). Nếu mất kết nối, Kubernetes ngừng định tuyến traffic tới Pod.
     - Endpoint `/startupz` (Startup Probe): Dành cho các service cần thời gian khởi động dài (chạy migrations lớn, nạp trước in-memory cache).
  4. **System & Tokio Runtime Telemetry:**
     - Thu thập chỉ số tài nguyên tiến trình: số lượng Tokio active tasks, thread pool saturation, memory RSS, open file descriptors.

---

### Bước 3: Nền tảng Lưu trữ Cơ sở dữ liệu quan hệ (`omni-database`) - [ĐÃ HOÀN THÀNH]

- **Crate thực hiện:** `libs/database` (`omni-database`) kết hợp đặc tả [`relational-persistence-architecture.md`](./docs/architecture/relational-persistence-architecture.md).
- **Vấn đề giải quyết:** Cung cấp lớp trừu tượng hóa chuẩn hóa cho toàn bộ các dịch vụ tương tác với PostgreSQL qua SQLx.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Enterprise PgPool Factory:**
     - Khởi tạo `sqlx::PgPool` với cấu hình tối ưu tải: `max_connections`, `min_connections`, `acquire_timeout`, `idle_timeout`, `max_lifetime`.
     - Cơ chế Ping kiểm tra kết nối nền bất đồng bộ (Keepalive).
  2. **Automated Migration Runner:**
     - Cung cấp hàm bọc `run_migrations(&PgPool)` sử dụng `sqlx::migrate!`.
     - Đảm bảo tính idempotent, hỗ trợ thực thi an toàn trong Kubernetes Init Containers hoặc trong luồng khởi động service.
  3. **Transaction Management Wrapper (Unit of Work):**
     - Hàm tiện ích thực thi trong transaction: `with_transaction(pool, |tx| async move { ... })`.
     - Tự động thực hiện `COMMIT` khi closure trả về `Ok(T)` và tự động `ROLLBACK` an toàn khi phát sinh lỗi `Err(AppError)`.
  4. **Database Observability & Slow Query Logging:**
     - Tạo `tracing::Span` cho mỗi truy vấn SQL, gắn kèm nhãn `db.system = "postgresql"`, `db.statement`.
     - Tự động ghi log cảnh báo mức `WARN` khi câu lệnh truy vấn vượt quá ngưỡng định trước (ví dụ: execution time > 200ms).
     - Gắn W3C `trace_id` hiện tại vào comment SQL (`/* trace_id=... */`) để DBA có thể liên kết trực tiếp một slow query trên Postgres về APM Trace trên Grafana Tempo.
  5. **Common Entity Auditing Model:**
     - Định nghĩa trait hoặc struct dùng chung cho các trường dữ liệu bắt buộc: `id` (UUIDv7 time-ordered), `created_at` (RFC 3339 UTC), `updated_at`, `created_by`, `updated_by`, `deleted_at` (hỗ trợ Soft Delete an toàn).

---

### Bước 4: Bộ nhớ đệm phân tán & Kiểm soát đồng thời (`omni-cache`) - [ĐÃ HOÀN THÀNH]

- **Crate thực hiện:** `libs/cache` (`omni-cache`).
- **Vấn đề giải quyết:** Tối ưu hóa độ trễ đọc, giảm tải cho PostgreSQL, và giải quyết triệt để tranh chấp dữ liệu (Race Condition) trong môi trường phân tán nhiều Pod replica.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Redis Async Multiplexed Manager:**
     - Sử dụng `redis::aio::ConnectionManager` hỗ trợ tự động kết nối lại khi rớt mạng mà không làm gián đoạn tiến trình.
  2. **Cache Stampede Protection (Chống sập cache):**
     - Xử lý hiện tượng hàng nghìn request cùng đọc dữ liệu khi một cache key bị hết hạn.
     - Triển khai cơ chế khóa cục bộ (Local Mutex / SingleFlight) kết hợp thuật toán tính hạn ngẫu nhiên (Probabilistic Early Expiration / Jitter TTL).
  3. **Distributed Locking (Khóa phân tán Redlock):**
     - Cung cấp abstraction `DistributedLock` với lease time (TTL) tự động giải phóng.
     - Giải phóng khóa an toàn bằng mã băm nguyên tử qua Redis Lua script (ngăn chặn việc Pod A vô tình giải phóng khóa của Pod B khi bị quá hạn).
  4. **Key Namespacing & Payload Compression:**
     - Tiêu chuẩn hóa cú pháp key: `omni:{service_name}:{resource}:{id}`.
     - Tự động nén dữ liệu qua `zstd` hoặc `lz4` nếu payload JSON serialize vượt quá dung lượng ngưỡng (ví dụ > 8KB).

---

### Bước 5: Nền tảng Nhận dạng & Bảo mật truy cập (`omni-auth`)

- **Crate thực hiện:** `libs/auth` (`omni-auth`).
- **Vấn đề giải quyết:** Định danh người dùng, kiểm soát phân quyền (RBAC/ABAC) và truyền dẫn ngữ cảnh định danh xuyên suốt hệ thống microservices.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **JWT Engine & Claims Specification:**
     - Sử dụng `jsonwebtoken` để tạo, ký và thẩm định tính toàn vẹn của Access Token và Refresh Token.
     - Struct `AuthUserClaims` chuẩn: `sub` (User ID UUIDv7), `email`, `roles`, `permissions`, `session_id`, `iss`, `aud`, `exp`, `iat`.
     - Hỗ trợ cơ chế luân chuyển khóa bất đối xứng (Asymmetric RSA/EdDSA hoặc JWKS endpoint) hoặc khóa đối xứng (HMAC-SHA256).
  2. **Password Cryptography Standard:**
     - Triển khai cơ chế băm và so khớp mật khẩu sử dụng `argon2` tuân thủ tiêu chuẩn bảo mật OWASP (m_cost, t_cost, p_cost, salt ngẫu nhiên bảo mật cao).
  3. **Axum Authentication Extractors & Authorization Guards:**
     - `AuthUser`: Trích xuất danh tính người dùng từ header `Authorization: Bearer <token>` hoặc header nội bộ đã được Gateway xác thực (`X-User-Id`, `X-User-Roles`).
     - `RequireRole(Role::Admin)`: Guard từ chối request trả về `403 Forbidden` (`omni-errors`) nếu người dùng không đủ quyền.
     - `RequirePermission("orders:create")`: Guard kiểm tra quyền chi tiết theo ma trận phân quyền.
  4. **Tracing Context Enrichment:**
     - Khi request được xác thực thành công, middleware tự động đính kèm `user.id` và `tenant.id` vào `tracing::Span` và `RequestContext` để mọi log phát sinh sau đó đều có vết danh tính người dùng.

---

### Bước 6: Kiến trúc Giao tiếp Hướng sự kiện & Khung gửi tin cậy (`omni-messaging`)

- **Crate thực hiện:** `libs/messaging` (`omni-messaging`).
- **Vấn đề giải quyết:** Tách rời liên kết giữa các microservices (Decoupled Services) thông qua trao đổi sự kiện bất đồng bộ mà vẫn bảo đảm tính tin cậy tuyệt đối, không làm mất mát dữ liệu.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **CNCF CloudEvents Standard Envelope:**
     - Chuẩn hóa định dạng đóng gói mọi event:
       - `id`: Event UUIDv7 duy nhất.
       - `source`: Tên microservice phát hành (vd: `urn:omni:service:order`).
       - `type`: Tên sự kiện (vd: `com.omni.order.created.v1`).
       - `time`: Dấu thời gian chuẩn RFC 3339 UTC.
       - `traceparent`: Ngữ cảnh W3C Distributed Trace Context.
       - `data`: Payload dữ liệu nghiệp vụ.
  2. **Transactional Outbox Pattern Implementation:**
     - Cung cấp mô hình Outbox nhằm loại bỏ nhu cầu sử dụng giao thức 2-Phase Commit (2PC):
     - Dữ liệu nghiệp vụ và Event thông báo được ghi đồng thời vào PostgreSQL trong cùng một Transaction (`outbox_events` table).
     - Module Outbox Publisher chạy nền định kỳ đọc bảng và xuất bản ra Message Broker (RabbitMQ/Kafka/NATS), cam kết phân phối ít nhất một lần (At-least-once delivery).
  3. **Resilient Consumer Framework:**
     - Khung nhận tin nhắn với cơ chế Retry lũy tiến (Exponential Backoff with Jitter).
     - Tự động định tuyến các message lỗi liên tục sang hàng đợi xử lý sự cố (Dead Letter Queue - DLQ) để đội ngũ kỹ thuật can thiệp thủ công mà không làm nghẽn tiến trình xử lý chính.
  4. **Trace Propagation Across Message Brokers:**
     - Tự động tiêm (Inject) và trích xuất (Extract) W3C `traceparent` qua Message Headers, bảo đảm APM Trace duy trì một mạch liên tục từ lúc người dùng gửi HTTP request ban đầu đến khi worker xử lý xong event ở downstream service.

---

### Bước 7: Khả năng Phục hồi mạng & Kiểm soát luồng (`omni-common` Resilience)

- **Crate thực hiện:** `libs/common` (module `resilience` / `middleware`).
- **Vấn đề giải quyết:** Bảo vệ hệ thống khỏi hiện tượng sụp đổ dây chuyền (Cascading Failure), quá tải lưu lượng đột biến và hỗ trợ cập nhật zero-downtime trên Kubernetes.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Adaptive Rate Limiting:**
     - Giới hạn tốc độ request sử dụng thuật toán Token Bucket / Leaky Bucket (tích hợp `governor` hoặc `tower`).
     - Hỗ trợ rate limit theo IP Client, theo `User-Id`, hoặc theo API Key đối tác.
  2. **Timeout, Concurrency Limiting & Load Shedding:**
     - Sử dụng `tower::timeout` để áp đặt ngưỡng timeout nghiêm ngặt cho mọi request (tránh rò rỉ socket connection).
     - Triển khai `tower::load_shed`: Khi hàng đợi nội bộ vượt quá ngưỡng an toàn, chủ động từ chối các request mới bằng mã lỗi `503 Service Unavailable` thay vì để tràn bộ nhớ RAM dẫn tới crash Pod OOMKilled.
  3. **Circuit Breaker:**
     - Tự động ngắt kết nối sang các downstream service bên ngoài nếu tỷ lệ lỗi hoặc timeout vượt ngưỡng (State: `Closed` -> `Open` -> `Half-Open`).
  4. **Kubernetes Graceful Shutdown Protocol:**
     - Lắng nghe tín hiệu hệ điều hành `SIGINT` và `SIGTERM`.
     - Quy trình hạ cánh an toàn chuẩn K8s:
       1. Nhận tín hiệu `SIGTERM` từ Kubernetes Kubelet.
       2. Tạm dừng nhận kết nối mới từ Service Ingress.
       3. Duy trì cửa sổ chờ (Grace Period Window, ví dụ 15 giây) để xử lý dứt điểm toàn bộ các HTTP requests và Outbox jobs đang chạy dở.
       4. Đóng toàn bộ PgPool, Redis connections, và flushes toàn bộ log buffer non-blocking sang Grafana Loki/Tempo.
       5. Thoát tiến trình an toàn với exit code `0`.

---

### Bước 8: Tự động hóa Tài liệu API tập trung (OpenAPI / Swagger Tooling)

- **Crate thực hiện:** `libs/common` (macro helpers) kết hợp đặc tả [`api-documentation-architecture.md`](file://docs/architecture/api-documentation-architecture.md).
- **Vấn đề giải quyết:** Đồng bộ tài liệu API trực tiếp từ mã nguồn Rust mà không cần viết tài liệu thủ công, phục vụ tổng hợp tập trung tại Gateway.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Utoipa Schema Abstraction:**
     - Đăng ký sẵn các dẫn xuất schema OpenAPI cho các cấu trúc cốt lõi: `ApiResponse<T>`, `ApiErrorResponse`, `ResponseMeta`, `PaginationMeta`, `InvalidParamError`.
  2. **Automated Documentation Endpoint:**
     - Khởi tạo route nội bộ chuẩn `/v3/api-docs/openapi.json` trên mỗi microservice để xuất bản tài liệu OpenAPI v3.1 lúc runtime.
  3. **Gateway Dynamic Aggregator Integration:**
     - Đảm bảo định dạng xuất ra tương thích 100% với giao thức tổng hợp tập trung của API Gateway để hiển thị trên màn hình Swagger UI duy nhất.

---

### Bước 9: Bộ khung Service mẫu & Môi trường phát triển cục bộ (Scaffold & Dev Harness)

- **Crate thực hiện:** Khung mẫu `AppBootstrap` và thư mục `infra/docker`, `tests/`.
- **Vấn đề giải quyết:** Chuẩn hóa cách thức một microservice được lắp ghép từ các thư viện nền tảng, cung cấp môi trường kiểm thử tích hợp tự động không phụ thuộc hạ tầng thật.
- **Các nhiệm vụ kỹ thuật cụ thể:**
  1. **Standard `AppBuilder` Pattern:**
     - Xây dựng mẫu thiết kế Builder chuẩn để khởi tạo ứng dụng Axum với đầy đủ các tầng Middleware theo đúng thứ tự ưu tiên:
       `Tracing Context -> Observability/Metrics -> Resilience/Timeout -> CORS/Security Headers -> Auth Extractor -> Business Routes -> Error Handler`.
  2. **Local Multi-Service Compose Harness (`process-compose.yaml` / Docker Compose):**
     - Thiết lập môi trường chạy thử cục bộ hoàn chỉnh gồm: PostgreSQL, Redis, Grafana Loki, Grafana Tempo, Prometheus, Jaeger, RabbitMQ/Kafka.
  3. **Integration Test Suite với Testcontainers:**
     - Thiết lập hạ tầng kiểm thử tích hợp tự động (`tests/integration_k8s.rs`) sử dụng thư viện `testcontainers` để tự động kéo container Postgres/Redis độc lập khi chạy `cargo test --all`.

---

## 5. Mẫu khởi tạo Service chuẩn (The Golden Path Example)

Sau khi hoàn thành 9 bước trên, bất kỳ một Domain Service mới nào trong `services/*` (ví dụ: `services/order`) sẽ được khởi tạo cực kỳ tinh gọn, kế thừa toàn bộ sức mạnh nền tảng:

```rust
// services/order/src/main.rs
use omni_common::bootstrap::AppBuilder;
use omni_common::config::AppConfig;
use omni_errors::AppError;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Nạp cấu hình đa tầng & Khởi tạo Logging / Tracing non-blocking
    let config = AppConfig::load("order-service")?;
    let _log_guard = omni_logging::init(&config.logging)?;

    // 2. Khởi tạo kết nối Dữ liệu, Bộ nhớ đệm và Đo lường
    let db_pool = omni_database::create_pool(&config.database).await?;
    let cache_manager = omni_cache::create_manager(&config.redis).await?;
    let metrics_handle = omni_metrics::init_prometheus_exporter()?;

    // 3. Chạy migration cơ sở dữ liệu nếu cần
    omni_database::run_migrations(&db_pool).await?;

    // 4. Lắp ghép ứng dụng bằng AppBuilder (Chuẩn mực Plug-and-Play)
    let app = AppBuilder::new("order-service")
        .with_state(AppState { db_pool, cache_manager })
        .with_observability_middleware() // RequestContext, W3C traceparent, Server-Timing
        .with_metrics_middleware()       // RED metrics cho Prometheus
        .with_resilience_middleware()    // Timeout, Rate Limiting, Load Shedding
        .with_security_headers()         // HSTS, CSP, X-Frame-Options
        .with_k8s_probes()               // /livez, /readyz, /startupz
        .with_metrics_endpoint(metrics_handle) // /metrics
        .with_openapi_docs(ApiDoc::openapi())  // /v3/api-docs/openapi.json
        .mount_api_routes(routes::order_router())
        .build();

    // 5. Khởi động Web Server kèm giao thức Graceful Shutdown chuẩn Kubernetes
    omni_common::server::serve_with_graceful_shutdown(app, config.server.port).await?;

    Ok(())
}
```

---

## 6. Tiêu chuẩn nghiệm thu & Kiểm soát chất lượng (Quality Gate Checklist)

Theo quy định bắt buộc tại [`AGENTS.md`](file://AGENTS.md) và [`.agents/rules/quality-gate.md`](file://.agents/rules/quality-gate.md), trước khi hoàn tất mỗi bước và commit code vào git:

1. **Chuẩn hóa định dạng mã nguồn:**

   ```bash
   cargo fmt --all
   ```

2. **Đồng bộ cây phụ thuộc workspace:**

   ```bash
   cargo hakari generate
   ```

3. **Kiểm tra Linter nghiêm ngặt (Không chấp nhận bất kỳ cảnh báo nào):**

   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

4. **Vượt qua 100% bộ kiểm thử tự động:**

   ```bash
   cargo test --all
   ```

5. **Ghi nhật ký thay đổi:** Cập nhật ngay vào mục `[Unreleased]` trong [`CHANGELOG.md`](file://CHANGELOG.md).
