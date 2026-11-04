# Changelog

All notable changes to the **OmniKubernetes** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## Developer Guide (Recording Rules)

- **During Development:** Record any new changes under the `[Unreleased]` section when merging code/PRs.
- **When Releasing a New Version:**
  1. Rename `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD` (e.g., `[0.1.0] - 2026-11-04`).
  2. Create a fresh empty `## [Unreleased]` block at the top for the next development cycle.
  3. Update git tag comparison links at the bottom of the file.
- **Section Types:**
  - `Added`: New features or API additions.
  - `Changed`: Changes in existing functionality, structure, or logic.
  - `Deprecated`: Soon-to-be removed features.
  - `Removed`: Now removed features.
  - `Fixed`: Bug fixes.
  - `Security`: Vulnerability fixes and security enhancements.

---

## [Unreleased]

_No unreleased changes._

---

## [0.1.0] - 2026-11-04

### Milestone: Platform Foundation & Base Architecture Complete

This inaugural release marks the **100% completion of the shared platform foundation** (Platform Foundation Phase) following the **"Foundation-First Architecture"** philosophy. All 11 platform milestones—encompassing the 8 shared libraries (`libs/*`), the service bootstrap framework (`AppBuilder`), the local multi-service development environment (`infra/docker/`, `process-compose.yaml`), and the integration testing harness (`tests/integration/`)—have been verified and are production-ready for Domain Microservices (`services/*`) development.

### Added

#### **1. Workspace & Development Infrastructure (Mono-repo)**

- Initialized Rust 2024 Edition workspace mono-repo (`rust-version = "1.94"`).
- Configured modular boundaries separating 8 shared libraries (`libs/*`) and 7 domain microservices (`services/*`).
- Configured initial multi-language localization resources (`common.ftl`, `errors.ftl`, `messages.ftl`) for `vi-VN`, `en-US`, and `ja-JP`.
- Enforced project-wide development tooling: CI/CD workflows, Git pre-commit hooks (`.githooks/pre-commit`), Cargo Hakari dependency unification (`workspace-hack`), linter and code formatting configurations (`rustfmt.toml`, `.prettierrc`, `.markdownlint.json`).
- Established strict Quality Gates and synchronized CodeGraph indexing for AI-assisted engineering.

#### **2. Shared Platform Libraries (`libs/*`)**

- **`omni-common` (Base Protocol, Configuration, Resilience & OpenAPI):**
  - Standardized response contracts: `ApiResponse<T>` and `ResponseMeta` with zero-allocation serialization paths (`bytes::Bytes`, `Cow`).
  - Dual pagination mechanisms: `OffsetPaginationMeta` and `CursorPaginationMeta`.
  - Lock-free internationalization powered by Mozilla Fluent via RCU pointer swap (`vi-VN`, `en-US`, `ja-JP`), supporting both server-side translation and client-side dynamic parameters.
  - 5-layer hierarchical configuration engine: Safe Defaults → `config/base.yaml` → `config/{env}/default.yaml` → Environment variables (`OMNI__*`) → Programmatic overrides.
  - 2-phase fail-fast configuration validation (field-level via `validator` + cross-field invariants).
  - Memory-safe `SecretString` wrapper with automatic heap zeroization on drop and redacted debug representations `[REDACTED]`.
  - Resilience suite: Dual-layer rate limiting (Layer 1 GCRA in-memory lock-free <50ns + Layer 2 Redis Cluster sliding window), adaptive load shedding derived from Little's Law using Semaphores, multi-tier timeouts with cancellation propagation (`CancellationToken`), enterprise 3-state circuit breaker (`Closed`, `Open`, `HalfOpen`), and AWS Full Jitter exponential backoff with token bucket retry budget (≤ 20%).
  - OpenAPI 3.1 tooling (`utoipa`): canonical `BearerAuth` security scheme (`persistAuthorization: true`), shared schemas auto-registration, dynamic forwarded-prefix awareness, and Gateway aggregator catalog router `/swagger-config.json`.

