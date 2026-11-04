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

_Chưa có thay đổi mới. Tất cả các thành phần nền tảng đã hoàn thiện._

---

## [0.2.0] - 2026-11-03

### Milestone: Platform Foundation Complete (Hoàn thành Nền tảng Basecode)

Phiên bản này đánh dấu **hoàn tất 100% tầng nền tảng dùng chung** (Platform Foundation) theo triết lý **"Foundation-First Architecture"**. Toàn bộ 10 bước xây dựng thư viện chia sẻ (`libs/*`) và khung scaffold service đã được nghiệm thu, sẵn sàng cho giai đoạn triển khai các Domain Microservices (`services/*`).

### Added

#### **Bước 11: Service Scaffold & Development Harness - [HOÀN THÀNH]**

- **Common Service Bootstrap Framework (`libs/common/src/bootstrap/`):**
  - Triển khai `AppBuilder` pattern chuẩn enterprise để lắp ghép microservice từ các thư viện nền tảng với thứ tự middleware đúng chuẩn: Tracing Context → Observability/Metrics → Resilience/Timeout → CORS/Security Headers → Auth Extractor → Business Routes → Error Handler.
  - Hỗ trợ tự động mount K8s probes (`/livez`, `/readyz`, `/startupz`), metrics endpoint (`/metrics`), và OpenAPI docs (`/v3/api-docs`).
  - Builder API kiểu tĩnh với generics cho AppState, tích hợp graceful shutdown coordinator.

- **Local Multi-Service Development Environment:**
  - `process-compose.yaml`: Orchestration stack cục bộ với PostgreSQL 16, Redis 7, RabbitMQ 3, Grafana (Loki + Tempo + Prometheus), và các microservices backend.
  - `infra/docker/`: Docker Compose configurations cho từng thành phần hạ tầng (database, cache, message broker, observability).
  - Environment hot-reload và health check tự động cho từng service.

- **Integration Testing Harness:**
  - `tests/integration/`: Framework tích hợp Testcontainers (PostgreSQL, Redis) để chạy toàn bộ test suite cô lập không cần hạ tầng thật.
  - Helper utilities cho setup/teardown database migrations, seed data, và assertion helpers.
  - Parallel test execution với isolated database schemas per test.

#### **Bước 1-10: Consolidated Platform Libraries - [ĐÃ HOÀN THÀNH TRƯỚC ĐÓ]**

- **Bước 1: Base Response & i18n Protocol (`omni-common`, `omni-errors`)**
  - Data contracts `ApiResponse<T>`, `ResponseMeta` với zero-allocation serialization.
  - Phân trang kép: `OffsetPaginationMeta` & `CursorPaginationMeta`.
  - Đa ngôn ngữ Mozilla Fluent lock-free (vi-VN, en-US, ja-JP) với server-side translation.
  - RFC 9457 Problem Details (`application/problem+json`) chuẩn quốc tế.

- **Bước 2: Distributed Context & Structured Logging (`omni-logging`)**
  - Request ID (UUIDv7) và W3C `traceparent` (32-hex trace_id) tự động.
  - JSON structured logging (ECS/OTel standard) với PII sanitizer tuân thủ Nghị định 13/2023/NĐ-CP & GDPR.
  - Non-blocking log appender tách biệt khỏi worker threads.
  - OpenTelemetry gRPC/OTLP bridge xuất sang Grafana Tempo.

- **Bước 3: Type-Safe Configuration (`omni-common`)**
  - Hierarchical config loading 5 tầng: Safe Defaults → `config/base.yaml` → `config/{env}/default.yaml` → Environment vars (`OMNI__*`) → Programmatic overrides.
  - Fail-Fast validation 2 pha (field-level + cross-field invariants).
  - Zero-Leak `SecretString` với auto-zeroize và redacted debug output.

- **Bước 4: Metrics Telemetry & K8s Probes (`omni-metrics`)**
  - RED Middleware (Rate, Errors, Duration) + In-Flight requests cho Axum.
  - Cardinality explosion protection qua URI path sanitization.
  - Kubernetes Tri-Probe (`/livez`, `/readyz`, `/startupz`) + Graceful Shutdown Drain Gate.
  - Tokio Runtime metrics, Watchdog heartbeat, Linux `/proc/self` telemetry (unsafe-free).

