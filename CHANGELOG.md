# Changelog

Tất cả những thay đổi đáng chú ý của dự án **OmniKubernetes** sẽ được ghi chép tại file này.

Định dạng tài liệu này tuân thủ theo chuẩn [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
và phiên bản tuân theo [Semantic Versioning (SemVer)](https://semver.org).

---

## Hướng dẫn dành cho Devs (Quy tắc ghi chép)

- **Khi đang làm việc:** Ghi ngay thay đổi vào mục `[Unreleased]` tương ứng bên dưới khi merge code/PR.
- **Khi Release phiên bản mới:**
  1. Đổi tên `[Unreleased]` thành `[X.Y.Z] - YYYY-MM-DD` (VD: `[0.1.0] - 2026-10-29`).
  2. Tạo lại một block `## [Unreleased]` trống ở trên cùng cho lần release tiếp theo.
  3. Cập nhật link so sánh tag ở cuối file.
- **Phân loại mục:**
  - `Added`: Tính năng mới hoặc bổ sung API mới.
  - `Changed`: Thay đổi hành vi, cấu trúc hoặc cập nhật logic hiện có.
  - `Deprecated`: Tính năng vẫn chạy nhưng dự kiến sẽ bị xóa trong bản tới.
  - `Removed`: Tính năng đã bị gỡ bỏ hẳn khỏi hệ thống.
  - `Fixed`: Sửa các lỗi (bug fix).
  - `Security`: Bản vá bảo mật, khắc phục lỗ hổng.

---

## [Unreleased]

### Added

- **Khả năng Phục hồi mạng & Kiểm soát luồng (`omni-common` Resilience & `omni-cache` Limiter):**
  - Đặc tả kiến trúc hệ thống chuẩn doanh nghiệp: `docs/architecture/resilience-traffic-control-architecture.md` (Phiên bản `1.0.0`, 10 nguyên lý cốt lõi).
  - Triển khai toàn diện module `libs/common/src/resilience/` (`omni-common`) và tích hợp phân tán trong `omni-cache`:
    - **Adaptive Rate Limiting 2 tầng (`rate_limit.rs` & `omni_cache::limiter`):**
      - Tầng 1 (In-Memory Fast-Path): Thuật toán GCRA (Generic Cell Rate Algorithm) lock-free sử dụng `AtomicU64` tính toán Theoretical Arrival Time (TAT) và CAS loop, đạt độ trễ kiểm tra nanosecond (< 50ns) không phát sinh network call hay cấp phát heap.
      - Tầng 2 (Distributed Sliding Window): Trait `DistributedRateLimiter` và hiện thực hóa `RedisRateLimiter` trong `omni-cache` (`limiter.rs`) sử dụng Redis Lua script trượt thời gian nguyên tử (`EVALSHA`), bảo đảm hạn ngạch toàn cụm Pod replica.
      - Phân cấp định danh đa chiều (`RateLimitKey`: `usr:`, `tnt:`, `key:`, `ip:`) với tính năng chống giả mạo IP qua danh sách `trusted_proxies`.
      - Phân tầng Route (`RouteTier`: `Sensitive`, `Heavy`, `Standard`, `ReadOnly`) với hệ số điều chỉnh hạn ngạch.
      - Tower Layer & Service (`RateLimitLayer`, `RateLimitService`) tự động tiêm headers IETF: `RateLimit-Limit`, `RateLimit-Remaining`, `RateLimit-Reset`, `Retry-After`.
    - **Adaptive Load Shedding & Concurrency Limiting (`load_shed.rs`):**
      - Áp dụng định luật Little $L = \lambda \cdot W$ qua Tokio `Semaphore` giới hạn số lượng request đồng thời trần (`max_concurrent_requests`).
      - Chủ động từ chối request bằng mã lỗi HTTP `503 Service Unavailable` tức thì trong < 10 micro-giây khi hệ thống bão hòa, loại bỏ nguy cơ phình hàng đợi vô hạn (Unbounded Queueing) và ngăn ngừa Pod bị Linux `OOMKilled` (Exit Code 137).
      - RAII Guard `ConcurrencyPermit` tự động cập nhật số lượng request in-flight.
    - **Multi-Tier Request Timeout & Cancellation Propagation (`timeout.rs`):**
      - Áp đặt thời hạn xử lý tối đa qua Tower Layer `TimeoutLayer`, trả về lỗi HTTP 504 / 408 Problem Details khi hết hạn.
      - Tích hợp `with_cancellation_token` liên kết `tokio_util::sync::CancellationToken` để dừng ngay các tác vụ SQL/DB nền khi Client ngắt kết nối hoặc timeout, chống lãng phí CPU/DB (Wasted Work).
      - Tiêm header `Request-Timeout` khi gọi downstream để lan truyền deadline.
    - **Enterprise 3-State Circuit Breaker (`circuit_breaker.rs`):**
      - Máy trạng thái hữu hạn FSM 3 pha: `Closed` $\leftrightarrow$ `Open` $\leftrightarrow$ `HalfOpen` với cửa sổ trượt (Sliding Window) thống kê kết quả cuộc gọi.
      - Tự động ngắt mạch sang `Open` khi tỷ lệ lỗi vượt `circuit_breaker_error_threshold_pct` hoặc tỷ lệ cuộc gọi chậm vượt ngưỡng sau `min_requests`.
      - Cơ chế thăm dò tự phục hồi thận trọng trong `HalfOpen` với hạn ngạch thử nghiệm probe requests.
      - Ma trận phân định lỗi nghiêm ngặt: Lỗi hạ tầng (500, 502, 503, 504, Connection Refused, Timeout) được tính vào Circuit Breaker; lỗi nghiệp vụ Client (400, 401, 403, 404, 409, 422) hoàn toàn bị loại trừ.
    - **Chống Bão Thử lại & Exponential Backoff with Jitter (`retry.rs`):**
      - Triển khai thuật toán **AWS Full Jitter Exponential Backoff**: $T_{\text{sleep}} = \text{random}(0, \min(T_{\text{max}}, T_{\text{base}} \times 2^{\text{attempt}}))$.
      - Chốt chặn an toàn lũy thừa `is_idempotent_request`: Cấm tuyệt đối thử lại request `POST`/`PATCH` trừ khi có header `Idempotency-Key` hoặc `X-Idempotency-Key`.
      - Hạn ngạch thử lại Token Bucket (`RetryBudget`): Giới hạn tổng số request thử lại không vượt quá 20% tổng lưu lượng hệ thống.
    - **7-Stage Zero-Downtime Kubernetes Graceful Shutdown Coordinator (`shutdown.rs`):**
      - Hóa giải race condition giữa tín hiệu `SIGTERM` của Kubelet và việc cập nhật bảng định tuyến mạng của Ingress/EndpointSlice.
      - Phối hợp tuần tự 7 giai đoạn: Bắt tín hiệu OS -> Kích hoạt Drain Gate `/readyz` 503 qua `ShutdownDrainGate` -> Ngủ chờ Quiescence -> Drain In-flight HTTP requests -> Dừng Background Workers qua `CancellationToken` -> Xả PgPool, Redis và Log Buffers -> Thoát an toàn mã 0.
      - `ProbeState` trong `omni-metrics` (`libs/metrics/src/probes.rs`) thực thi trait `ShutdownDrainGate`.
    - **Hợp đồng Lỗi RFC 9457 & Deep Observability (`error.rs`, `metrics.rs`):**
      - Chuẩn hóa các lỗi `RateLimited`, `LoadShedRejected`, `CircuitBreakerOpen`, `RequestTimeout` thành RFC 9457 Problem Details (`application/problem+json`).
      - Bộ chỉ số Prometheus: `http_requests_rate_limited_total`, `resilience_circuit_breaker_state`, `resilience_circuit_breaker_transitions_total`, `resilience_circuit_breaker_rejected_total`, `resilience_load_shed_rejected_total`, `resilience_concurrency_in_flight`, `resilience_retry_attempts_total`, `resilience_retry_exhausted_total`, `resilience_shutdown_stage_duration_seconds`.
    - **Mô hình Cấu hình Phân tầng (`config/resilience.rs`):**
      - Mở rộng `ResilienceConfig` với 5 nhóm tham số kiểm soát luồng và khả năng chịu lỗi, không dùng comment banner kiểu `// ====`.
    - **Bộ kiểm thử toàn diện:**
      - Bộ test tích hợp `libs/common/tests/resilience_tests.rs` bao phủ 12/12 test cases cho toàn bộ các thuật toán và middleware.
- **Kiến trúc Giao tiếp Hướng sự kiện & Khung gửi tin cậy (`omni-messaging`):**
  - Đặc tả kiến trúc hệ thống chuẩn doanh nghiệp: `docs/architecture/event-driven-messaging-outbox-architecture.md`.
  - Triển khai toàn diện crate `libs/messaging` (`omni-messaging`) bám sát 9 nguyên lý kiến trúc cốt lõi:
    - **CloudEvents Engine (`cloudevent::*`):** Tuân thủ 100% đặc tả CNCF CloudEvents v1.0.2 (`CloudEvent<T>`, `CloudEventBuilder`, `CloudEventExtensions`), hỗ trợ chuyển đổi linh hoạt giữa Structured Content Mode và Binary Content Mode, time-ordered UUIDv7 event identifiers, UTC RFC 3339 timestamps.
    - **Transactional Outbox Engine (`outbox::*`):** Triển khai `OutboxRepository` liên kết trực tiếp với `omni_database::UnitOfWork` để loại bỏ hoàn toàn Dual-Write Dilemma bằng ACID Local Transaction; tối ưu chỉ mục partial B-Tree phục vụ Polling.
    - **Hybrid Outbox Relay Worker (`outbox::relay`):** Kết hợp PostgreSQL `LISTEN/NOTIFY` (`omni_outbox_tick`) để thức tỉnh tức thì không độ trễ và `SELECT ... FOR UPDATE SKIP LOCKED` để kéo dữ liệu theo lô an toàn giữa nhiều instance pod song song.
    - **Broker Transport Abstraction & Pluggable Drivers (`broker::*`):** Các trait cốt lõi `MessagePublisher`, `MessageConsumer`, `RawBrokerMessage`. Triển khai driver `InMemoryBroker` phục vụ kiểm thử độc lập, `RabbitMqBroker` (AMQP 0-9-1 với `lapin`), adapter stubs cho Apache Kafka và NATS JetStream.
    - **Dual-Layer Idempotent Inbox Engine (`inbox::*`):** Cơ chế bảo vệ lũy thừa 2 tầng chống xử lý trùng lặp: Fast-path trên Redis (`omni-cache` / `SET NX EX`) và Strong-consistency trên PostgreSQL (`inbox_events` với `ON CONFLICT DO NOTHING`).
    - **Resilient Consumer Pipeline (`consumer::*`):** Bộ điều phối worker pool chịu lỗi với concurrency control, Exponential Backoff kèm Full Jitter, Poison Pill detection và tự động định tuyến Dead Letter Queue (DLQ) kèm chẩn đoán lỗi (`DeadLetterHeaders`).
    - **Declarative Message Handlers (`consumer::handler`):** Trait `MessageHandler<T>` tự động deserialize payload CloudEvent, bọc ngữ cảnh `MessageContext` và tự động xử lý Ack/Nack/Retry/DLQ.
    - **Housekeeping & Retention Engine (`outbox::cleanup`):** `OutboxRetentionWorker` dọn dẹp định kỳ các bản ghi Outbox và Inbox đã hoàn tất vượt ngưỡng retention_hours để chống phình to bảng (Table Bloat).
    - **Distributed Tracing & Telemetry (`tracing::*`, `instrument::*`):** W3C Carrier Injection & Extraction (`traceparent`, `tracestate`) tích hợp OpenTelemetry Semantic Conventions v1.27+, bộ chỉ số Prometheus chuẩn hóa (`messaging_outbox_*`, `messaging_consumer_*`, `messaging_dlq_*`) và bộ lọc PII sanitization cho payload event.
    - **Bảng phân loại lỗi & RFC 9457 Adapter (`error::*`):** Phân định `MessagingError`, `MessageProcessingError` (Transient vs Terminal) và ánh xạ sang `omni_errors::AppError`.
    - **Bộ kiểm thử tự động toàn diện:** Bao phủ 23 bài test đơn vị và tích hợp trong `tests/` (`cloudevent_tests.rs`, `retry_backoff_tests.rs`, `w3c_propagation_tests.rs`, `memory_broker_tests.rs`, `idempotent_consumer_tests.rs`, `outbox_model_and_config_tests.rs`).
- Đặc tả kiến trúc hệ thống trong `docs/architecture/`:
  - Chuẩn giao thức phản hồi và đa ngôn ngữ cấp doanh nghiệp (`base-response-architecture.md`).
  - Chuẩn tài liệu hóa API và tổng hợp OpenAPI tập trung tại Gateway (`api-documentation-architecture.md`).
  - Chuẩn quản lý cấu hình phân cấp kiểu tĩnh an toàn (`configuration-management-architecture.md`).
  - Chuẩn Metrics Telemetry, RED Middleware & Kubernetes Probes (`metrics-probes-telemetry-architecture.md`).
  - Chuẩn kiến trúc lưu trữ quan hệ, Transaction & Resilience cho PostgreSQL (`relational-persistence-architecture.md`).
  - Chuẩn Bộ nhớ đệm phân tán, Chống Stampede & Khóa phân tán với Fencing Token (`distributed-caching-concurrency-architecture.md`).
  - Chuẩn Nền tảng Nhận dạng, Quản lý Phiên & Bảo mật Truy cập Zero-Trust (`identity-security-context-architecture.md`).
- Triển khai toàn diện Nền tảng Nhận dạng & Bảo mật Truy cập `omni-auth` (`libs/auth`):
  - **Mô hình Cấu hình Mở rộng & Startup Guardrails (`omni-common::config::auth`):**
    - Mở rộng `AuthConfig` hỗ trợ đa chế độ: `Symmetric`, `AsymmetricPem`, `JwksRemote`.
    - Hỗ trợ đa thuật toán ký số: `HS256`, `HS384`, `HS512`, `RS256`, `EdDSA`, `ES256`, `ES384`, `ES512`.
    - Bổ sung các cấu hình `AsymmetricConfig` (`private_key_pem`, `public_key_pem`), `JwksConfig` (`url`, `refresh_interval_secs`, `cache_ttl_secs`), `Argon2Config` (`memory_kib`, `iterations`, `parallelism`, `salt_length_bytes`).
    - Các chốt chặn an toàn khởi động (Fail-Fast Guardrails): `CFG_PROD_JWT_ENTROPY`, `CFG_AUTH_DEV_SECRET_IN_PROD`, `CFG_AUTH_MISSING_ASYMMETRIC_KEYS`, `CFG_AUTH_INVALID_TTL_RATIO`, `CFG_AUTH_ARGON2_TOO_WEAK`.
  - **Cơ chế Băm & Xác thực Mật khẩu Argon2id (`crypto::password`):**
    - Triển khai `PasswordSecurity` tuân thủ khuyến nghị OWASP Password Storage và RFC 9106.
    - Chạy non-blocking trên Tokio blocking thread pool thông qua `tokio::task::spawn_blocking` để triệt tiêu nguy cơ nghẽn Event Loop (Worker Thread Starvation).
    - So khớp mật khẩu đẳng thời (Constant-Time Verification) chống Timing Side-Channel Attacks.
  - **Động cơ JWT & Đặc tả Claims Chặt chẽ (`token`):**
    - Cấu trúc `AuthUserClaims` tuân thủ RFC 7519 & RFC 9562 với các định danh UUIDv7 (`sub`, `session_id`, `tenant_id`, `jti`).
    - Hỗ trợ mô hình phân quyền RBAC (`Role::SuperAdmin`, `Admin`, `Operator`, `User`, `ServiceAccount`, `Custom`) kết hợp Fine-Grained Permissions dạng `domain:action` và wildcard matching (`orders:*`, `*`).
    - `TokenEngine` quản lý luân chuyển khóa qua `kid`, ký và thẩm định đa thuật toán với `jsonwebtoken 11.1.0`.
    - Tạo cặp `TokenPair` chuẩn OAuth2 Bearer (`access_token`, `refresh_token`).
  - **Giao thức Gateway Signed Context (`middleware::signed_context`):**
    - Giao thức chữ ký số HMAC-SHA256 bảo vệ mạng nội bộ pod-to-pod trong Kubernetes chống giả mạo Header (`X-User-Id`, `X-Tenant-Id`).
    - Freshness Check chống Replay Attack dựa trên `X-Omni-Timestamp` và `signature_max_age_secs`.
  - **Kiến trúc Phân tầng Thu hồi Token (Tiered Revocation) & RTR (`revocation`):**
    - JTI Blacklist trên `omni-cache` (Redis) với TTL chính xác bằng thời gian sống còn lại của Access Token.
    - Refresh Token Rotation (RTR) với cơ chế phát hiện tái sử dụng trái phép (Token Theft Detection) và vô hiệu hóa tức thời toàn bộ Token Family.
  - **Axum 0.8 Extractors & Declarative Guards (`extractor`, `guard`):**
    - Extractors kiểu tĩnh: `AuthenticatedUser` (bắt buộc), `OptionalUser` (tùy chọn), `CurrentTenant` (cô lập đa tenant), `BearerToken`.
    - Route Layers khai báo: `require_role`, `require_any_role`, `require_permission`, `require_all_permissions`.
  - **Tích hợp RFC 9457 & Đa ngôn ngữ i18n (`error`):**
    - Bảng phân loại `AuthError` chuẩn hóa (`ERR_AUTH_*`) và tự động chuyển đổi sang `omni_errors::AppError` (`401 Unauthorized`, `403 Forbidden`).
    - Bổ sung 100% bản dịch chuỗi lỗi `ERR_AUTH_*` cho cả 3 ngôn ngữ `vi-VN`, `en-US`, `ja-JP`.
  - **Context Bridging & Observability (`bridge`, `instrument`):**
    - Đồng bộ tự động danh tính sang `RequestContext`, OpenTelemetry Span attributes, và PostgreSQL `SessionContext` (`omni.actor_id`, `omni.tenant_id`).
    - Bộ chỉ số Prometheus: `auth_tokens_validated_total`, `auth_failures_total`, `auth_crypto_duration_seconds`.
- Triển khai thư viện lõi `omni-common` (`libs/common`):
  - **Type-Safe Configuration & Environment Binding (`config`):**
    - Bộ nạp cấu hình phân cấp 5 tầng `ConfigLoader` (`loader.rs`): Nạp và hợp nhất sâu theo thứ tự ưu tiên tăng dần: Safe Defaults -> `config/base.yaml` -> `config/{env}/default.yaml` -> System Environment `OMNI__*` (kèm hỗ trợ `.env` cục bộ cho môi trường Dev) -> Programmatic Overrides (`with_override`, `with_env_vars`).
    - Quy ước tiền tố `OMNI__` (Double-Underscore) phân tách chính xác các cấp độ lồng nhau mà vẫn bảo toàn tên trường `snake_case` trong Rust struct (`OMNI__DATABASE__MAX_CONNECTIONS` -> `database.max_connections`).
    - Hỗ trợ bí danh tiêu chuẩn Cloud-Native và Kubernetes Downward API fallbacks: `APP_ENV`/`RUST_ENV`/`ENV` -> `service.environment`, `RUST_LOG`/`LOG_LEVEL` -> `logging.log_level`, `OTEL_EXPORTER_OTLP_ENDPOINT` -> `logging.otlp_endpoint`, `K8S_POD_NAME`/`HOSTNAME` -> `k8s.pod_name`, `K8S_NAMESPACE` -> `k8s.namespace`, `K8S_NODE_NAME` -> `k8s.node_name`.
    - Quy trình thẩm định tức thời hai pha (Fail-Fast Validation): Pha 1 kiểm tra ràng buộc trường qua `validator::Validate` và Pha 2 kiểm tra các bất biến liên trường & chốt chặn bảo mật Production (`CFG_DB_POOL_BOUNDS`, `CFG_TIMEOUT_HIERARCHY`, `CFG_PROD_SECRET_REQUIRED`, `CFG_PROD_JWT_ENTROPY`, `CFG_PROD_SWAGGER_UI_DISABLED`, `CFG_PROD_CORS_STRICT`, `CFG_I18N_DEFAULT_IN_SUPPORTED`), gom toàn bộ lỗi vào `ConfigError::ValidationFailed` kèm gợi ý biến môi trường `OMNI__*`.
    - Bảo vệ thông tin mật tuyệt đối (Zero-Leak Secrets): Bọc toàn bộ mật khẩu, khóa ký JWT, chuỗi kết nối vào `secrecy::SecretString`, tự động zeroize bộ nhớ khi drop, che giấu `[REDACTED]` khi format Debug/Display/JSON Summary, cùng các hàm tạo URL an toàn `database.redacted_url()`, `redis.redacted_url()`, `messaging.redacted_broker_url()`.
    - Cấu trúc gốc mở rộng kiểu tĩnh `AppConfig<C = EmptyCustomConfig>` tích hợp sẵn toàn diện các phân hệ: `service`, `k8s`, `server`, `logging`, `observability`, `database`, `redis`, `auth`, `messaging`, `resilience`, `i18n`, và cho phép gắn cấu hình domain nghiệp vụ riêng thông qua `custom: C`.
    - Cầu nối chuyển đổi sang `omni_logging::LoggingConfig` qua `config.to_logging_config()`.
    - Hệ thống tập tin cấu hình chuẩn: `config/base.yaml`, `config/dev/default.yaml`, `config/staging/default.yaml`, `config/prod/default.yaml`, và `.env.example`.
    - Bộ kiểm thử toàn diện `libs/common/tests/configuration_tests.rs` xác thực cả 5 tầng phân cấp, parsing `OMNI__`, Fail-Fast, Zero-Leak secrets, Production guardrails và cấu hình domain mở rộng.
  - **Base Response Data Contracts (`response`):** Cấu trúc `ApiResponse<T>`, `ApiResponseBuilder<T>`, `ResponseMeta` sử dụng `Cow<'static, str>` tối ưu hóa zero-allocation cho chuỗi tĩnh, định dạng thời gian chuẩn ISO 8601 / RFC 3339 UTC độ chính xác mili-giây (`YYYY-MM-DDTHH:mm:ss.sssZ`), cùng các hàm tạo response chuẩn ngữ nghĩa HTTP RFC 9110 (`200 OK`, `201 Created` kèm header `Location`, `204 No Content` không có body).
  - **Dual Pagination Architecture (`response`):** Hỗ trợ song song phân trang `OffsetPaginationMeta` (`mode = "offset"`) và `CursorPaginationMeta` (`mode = "cursor"`) qua Internally Tagged Enum `PaginationMeta` và `PaginatedResponse<T>`.
  - **Internationalization & Localization (`i18n`):**
    - Thuật toán chuẩn hóa ngôn ngữ BCP 47 & RFC 4647 (`match_canonical_locale`, `match_accept_language`) không cấp phát bộ nhớ động (0 heap allocation), hỗ trợ ánh xạ tiền tố ngôn ngữ (`vi` -> `vi-VN`, `en`/`en-GB` -> `en-US`, `ja` -> `ja-JP`) và phân tích trọng số `q` trong header `Accept-Language`.
    - Bộ trích xuất `RequestLocale` (Axum `FromRequestParts`) thực thi quy trình xác định ngôn ngữ 5 bước không gây nghẽn I/O: Query Parameter (`?lang`, `?locale`) -> Header `X-Locale` -> JWT `AuthUserClaims` -> Header `Accept-Language` -> Fallback mặc định (`vi-VN`).
    - Bộ quản lý bản dịch Mozilla Fluent `I18nManager` áp dụng mô hình **RCU (Read-Copy-Update)** lock-free với `Arc<ArcSwap<LocaleBundles>>`, kiểm tra toàn vẹn cú pháp `.ftl` trước khi cập nhật, kèm endpoint nội bộ `POST /internal/v1/reload-locales` và trình lắng nghe tín hiệu Unix `SIGHUP` (`spawn_sighup_reload_listener`).
  - **Observability & Server-Timing Middleware (`middleware`):**
    - `request_context_middleware` và `RequestContext`: Trích xuất hoặc tự động sinh `X-Request-Id` (`UUIDv7`) và W3C `traceparent` (`trace_id` 32 ký tự hex), ghi nhận vào `tracing::Span` hiện tại và đảm bảo mọi HTTP response đều đính kèm header `Content-Language`, `X-Request-Id`.
    - `timing_middleware`: Đo lường thời gian xử lý request và gắn trực tiếp vào HTTP Header `Server-Timing: total;dur=...` (W3C Recommendation) cùng `X-Response-Time: ...ms` mà không can thiệp lại JSON body.
- Triển khai thư viện xử lý lỗi `omni-errors` (`libs/errors`):
  - **RFC 9457 Problem Details (`problem`):** Cấu trúc `ApiErrorResponse`, `ApiErrorResponseBuilder` và `InvalidParamError` tuân thủ chuẩn RFC 9457 với header `Content-Type: application/problem+json`.
  - **Dual-mode Validation Localization:** Hàm `InvalidParamError::localized` kết hợp thông điệp dịch sẵn phía Server (`reason`) với mã lỗi (`code`) và tham số nội suy động (`params`) phục vụ bản địa hóa phía Client (Mobile/SPA).
  - **RFC 6901 JSON Pointer Mapping:** Bộ hàm chuyển đổi hai chiều `json_pointer_to_dot_notation` và `dot_notation_to_json_pointer` (`items.0.quantity` <-> `/items/0/quantity`).
  - **Error Codes & Application Error Adapter (`codes`, `app_error`):** Định nghĩa tập trung các hằng số URI loại lỗi (`TYPE_*`), mã lỗi nghiệp vụ (`ERR_*`), mã lỗi kiểm tra trường dữ liệu (`VAL_*`), và enum `AppError` chuyển đổi sang `ApiErrorResponse`.
  - **Zero-Copy Error Code Attribution (`ErrorCodeExtension`):** Cơ chế đính kèm siêu dữ liệu mã lỗi vào `axum::response::Response::extensions()` để truyền sang `omni-metrics` không cần đọc lại body JSON.
- Triển khai thư viện metrics và telemetry `omni-metrics` (`libs/metrics`):
  - **Prometheus Exporter & Configuration (`exporter`, `config`):**
    - Bộ khởi tạo `PrometheusBuilder` và `PrometheusHandle` hỗ trợ cả chế độ Global Singleton idempotent (`init_prometheus`) lẫn Isolated Recorder (`create_isolated_recorder`) cho kiểm thử song song an toàn.
    - Cấu hình `MetricsConfig` với endpoint `/metrics` (Content-Type: `text/plain; version=0.0.4; charset=utf-8`) và gắn nhãn toàn cục `service`, `environment`.
    - Định nghĩa bộ buckets Histogram chuẩn: `HTTP_DURATION_BUCKETS` (11 mốc từ 5ms đến 30s), `DB_DURATION_BUCKETS` (9 mốc từ 1ms đến 10s), `PROBE_DURATION_BUCKETS` (7 mốc từ 10ms đến 5s).
    - Tự động khởi chạy tác vụ nền `PrometheusHandle::run_upkeep()` định kỳ mỗi 5 giây để xả `AtomicBucket` chống rò rỉ bộ nhớ (OOMKilled).
  - **RED Metrics Middleware & Cardinality Protection (`middleware`, `sanitizer`):**
    - Middleware `red_metrics_middleware` thu thập bộ chỉ số RED (Rate - Errors - Duration) và In-Flight Requests cho mọi HTTP request qua Axum.
    - RAII Guard `InFlightGuard` đảm bảo `http_requests_in_flight` luôn được giảm trừ chính xác ngay cả khi Future bị hủy (Client disconnect / Timeout) hoặc Handler gặp Panic.
    - Bộ chuẩn hóa đường dẫn hai tầng chống bùng nổ số chiều (Cardinality Explosion):
      - `normalize_matched_path`: Chuyển đổi mẫu Axum `/api/v1/orders/{id}` thành `/api/v1/orders/:id`.
      - `sanitize_uri_path`: Zero-Regex Byte Scanner tự động quy đổi UUIDv4/v7, ULID, ID có tiền tố (`usr_...`, `ord_...`), chuỗi Hex, số nguyên về `:id`, đồng thời gom toàn bộ `404 Not Found` không khớp route về `"__unmatched__"`.
    - Tích hợp trực tiếp với `omni-errors::ErrorCodeExtension` để ghi nhận chính xác mã lỗi nghiệp vụ (`ERR_VALIDATION_FAILED`, `ERR_UNAUTHORIZED`, v.v.) vào counter `http_request_errors_total` với chi phí cấp phát bằng 0.
    - Tự động bỏ qua các endpoint hạ tầng (`/livez`, `/readyz`, `/startupz`, `/healthz`, `/metrics`) khỏi đo lường RED để tránh nhiễu số liệu.
  - **Kubernetes Tri-Probe Architecture (`probes`):**
    - Phân định rạch ròi trách nhiệm của 3 endpoints theo chuẩn Kubernetes:
      - **`GET /livez` (Liveness Probe):** Chỉ kiểm tra sự sống của tiến trình và khả năng lập lịch của Tokio Reactor (Watchdog Heartbeat). Tuyệt đối không kiểm tra Database/Redis để ngăn thảm họa khởi động lại dây chuyền (Cascading Restart / Thundering Herd).
      - **`GET /readyz` (Readiness Probe):** Kiểm tra song song có giới hạn thời gian (`tokio::time::timeout`) toàn bộ phụ thuộc hạ tầng cốt lõi qua trait `HealthIndicator` (PostgreSQL, Redis, Message Broker) kết hợp cổng chặn Graceful Shutdown Drain (chủ động trả `503` ngay khi nhận `SIGTERM`).
      - **`GET /startupz` (Startup Probe):** Bảo vệ các dịch vụ cần thời gian khởi tạo ban đầu dài (SQLx migrations, nạp cache i18n) khỏi bị `livenessProbe` giết nhầm.
    - Trait mở rộng `HealthIndicator` cho phép các crate hạ tầng (`omni-database`, `omni-cache`, `omni-messaging`) triển khai kiểm tra kết nối riêng.
    - Bộ điều phối `HealthRegistry` chạy song song các `HealthIndicator` và ghi nhận độ trễ phản hồi vào metric `k8s_readiness_check_duration_seconds`, trạng thái sức khỏe vào `k8s_readiness_dependency_status` (1.0 = Healthy, 0.0 = Unhealthy).
    - Quản lý trạng thái vòng đời `ProbeState` (`startup_complete`, `shutting_down`, `watchdog_heartbeat`) với cơ chế Atomic Lock-Free.
    - Phân loại phụ thuộc hạ tầng `DependencyCriticality::Critical` (lỗi -> `/readyz` trả `503`) và `DependencyCriticality::NonCritical` (lỗi -> `/readyz` trả `200` với `status: "degraded"`).
  - **Tokio Runtime & System Telemetry (`system`):**
    - Thu thập chỉ số Tokio Runtime (`tokio::runtime::RuntimeMetrics`): `tokio_runtime_workers_count`, `tokio_runtime_alive_tasks`, `tokio_runtime_global_queue_depth`, `tokio_runtime_worker_busy_duration_seconds`, `tokio_runtime_worker_park_count_total`.
    - Thu thập chỉ số tiến trình hệ điều hành từ `/proc/self/status` và `/proc/self/stat` (Linux container) với cơ chế fallback mượt mà trên macOS: `process_resident_memory_bytes`, `process_virtual_memory_bytes`, `process_cpu_seconds_total`, `process_threads`, `process_open_fds`, `process_max_fds`, `process_uptime_seconds`.
    - Tác vụ nền `spawn_system_metrics_collector` định kỳ mỗi 30 giây cập nhật chỉ số hệ thống.
    - Watchdog heartbeat `tokio_watchdog_check_lag_seconds` đo độ trễ lập lịch thực tế của Tokio Reactor để phát hiện Thread Pool Saturation.
    - Metric phiên bản `omni_build_info` (giá trị cố định `1.0`) kèm nhãn `version`, `rust_version` để đối chiếu phiên bản binary đang chạy trên từng Pod.
    - Tuân thủ nghiêm ngặt `unsafe_code = "deny"` trên toàn workspace và hỗ trợ kiểm thử song song cô lập (`metrics::with_local_recorder`).
  - **Bộ kiểm thử toàn diện:**
    - `libs/metrics/tests/sanitizer_tests.rs`: Kiểm chứng chuẩn hóa đường dẫn (UUIDv4/v7, ULID, ID có tiền tố, số nguyên, Hex về `:id`).
    - `libs/metrics/tests/red_metrics_tests.rs`: Kiểm chứng RED middleware, Zero-Copy error attribution, RAII `InFlightGuard`, và loại trừ health probes.
    - `libs/metrics/tests/probes_tests.rs`: Kiểm chứng `/livez`, `/readyz`, `/startupz` với các trạng thái `starting`, `healthy`, `degraded`, `unhealthy`, `draining`.
    - `libs/metrics/tests/system_tests.rs`: Kiểm chứng thu thập Tokio Runtime và Process metrics.
    - `libs/metrics/tests/exporter_tests.rs`: Kiểm chứng Prometheus exporter và isolated recorder.
- Triển khai thư viện quản lý cơ sở dữ liệu `omni-database` (`libs/database`):
  - **Enterprise PgPool Factory & Connection Management (`pool`, `config`):**
    - Bộ khởi tạo `DbPool` với bounded backoff retry (10 attempts, 250ms-5s exponential backoff) khi khởi động, hỗ trợ kết nối PostgreSQL 16+ qua SQLx 0.9.
    - Cấu hình `DatabaseSettings` mở rộng từ `DatabaseConfig` (`omni-common`) với chuẩn hóa `application_name` (loại bỏ ký tự đặc biệt, tối đa 30 ký tự) và jitter ngẫu nhiên cho `max_lifetime` (0-10%) để tránh reconnect storm đồng loạt.
    - Dựng `PgConnectOptions` bằng builder (không qua URL) để bảo vệ mật khẩu chứa ký tự đặc biệt (`@ : / ? #`), tích hợp SSL/TLS (`verify-full`, `ssl_root_cert_path`), và statement cache LRU.
    - Hỗ trợ chế độ PgBouncer (`pgbouncer_mode = true`) bỏ qua startup parameter `options` (đặt server-side timeouts qua `ALTER ROLE ... SET` thay vì connection parameter).
    - Timeout Hierarchy: `acquire_timeout` (5s) + `statement_timeout` (10s) < `server.request_timeout` (30s) < Ingress timeout, đảm bảo tầng trong luôn hết hạn trước tầng ngoài.
    - Server-side timeouts bắt buộc: `statement_timeout`, `lock_timeout`, `idle_in_transaction_session_timeout` để chặn truy vấn chạy vô hạn ngay cả khi client timeout hoặc bị disconnect.
    - Pool Sentinel (`sentinel.rs`): Tác vụ nền định kỳ mỗi 10s thực thi `SELECT 1` keepalive ping (chỉ khi idle >= `ping_idle_threshold_secs`) và cache trạng thái sức khỏe cho `/readyz` mà không gây probe query thundering herd.
    - Graceful drain trong shutdown: đóng pool theo thứ tự (dừng Sentinel -> đợi in-flight transactions -> `pool.close().await`).
  - **Kubernetes Health Integration (`health`):**
    - Triển khai trait `HealthIndicator` (`omni-metrics`) qua `PgHealthIndicator` với kiểm tra cached state từ Pool Sentinel (không thực thi `SELECT 1` trực tiếp mỗi lần `/readyz`).
    - Báo cáo `PoolHealthState` kèm số kết nối hiện tại, idle, max vào metric `k8s_readiness_dependency_status` và `db_client_connections_usage`.
  - **Unit of Work & Transaction Management (`transaction`, `session`):**
    - API Unit of Work: `with_transaction` (closure-based) và `UnitOfWork` (RAII guard) đảm bảo `COMMIT` khi `Ok`, `ROLLBACK` khi `Err`/drop/panic/future bị hủy.
    - Zero-Round-Trip Session Context: ghép isolation level, timeouts, `trace_id`, `actor_id`, `tenant_id` vào chính câu lệnh `BEGIN` qua Simple Query Protocol multi-statement (`BEGIN; SET LOCAL application_name = ...; SET LOCAL omni.actor_id = ...; ...`) — chi phí round-trip tăng thêm bằng 0.
    - Retry Policy cho serialization failure (`SQLSTATE 40001`, `40P01`): bounded exponential backoff (max 3 attempts, 50ms-500ms) chỉ áp dụng cho transaction read-write ngắn, không retry DDL/long-running transactions.
    - Hỗ trợ 4 mức isolation: `ReadCommitted` (mặc định), `RepeatableRead`, `Serializable`, `ReadUncommitted`.
    - Cấu trúc `SessionContext` chứa `Actor` (user_id/service_id/anonymous), `TraceId`, `SpanId`, `tenant_id` để truyền vào database session variables và audit triggers.
  - **SQLSTATE Error Taxonomy & AppError Mapping (`error`):**
    - Phân loại toàn bộ `sqlx::Error` theo PostgreSQL SQLSTATE thành enum `DbError` có ngữ nghĩa: `UniqueViolation`, `ForeignKeyViolation`, `CheckViolation`, `NotNullViolation`, `SerializationFailure`, `DeadlockDetected`, `QueryCanceled`, `ConnectionLost`, `PoolExhausted`, `MigrationFailed`, v.v.
    - Trait `Retryable` phân loại lỗi có thể retry an toàn (serialization failure, deadlock, transient network) vs không retry (constraint violation, syntax error).
    - Ánh xạ `DbError` sang RFC 9457 `AppError` (`omni-errors`): unique violation → `Conflict`, foreign key → `UnprocessableEntity`, pool exhausted → `ServiceUnavailable`, internal errors được sanitized (không lộ constraint name, câu SQL, chi tiết Postgres).
    - Enum `SqlState` chứa 60+ SQLSTATE codes thông dụng (integrity constraint, transaction, connection, syntax, internal errors) với phương thức `retryable()` và `is_constraint_violation()`.
  - **Observability & OpenTelemetry Semantic Conventions (`instrument`, `commenter`):**
    - Extension trait `DbFutureExt` tự động bọc span OTel `db.query` theo Database Semantic Conventions (stable) cho mọi query/execute future: `db.system.name = "postgresql"`, `db.query.text`, `db.operation.name` (SELECT/INSERT/UPDATE/DELETE), `db.collection.name` (table), `db.response.status_code` (ok/error).
    - Slow query log mức `WARN` với `trace_id` khi thời gian thực thi >= `slow_query_threshold_ms` (200ms mặc định).
    - Metrics Prometheus: `db_client_operation_duration_seconds`, `db_client_transaction_duration_seconds`, `db_client_transactions_total`, `db_client_transaction_retries_total`, `db_client_errors_total`, `db_client_connections_usage/min/max`.
    - Sqlcommenter (`commenter.rs`): tùy chọn gắn `/*application='...',traceparent='...'*/` vào cuối câu SQL theo chuẩn Google sqlcommenter (chỉ bật khi `trace_correlation = "statement"`).
    - Correlation qua `application_name`: `SET LOCAL application_name = '<service>:<trace_id>'` trong `BEGIN` statement để liên kết Postgres `pg_stat_activity` với APM traces.
  - **Database Migrations (`migrate`):**
    - Wrapper `run_migrations` cho SQLx migrator hỗ trợ 3 chế độ: `disabled`, `apply` (áp dụng migrations trực tiếp), `verify` (chỉ kiểm tra schema, mặc định cho prod).
    - Kết nối migration riêng: hỗ trợ `migration.host/port/username/password` riêng để kết nối trực tiếp Postgres (bỏ qua PgBouncer) vì advisory lock cần session mode.
    - Timeout riêng: `migration.statement_timeout_ms = 0` (không giới hạn) cho backfill dài, vẫn chịu `activeDeadlineSeconds` của Kubernetes Job.
  - **Entity Auditing & Optimistic Concurrency (`audit`, `id`):**
    - Struct `AuditColumns` chuẩn: `created_at`, `created_by`, `updated_at`, `updated_by`, `deleted_at` (soft delete), `deleted_by`, `version` (optimistic lock).
    - Database triggers tự động đóng dấu audit từ session variables: `omni.actor_id` → `created_by`/`updated_by`/`deleted_by`.
    - Hàm `check_optimistic_lock` kiểm tra version, trả `DbError::OptimisticLockFailure` (ánh xạ sang HTTP 409 Conflict với header `ETag`).
    - Type-safe ID: `Id<T>` wrapper cho `UUIDv7` (RFC 9562) với marker type `T`, thân thiện B-Tree index và có thứ tự thời gian.
  - **Keyset Cursor Pagination (`pagination`):**
    - Hàm `paginate_keyset` cầu nối từ query results sang `CursorPaginationMeta` (`omni-common`), hỗ trợ cursor base64 với format `<sort_key>:<id>`.
    - Tự động sinh `next_cursor` và `prev_cursor`, kiểm tra `has_next_page` qua over-fetch (limit + 1).
  - **Bộ kiểm thử toàn diện:** 8 test suites bao gồm config, error taxonomy, session context, audit, pagination, instrumentation, health sentinel, và live Postgres integration tests.

- Triển khai thư viện bộ nhớ đệm phân tán và kiểm soát đồng thời `omni-cache` (`libs/cache`):
  - **Redis/Valkey Async Multiplexed Backend (`backend`, `client`):**
    - Bộ quản lý kết nối `RedisBackend` sử dụng `redis::aio::ConnectionManager` với tự động kết nối lại (auto-reconnect) khi mất mạng.
    - Hỗ trợ cả Redis Cluster và Sentinel topology qua cấu hình `RedisEndpoint`.
    - `MemoryBackend` cho kiểm thử đơn vị và phát triển cục bộ không cần Redis thật.
    - Trait `CacheBackend` trừu tượng hóa cho phép mở rộng sang các backend khác (Memcached, DragonflyDB).
  - **6-Layer Stampede Protection Architecture (`stampede`):**
    - **Layer 1 - In-Process Sharded Single-Flight (`single_flight.rs`):** Shard theo hash key vào 16 buckets, mỗi bucket một Tokio Mutex độc lập, ngăn chặn thundering herd trong cùng một Pod.
    - **Layer 2 - Probabilistic TTL Jitter (`jitter.rs`):** Thêm jitter ngẫu nhiên 10-20% vào TTL để tránh hàng loạt keys hết hạn đồng loạt.
    - **Layer 3 - XFetch (Vattani VLDB 2015) (`xfetch.rs`):** Cơ chế tái tính sớm xác suất dựa trên delta time và beta factor, giảm cache miss cho high-traffic keys.
    - **Layer 4 - Stale-While-Revalidate / Stale-If-Error (SWR/SIE) (`swr.rs`):** Trả về dữ liệu cũ trong lúc recompute ở background, đảm bảo độ trễ P99 ổn định.
    - **Layer 5 - Distributed Lease (`lease.rs`):** Lua script atomic `WRITE_WITH_LEASE` cấp quyền độc quyền tái tính toán cho một Pod duy nhất.
    - **Layer 6 - Negative Caching (`negative.rs`):** Cache kết quả "không tồn tại" với TTL ngắn để chống cache penetration attacks.
  - **Fail-Closed Distributed Locking (`lock`, `scripts`):**
    - `DistributedLock` với monotonically increasing Fencing Tokens và Watchdog heartbeat tự động gia hạn.
    - Lua scripts atomic (`SCRIPT_ACQUIRE_WITH_FENCE`, `SCRIPT_EXTEND`, `SCRIPT_RELEASE`) đảm bảo correctness ngay cả khi clock skew hoặc network partition.
    - RAII Guard `LockGuard` tự động giải phóng khóa khi drop, ngăn chặn deadlock.
    - Phân loại lý do thất bại (`LockAcquireReason`, `LockLostReason`) để debug và monitoring.
  - **Cluster-Ready Keyspace & Clean Key Grammar (`key`):**
    - Cấu trúc key chuẩn: `{namespace}:{entity}:{id}` với hash tags `{id}` và `{lock-ns}` cho Redis Cluster multi-key operations.
    - HMAC-SHA256 PII masking cho sensitive identifiers (email, phone, SSN) trong cache keys.
    - Key length validation (max 512 bytes) và character whitelist để tránh injection.
  - **Binary Protocol with Compression & Decompression Bomb Protection (`codec`):**
    - Wire format: 4-byte magic header + 1-byte version + 1-byte flags + 2-byte reserved + payload.
    - Tự động nén payload > 1KB bằng `zstd` (compression level 3) để tiết kiệm bandwidth.
    - Pre-allocation check: từ chối giải nén nếu uncompressed size > 16MB để chống decompression bomb DoS.
  - **Kubernetes Health Integration & Slow Command Detection (`health`, `instrument`, `sentinel`):**
    - Triển khai trait `HealthIndicator` (`omni-metrics`) qua `RedisHealthIndicator` với cached PING không gây probe thundering herd.
    - `CacheMetricsRecorder` thu thập: `cache_operation_duration_seconds`, `cache_hits_total`, `cache_misses_total`, `cache_errors_total`, `cache_stampede_prevented_total`.
    - Slow command logging (threshold 100ms mặc định) với `trace_id` correlation.
    - Cache Sentinel tác vụ nền định kỳ kiểm tra kết nối và cập nhật `CacheHealthState`.
  - **Type-Safe Cache Abstraction (`typed`):**
    - `TypedCache<T>` với automatic serialization/deserialization qua `serde_json`.
    - `CachePolicy` builder pattern: TTL, SWR window, negative cache TTL, compression threshold.
    - Fail-open reads: Redis lỗi tự động fallback về loader function và ghi log mức WARN.
  - **Invalidation Replay Buffer (`invalidation`):**
    - `InvalidationReplayBuffer` với bounded ring buffer (capacity 10000) lưu trữ invalidation events.
    - Hỗ trợ pattern matching: exact key, wildcard `*`, và prefix matching cho bulk invalidation.
  - **Bộ kiểm thử toàn diện:**
    - `libs/cache/tests/`: Integration tests với Testcontainers Redis, unit tests cho stampede protection layers, distributed lock correctness, codec round-trip, key grammar validation.

- Bổ sung bộ kiểm thử đơn vị và tích hợp (`response_and_i18n_tests.rs`, `problem_details_tests.rs`, `observability_tests.rs`) cho `omni-common` và `omni-errors`.

### Changed

- Cập nhật `Cargo.toml` cấp workspace, `libs/common/Cargo.toml`, `libs/errors/Cargo.toml`, `libs/metrics/Cargo.toml` và `libs/database/Cargo.toml` để tích hợp các thư viện cơ sở dữ liệu (`sqlx 0.9` với PostgreSQL runtime `tokio-rustls`, `json`), metrics (`metrics`, `metrics-exporter-prometheus`), đa ngôn ngữ (`fluent-bundle`, `intl-memoizer`, `unic-langid`) và nâng `rust-version = "1.94"`.
- Bổ sung các khóa thông điệp kiểm tra ràng buộc dữ liệu (`NUMBER_MIN_VALUE`, `INVALID_POSTAL_CODE`) vào bộ tài nguyên Fluent `locales/{vi-VN,en-US,ja-JP}/errors.ftl`.
- Mở rộng cấu hình cơ sở dữ liệu `omni-common::config::database` và hệ thống guardrails bảo mật (`CFG_DB_TIMEOUT_BUDGET`, `CFG_DB_LIFETIME_ORDER`, `CFG_DB_STATEMENT_TIMEOUT_REQUIRED`, `CFG_PROD_DB_TLS_REQUIRED`, `CFG_PROD_DB_MIGRATION_MODE`, `CFG_DB_APPLICATION_NAME`).
- Tối ưu hóa cấu hình connection pool trong `config/prod/default.yaml` (`max_connections: 15`, `min_connections: 2`) cho môi trường multi-pod.
- Bổ sung bộ buckets chuẩn `DB_DURATION_BUCKETS` và đăng ký histogram matcher prefix `db_client_` trong `omni-metrics`.
- Cập nhật `omni-common::middleware::is_health_probe_path` để bao gồm `/startupz`, đảm bảo cả 3 đầu dò Kubernetes đều được miễn trừ khỏi APM Root Span và Access Log.
- Cập nhật `omni-errors::ApiErrorResponse::into_response()` để đính kèm `ErrorCodeExtension` vào `Response::extensions()` phục vụ Zero-Copy error attribution trong `omni-metrics`.
- Cập nhật `Cargo.toml` workspace và `libs/cache/Cargo.toml` để tích hợp Redis client (`redis 0.27` với `tokio-comp`, `connection-manager`), compression (`zstd`), crypto (`hmac`, `sha2`).
- Mở rộng `omni-common::config` với `CacheSettings` và `RedisEndpoint` configuration.
- Bổ sung bộ buckets chuẩn `CACHE_DURATION_BUCKETS` (9 mốc từ 1ms đến 5s) trong `omni-metrics`.
- Đồng bộ `workspace-hack/Cargo.toml` qua `cargo hakari generate` sau khi thêm dependencies mới.

### Security

- **Bảo vệ dữ liệu cá nhân (PII) & Phòng chống Reflected XSS (`InvalidParamError`):** Thực thi chính sách _Secure-by-Default_ ẩn trường `rejected_value` (`None`) khỏi JSON response đối với toàn bộ chuỗi nhập tự do và dữ liệu PII; chỉ cho phép phản hồi các giá trị hằng số enum hệ thống không chứa PII thông qua `with_rejected_enum_value`.
- **Ngăn chặn rò rỉ thông tin hạ tầng (`ApiErrorResponse::internal_server_error`, `DbError`, & `AppError::Internal`):** Tự động lọc bỏ chi tiết lỗi nhạy cảm (truy vấn SQL, thông tin kết nối cơ sở dữ liệu, stack trace) khỏi payload HTTP `500 Internal Server Error` trả về cho Client, chỉ ghi log nội bộ qua `tracing::error!` kèm `request_id` và `trace_id`. Toàn bộ thông tin đăng nhập PostgreSQL được bọc bằng `secrecy::SecretString` và hàm `redacted_url()`.
- **Phòng chống SQL Injection:** Tận dụng `SqlSafeStr` của SQLx 0.9 bảo vệ dynamic SQL, hàm `quote_pg_literal` và percent-encoding bảo vệ SQL session variable injection trong `SET LOCAL`.
- **Bảo vệ Cardinality Explosion DoS:** Cơ chế `sanitize_uri_path` tự động gom các request quét URL ngẫu nhiên (404 Scanning) về nhãn cố định `"__unmatched__"`, ngăn chặn tấn công làm bùng nổ số chiều Prometheus dẫn đến OOM máy chủ monitoring.

- **PII Protection in Cache Keys:** HMAC-SHA256 masking cho sensitive identifiers, không bao giờ lưu raw PII vào Redis keys.
- **Decompression Bomb Protection:** Pre-allocation check từ chối giải nén payload > 16MB để chống DoS attacks.
- **Lock Safety with Fencing Tokens:** Monotonically increasing tokens ngăn chặn race condition khi lock holder bị timeout nhưng vẫn thực thi critical section.

---

## [0.1.0] - 2026-10-29

### Added

- Khởi tạo kiến trúc workspace mono-repo cho hệ thống OmniKubernetes (Rust edition 2024).
- Cấu hình khung thư mục cho 8 shared libraries (`omni-common`, `omni-errors`, `omni-logging`, `omni-auth`, `omni-database`, `omni-cache`, `omni-messaging`, `omni-metrics`) và 7 microservices (`gateway`, `auth`, `user`, `product`, `order`, `payment`, `notification`).
- Thiết lập bộ tài nguyên đa ngôn ngữ khởi tạo (`common.ftl`, `errors.ftl`, `messages.ftl`) cho 3 ngôn ngữ `vi-VN`, `en-US`, `ja-JP`.
- Cấu hình tiêu chuẩn CI/CD, Git hooks, linter và formatter (`rustfmt.toml`, `.prettierrc`, `.markdownlint.json`).

<!-- Link so sánh Git Tags -->

[Unreleased]: https://github.com/NQHxDev/OmniKubernetes/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/NQHxDev/OmniKubernetes/releases/tag/v0.1.0