- **`omni-errors` (RFC 9457 Problem Details Standardization):**
  - International standard RFC 9457 error representation (`application/problem+json`) via `AppError`, `ApiErrorResponse`, and `InvalidParamError`.
  - Bidirectional JSON Pointer (RFC 6901) field-level error mapping.
  - Centralized cross-crate error taxonomy mapping infrastructure failures (Database, Cache, Auth, Network) to safe HTTP and business error codes without leaking internal infrastructure details.

- **`omni-logging` (Distributed Tracing & Structured Logging):**
  - Automated dual identification: Request ID (UUIDv7) + W3C `traceparent` (32-hex `trace_id`).
  - Structured JSON logging aligned with ECS/OTel standards.
  - Non-blocking log processing using `tracing-appender` decoupled from Tokio worker threads.
  - Automatic PII Sanitizer compliant with Vietnam Decree 13/2023/ND-CP and GDPR.
  - OpenTelemetry gRPC/OTLP bridge exporting distributed traces to Grafana Tempo.
  - Automated performance telemetry emitting `Server-Timing` and `X-Response-Time` headers.

- **`omni-metrics` (Prometheus Telemetry & Kubernetes Probes):**
  - Axum RED middleware (Rate, Errors, Duration) and In-Flight request tracking.
  - Cardinality explosion prevention through a zero-regex byte scanner (`sanitize_uri_path`, `normalize_matched_path`).
  - Zero-copy error classification utilizing `ErrorCodeExtension` integrated directly with `omni-errors`.
  - Kubernetes Tri-Probe suite (`/livez`, `/readyz`, `/startupz`, `/healthz`) coupled with a Graceful Shutdown Drain Gate.
  - Runtime telemetry: Tokio runtime saturation metrics, Watchdog heartbeat, and Linux `/proc/self` process metrics (100% safe Rust).

- **`omni-database` (PostgreSQL Relational Persistence Foundation):**
  - Enterprise `PgPool` factory featuring bounded exponential backoff retries, 3-tier keepalive pinging, and a Pool Sentinel background thread publishing Prometheus metrics.
  - Zero-round-trip Unit of Work (`build_begin_statement`) consolidating `trace_id`, `actor_id`, `tenant_id`, and `statement_timeout` into the initial `BEGIN` query.
  - SQLSTATE error mapping translating PostgreSQL errors into `DbError` and RFC 9457 `AppError`.
  - Entity auditing model (`AuditColumns`, database trigger `omni_set_audit_timestamps`), type-safe UUIDv7 primary keys `Id<T>`, and optimistic locking (`version` column).
  - Keyset cursor pagination bridge (`KeysetCursor`, `paginate_keyset`) directly compatible with `CursorPaginationMeta`.
  - Database migration runner supporting 3 runtime modes: `Disabled`, `Apply`, and `Verify`.

- **`omni-cache` (Distributed Caching & Concurrency Control with Redis/Valkey):**
  - Asynchronous multiplexed backend (`ConnectionManager`) with automatic connection healing.
  - Comprehensive 6-layer cache stampede mitigation: Single-Flight (`Arc<Notify>`), TTL Jitter, XFetch probabilistic early refresh, Stale-While-Revalidate / Stale-If-Error (SWR/SIE), Distributed Lease, and Negative Caching.
  - Fail-closed Redlock distributed locking with monotonic fencing tokens and background lease renewal watchdog.
  - HMAC-SHA256 PII masking across keyspace, with Redis Cluster hash tag compliance.
  - Optimized binary protocol utilizing Zstd level 3 compression and decompression bomb attack mitigation (>16MB limit).
  - High-level `TypedCache<T>` abstraction supporting fail-open cached reads with fallback loaders.