- **Bước 5: Relational Persistence (`omni-database`)**
  - Enterprise PgPool factory với bounded backoff retry, 3-tier keepalive ping, graceful shutdown.
  - Zero-Round-Trip Unit of Work: ghép `trace_id`, `actor_id`, `tenant_id` vào câu lệnh `BEGIN`.
  - SQLSTATE error taxonomy sang RFC 9457 `AppError` không lộ internal structure.
  - Entity auditing (`AuditColumns`), `Id<T>` UUIDv7 kiểu tĩnh, Optimistic Locking.
  - Keyset cursor pagination bridge sang `CursorPaginationMeta`.

- **Bước 6: Distributed Caching & Concurrency (`omni-cache`)**
  - Redis/Valkey async multiplexed backend với auto-reconnect.
  - 6-layer stampede protection: Single-Flight, TTL Jitter, XFetch, SWR/SIE, Distributed Lease, Negative Caching.
  - Fail-closed distributed locking với fencing tokens & watchdog.
  - HMAC-SHA256 PII masking, cluster-ready keyspace với hash tags.
  - Binary protocol với zstd compression & decompression bomb protection.

- **Bước 7: Identity & Security Context (`omni-auth`)**
  - JWT engine đa thuật toán (HS256/384/512, RS256, EdDSA, ES256/384/512).
  - `AuthUserClaims` type-safe (UUIDv7 sub/session/tenant, RBAC roles, permissions).
  - Argon2id password hashing OWASP constant-time.
  - Real-time JTI Blacklist + Refresh Token Rotation với theft detection.
  - Axum extractors (`AuthenticatedUser`, `OptionalUser`, `CurrentTenant`) & Guards.
  - Gateway Signed Context Protocol chống header spoofing trong K8s.

- **Bước 8: Event-Driven Messaging & Outbox (`omni-messaging`)**
  - CNCF CloudEvents v1.0.2 envelope (Structured & Binary Content Modes).
  - Transactional Outbox Pattern liên kết `UnitOfWork` ACID.
  - Hybrid Outbox Relay: PostgreSQL `LISTEN/NOTIFY` + `SELECT ... FOR UPDATE SKIP LOCKED`.
  - Dual-Layer Idempotent Inbox (Redis fast-path + PostgreSQL strong-consistency).
  - Resilient Consumer với Exponential Backoff, Poison Pill Isolation, DLQ.
  - Pluggable brokers: In-Memory, RabbitMQ, Kafka adapter, NATS JetStream adapter.

- **Bước 9: Resilience & Traffic Control (`omni-common`, `omni-cache`)**
  - Dual-Layer Rate Limiting: GCRA in-memory (<50ns) + Redis Sliding Window distributed.
  - Adaptive Load Shedding theo Little's Law với Tokio Semaphore.
  - Multi-Tier Timeout & Future cancellation propagation (`CancellationToken`).
  - Enterprise 3-State Circuit Breaker (`Closed` ↔ `Open` ↔ `HalfOpen`).
  - AWS Full Jitter Exponential Backoff, Idempotency Gating, Token Bucket `RetryBudget` ≤20%.
  - 7-Stage Zero-Downtime K8s Graceful Shutdown Coordinator.

- **Bước 10: API Documentation Aggregation (`omni-common`, `infra/kubernetes`)**
  - Canonical security scheme `BearerAuth` (HTTP Bearer JWT) với `persistAuthorization: true`.
  - Common schemas registration: `ResponseMeta`, pagination schemas, RFC 9457 error schemas.
  - Dynamic Forwarded-Prefix Awareness tự động cập nhật Server Base URL từ `X-Forwarded-Prefix`.
  - Gateway Centralized Aggregator Catalog (`SwaggerConfig`) và router `/swagger-config.json`.
  - Kubernetes manifests: `swagger-ui.yaml`, decoupled ingress per service, single ingress preserve prefix.

### Changed

- **Workspace Dependencies:** Nâng cấp lên Rust 2024 Edition với `rust-version = "1.85"`.
- **Configuration Structure:** Chuẩn hóa toàn bộ config structs trong `omni-common::config` để dùng chung cho mọi service.
- **Metrics Buckets:** Định nghĩa các buckets chuẩn `HTTP_DURATION_BUCKETS`, `DB_DURATION_BUCKETS`, `CACHE_DURATION_BUCKETS` trong `omni-metrics`.
- **Error Taxonomy:** Hoàn thiện bảng phân loại lỗi cross-crate: `omni-errors::AppError` ← `omni-database::DbError`, `omni-cache::CacheError`, `omni-auth::AuthError`, `omni-messaging::MessagingError`.

