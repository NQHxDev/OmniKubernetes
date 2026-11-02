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

- Đặc tả kiến trúc hệ thống trong `docs/architecture/`:
  - Chuẩn giao thức phản hồi và đa ngôn ngữ cấp doanh nghiệp (`base-response-architecture.md`).
  - Chuẩn tài liệu hóa API và tổng hợp OpenAPI tập trung tại Gateway (`api-documentation-architecture.md`).
  - Chuẩn quản lý cấu hình phân cấp kiểu tĩnh an toàn (`configuration-management-architecture.md`).
  - Chuẩn Metrics Telemetry, RED Middleware & Kubernetes Probes (`metrics-probes-telemetry-architecture.md`).
  - Chuẩn kiến trúc lưu trữ quan hệ, Transaction & Resilience cho PostgreSQL (`relational-persistence-architecture.md`).
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

- Bổ sung bộ kiểm thử đơn vị và tích hợp (`response_and_i18n_tests.rs`, `problem_details_tests.rs`, `observability_tests.rs`) cho `omni-common` và `omni-errors`.

### Changed

- Cập nhật `Cargo.toml` cấp workspace, `libs/common/Cargo.toml`, `libs/errors/Cargo.toml`, `libs/metrics/Cargo.toml` và `libs/database/Cargo.toml` để tích hợp các thư viện cơ sở dữ liệu (`sqlx 0.9` với PostgreSQL runtime `tokio-rustls`, `json`), metrics (`metrics`, `metrics-exporter-prometheus`), đa ngôn ngữ (`fluent-bundle`, `intl-memoizer`, `unic-langid`) và nâng `rust-version = "1.94"`.
- Bổ sung các khóa thông điệp kiểm tra ràng buộc dữ liệu (`NUMBER_MIN_VALUE`, `INVALID_POSTAL_CODE`) vào bộ tài nguyên Fluent `locales/{vi-VN,en-US,ja-JP}/errors.ftl`.
- Mở rộng cấu hình cơ sở dữ liệu `omni-common::config::database` và hệ thống guardrails bảo mật (`CFG_DB_TIMEOUT_BUDGET`, `CFG_DB_LIFETIME_ORDER`, `CFG_DB_STATEMENT_TIMEOUT_REQUIRED`, `CFG_PROD_DB_TLS_REQUIRED`, `CFG_PROD_DB_MIGRATION_MODE`, `CFG_DB_APPLICATION_NAME`).
- Tối ưu hóa cấu hình connection pool trong `config/prod/default.yaml` (`max_connections: 15`, `min_connections: 2`) cho môi trường multi-pod.
- Bổ sung bộ buckets chuẩn `DB_DURATION_BUCKETS` và đăng ký histogram matcher prefix `db_client_` trong `omni-metrics`.
- Cập nhật `omni-common::middleware::is_health_probe_path` để bao gồm `/startupz`, đảm bảo cả 3 đầu dò Kubernetes đều được miễn trừ khỏi APM Root Span và Access Log.
- Cập nhật `omni-errors::ApiErrorResponse::into_response()` để đính kèm `ErrorCodeExtension` vào `Response::extensions()` phục vụ Zero-Copy error attribution trong `omni-metrics`.
- Đồng bộ `workspace-hack/Cargo.toml` qua `cargo hakari generate` sau khi thêm dependencies mới.

### Security

- **Bảo vệ dữ liệu cá nhân (PII) & Phòng chống Reflected XSS (`InvalidParamError`):** Thực thi chính sách _Secure-by-Default_ ẩn trường `rejected_value` (`None`) khỏi JSON response đối với toàn bộ chuỗi nhập tự do và dữ liệu PII; chỉ cho phép phản hồi các giá trị hằng số enum hệ thống không chứa PII thông qua `with_rejected_enum_value`.
- **Ngăn chặn rò rỉ thông tin hạ tầng (`ApiErrorResponse::internal_server_error`, `DbError`, & `AppError::Internal`):** Tự động lọc bỏ chi tiết lỗi nhạy cảm (truy vấn SQL, thông tin kết nối cơ sở dữ liệu, stack trace) khỏi payload HTTP `500 Internal Server Error` trả về cho Client, chỉ ghi log nội bộ qua `tracing::error!` kèm `request_id` và `trace_id`. Toàn bộ thông tin đăng nhập PostgreSQL được bọc bằng `secrecy::SecretString` và hàm `redacted_url()`.
- **Phòng chống SQL Injection:** Tận dụng `SqlSafeStr` của SQLx 0.9 bảo vệ dynamic SQL, hàm `quote_pg_literal` và percent-encoding bảo vệ SQL session variable injection trong `SET LOCAL`.
- **Bảo vệ Cardinality Explosion DoS:** Cơ chế `sanitize_uri_path` tự động gom các request quét URL ngẫu nhiên (404 Scanning) về nhãn cố định `"__unmatched__"`, ngăn chặn tấn công làm bùng nổ số chiều Prometheus dẫn đến OOM máy chủ monitoring.

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