- **`omni-auth` (Identity, Authentication & Security Context):**
  - Multi-algorithm JWT engine (`jsonwebtoken`: HS256/384/512, RS256, EdDSA, ES256/384/512).
  - Strongly-typed `AuthUserClaims` (UUIDv7 `sub`, `session_id`, `tenant_id`, RBAC roles, and fine-grained permissions).
  - OWASP-compliant Argon2id password hashing executed asynchronously via `spawn_blocking`.
  - Real-time token revocation through Redis JTI blacklisting and Refresh Token Rotation (RTR) with token family theft detection.
  - Axum 0.8 extractors (`AuthenticatedUser`, `OptionalUser`, `CurrentTenant`, `BearerToken`) and authorization guards (`RequireRole`, `RequirePermission`).
  - Gateway Signed Context Protocol using HMAC-SHA256 signatures to prevent internal header spoofing across Kubernetes service mesh.
  - Automatic identity context bridging into `RequestContext`, `tracing::Span`, and PostgreSQL `SessionContext`.

- **`omni-messaging` (Event-Driven Messaging & Transactional Outbox Pattern):**
  - Standardized CNCF CloudEvents v1.0.2 envelope (`CloudEvent<T>`, `CloudEventBuilder`, UUIDv7, supporting Structured and Binary content modes).
  - Transactional Outbox Pattern atomically bound to PostgreSQL local transactions via `UnitOfWork`.
  - Hybrid Outbox Relay worker combining real-time PostgreSQL `LISTEN/NOTIFY` signaling with micro-batched `SELECT ... FOR UPDATE SKIP LOCKED` polling.
  - Dual-layer idempotent inbox engine (Redis fast-path `SET NX EX` + PostgreSQL strong consistency table).
  - Resilient consumer execution pipeline equipped with Concurrency Controller, Exponential Backoff with Full Jitter, Poison Pill Isolation, and Dead Letter Queue (DLQ).
  - End-to-end W3C distributed tracing context propagation across message brokers.
  - Pluggable broker drivers: In-Memory (Test/CI), RabbitMQ AMQP 0-9-1 (`lapin`), Kafka adapter, and NATS JetStream adapter.
  - Automated background housekeeping worker pruning processed outbox/inbox records to prevent table bloat.

#### **3. Service Scaffold & Development Harness**

- **Common Service Bootstrap Framework (`omni-common::bootstrap`):**
  - Enterprise `AppBuilder` pattern enabling microservice composition in under 30 lines of code with strictly validated middleware ordering: Tracing Context → Observability/Metrics → Resilience/Timeout → CORS/Security Headers → Auth Extractor → Business Routes → Error Handler.
  - Integrated 7-Stage Zero-Downtime Kubernetes Graceful Shutdown Coordinator coordinating readiness probe draining (`/readyz`), quiescence delay, worker teardown, and infrastructure draining.
  - Automated route mounting for Kubernetes probes (`/livez`, `/readyz`, `/startupz`), Prometheus metrics (`/metrics`), and OpenAPI specifications (`/v3/api-docs`).

- **Local Multi-Service Development Environment:**
  - Local container orchestration stack powered by `process-compose.yaml` and Docker Compose (`infra/docker/`), managing PostgreSQL 16, Redis 7, RabbitMQ 3, and Grafana Observability (Loki + Tempo + Prometheus).
  - Complete Kubernetes manifests (`infra/kubernetes/`) for centralized Swagger UI aggregation and Ingress path routing.

- **Integration Testing Harness:**
  - Robust integration testing harness in `tests/integration/` utilizing Testcontainers for isolated, ephemeral PostgreSQL and Redis instances per test suite.
  - Database test utilities providing automatic migration execution, fixture seeding, and custom assertions.

### Changed