### Security

- **PII Protection:** Bộ lọc PII tự động trong logs (`omni-logging`), cache keys (`omni-cache` HMAC-SHA256), và error responses (`omni-errors`).
- **Secret Management:** Zero-leak `SecretString` wrapper cho passwords, JWT keys, database URLs trong `omni-common::config`.
- **SQL Injection Prevention:** SQLx `SqlSafeStr`, `quote_pg_literal`, percent-encoding cho session variables trong `omni-database`.
- **Cardinality DoS Protection:** URI path sanitization trong `omni-metrics` chống bùng nổ Prometheus metrics.
- **Decompression Bomb Protection:** Pre-allocation check từ chối payload >16MB trong `omni-cache::codec`.
- **Lock Safety:** Fencing tokens monotonic trong `omni-cache::lock` chống race condition.
- **Gateway Security:** HMAC-SHA256 signed context protocol trong `omni-auth::middleware` chống header spoofing.

### Performance

- **Zero-Allocation Paths:** Sử dụng `bytes::Bytes`, `Arc`, và `Cow` trong hot paths (`ApiResponse`, `RequestContext`).
- **Lock-Free Algorithms:** GCRA rate limiter với `AtomicU64` CAS loop (<50ns per check).
- **Connection Pooling:** SQLx PgPool với jittered `max_lifetime`, Redis `ConnectionManager` auto-reconnect.
- **Non-Blocking I/O:** Structured logging qua `tracing-appender`, Argon2id password hashing qua `spawn_blocking`.
- **Compression:** Zstd level 3 cho cache payloads >1KB tiết kiệm bandwidth.

### Documentation

- **Architecture Specs:** 10 tài liệu kiến trúc chi tiết trong `docs/architecture/`:
  - `base-response-architecture.md`
  - `configuration-management-architecture.md`
  - `metrics-probes-telemetry-architecture.md`
  - `relational-persistence-architecture.md`
  - `distributed-caching-concurrency-architecture.md`
  - `identity-security-context-architecture.md`
  - `event-driven-messaging-outbox-architecture.md`
  - `resilience-traffic-control-architecture.md`
  - `api-documentation-architecture.md`
  - `service-scaffold-dev-harness-architecture.md`

- **Agent Guidelines:** `AGENTS.md` và `.agents/rules/` (quality-gate, codegraph) cho AI-assisted development.

### Testing

- **Total Test Coverage:** 100+ test cases across 8 crates:
  - `omni-common`: 25 tests (response, i18n, config, resilience, openapi)
  - `omni-errors`: 8 tests (problem details, error mapping)
  - `omni-logging`: 5 tests (structured logging, PII sanitizer)
  - `omni-metrics`: 15 tests (RED middleware, probes, sanitizer, system metrics)
  - `omni-database`: 20 tests (pool, transaction, error taxonomy, pagination, auditing)
  - `omni-cache`: 18 tests (stampede layers, lock, codec, key grammar)
  - `omni-auth`: 12 tests (JWT, password, extractors, guards, revocation)
  - `omni-messaging`: 23 tests (CloudEvents, outbox, inbox, consumer, broker)

- **Integration Tests:** Testcontainers-based với PostgreSQL, Redis cô lập per test suite.

---

## [0.1.0] - 2026-10-29

### Added

- Khởi tạo kiến trúc workspace mono-repo cho hệ thống OmniKubernetes (Rust edition 2024).
- Cấu hình khung thư mục cho 8 shared libraries (`omni-common`, `omni-errors`, `omni-logging`, `omni-auth`, `omni-database`, `omni-cache`, `omni-messaging`, `omni-metrics`) và 7 microservices (`gateway`, `auth`, `user`, `product`, `order`, `payment`, `notification`).
- Thiết lập bộ tài nguyên đa ngôn ngữ khởi tạo (`common.ftl`, `errors.ftl`, `messages.ftl`) cho 3 ngôn ngữ `vi-VN`, `en-US`, `ja-JP`.
- Cấu hình tiêu chuẩn CI/CD, Git hooks, linter và formatter (`rustfmt.toml`, `.prettierrc`, `.markdownlint.json`).

<!-- Link so sánh Git Tags -->

[Unreleased]: https://github.com/NQHxDev/OmniKubernetes/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/NQHxDev/OmniKubernetes/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/NQHxDev/OmniKubernetes/releases/tag/v0.1.0