- **Workspace Dependencies:** Unified entire workspace to Rust 2024 Edition with `rust-version = "1.94"`.
- **Configuration Structure:** Consolidated configuration structs in `omni-common::config` into a reusable standard across all microservices.
- **Metrics Buckets:** Standardized Prometheus histogram bucket presets: `HTTP_DURATION_BUCKETS`, `DB_DURATION_BUCKETS`, and `CACHE_DURATION_BUCKETS`.
- **Error Taxonomy:** Finalized cross-crate error mapping taxonomy into RFC 9457 `AppError` (`omni-errors::AppError` ← `omni-database::DbError`, `omni-cache::CacheError`, `omni-auth::AuthError`, `omni-messaging::MessagingError`).

### Security

- **PII Protection:** Automated PII masking in structured logs (`omni-logging`), cache keys (`omni-cache` HMAC-SHA256), and error responses (`omni-errors`) complying with Vietnam Decree 13/2023/ND-CP and GDPR.
- **Secret Management:** Memory-safe `SecretString` wrapper preventing secret leaks via heap auto-zeroization and redacted debug formatting.
- **SQL Injection Prevention:** Enforced SQLx parameterized bindings, `SqlSafeStr`, and percent-encoded session parameters in `omni-database`.
- **Cardinality DoS Protection:** Path sanitization and URI normalization before recording Prometheus metrics to protect memory against high-cardinality attacks.
- **Decompression Bomb Protection:** Pre-allocation checks rejecting uncompressed payloads exceeding 16MB in `omni-cache::codec`.
- **Distributed Lock Safety:** Monotonic fencing tokens in `omni-cache::lock` to eliminate race conditions across distributed nodes.
- **Gateway Security:** Cryptographic HMAC-SHA256 signed identity headers preventing header spoofing between API Gateway and downstream microservices.

### Performance

- **Zero-Allocation Hot Paths:** Leveraged `bytes::Bytes`, `Arc`, and `Cow` across hot HTTP paths (`ApiResponse`, `RequestContext`).
- **Lock-Free Concurrency:** Implemented GCRA rate limiter with `AtomicU64` CAS loop completing rate checks in <50ns.
- **Optimized Connection Pooling:** SQLx `PgPool` with jittered connection lifespans and Redis `ConnectionManager` with multiplexed auto-reconnecting pipelining.
- **Non-Blocking I/O:** Asynchronous logging via dedicated `tracing-appender` threads; expensive Argon2id hashing offloaded to dedicated blocking threads via `spawn_blocking`.
- **Payload Compression:** Zstd level 3 compression applied to cache payloads larger than 1KB.

### Documentation

- **Architecture Specifications:** 10 comprehensive architectural design documents published in `docs/architecture/`:
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
- **Developer & Agent Guidelines:** Published `AGENTS.md`, crate-level READMEs, and quality gate specifications in `.agents/rules/` (`quality-gate.md`, `codegraph.md`).

### Testing

- **Comprehensive Test Suite:** 126+ test cases (104 unit tests + 22 integration tests with Testcontainers) validating all 8 foundation crates:
  - `omni-common`: 25 tests (response, i18n, config, resilience, openapi)
  - `omni-errors`: 8 tests (problem details, error mapping)
  - `omni-logging`: 5 tests (structured logging, PII sanitizer)
  - `omni-metrics`: 15 tests (RED middleware, probes, sanitizer, system metrics)
  - `omni-database`: 20 tests (pool, transaction, error taxonomy, pagination, auditing)
  - `omni-cache`: 18 tests (stampede layers, lock, codec, key grammar)
  - `omni-auth`: 12 tests (JWT, password, extractors, guards, revocation)
  - `omni-messaging`: 23 tests (CloudEvents, outbox, inbox, consumer, broker)
- **Quality Gate Compliance:** 100% adherence to verification pipelines: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all`.

---

<!-- Git Tag Comparison Links -->

[Unreleased]: https://github.com/NQHxDev/OmniKubernetes/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/NQHxDev/OmniKubernetes/releases/tag/v0.1.0
