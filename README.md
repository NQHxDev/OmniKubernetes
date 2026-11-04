# OmniKubernetes

[![Rust Edition](https://img.shields.io/badge/Rust-2024%20Edition-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Framework: Axum](https://img.shields.io/badge/Axum-0.8-blueviolet?logo=rust)](https://github.com/tokio-rs/axum)
[![Runtime: Tokio](https://img.shields.io/badge/Tokio-1.43+-blue?logo=tokio)](https://tokio.rs/)
[![Kubernetes](https://img.shields.io/badge/Kubernetes-Cloud--Native-326ce5?logo=kubernetes&logoColor=white)](https://kubernetes.io/)

**OmniKubernetes** is an enterprise-grade, cloud-native microservices platform engineered from the ground up in **Rust** (2024 Edition). Designed according to the **"Foundation-First Architecture"** philosophy, it delivers a unified ecosystem featuring zero-cost abstractions, multi-tenant security boundaries, event-driven architecture, resilient concurrency control, and end-to-end observability for mission-critical deployments on Kubernetes.

---

## Architecture & Repository Layout

The repository is structured as a modular Cargo workspace mono-repo with clean separation of concerns between shared foundation libraries (`libs/*`), domain services (`services/*`), and infrastructure manifests (`infra/*`):

```text
OmniKubernetes/
├── .agents/                 # AI engineering rules and verification protocols
├── config/                  # Hierarchical environment configurations (base, dev, prod)
├── docs/
│   └── architecture/        # Comprehensive architectural specifications
├── infra/
│   ├── docker/              # Multi-container Docker Compose stacks (DB, Cache, Broker, Observability)
│   └── kubernetes/          # Kubernetes manifests, Helm charts, and Ingress routing
├── libs/                    # Shared Platform Foundation Libraries
│   ├── auth/                # omni-auth: Multi-algo JWT, Argon2id, RBAC, Signed Context
│   ├── cache/               # omni-cache: Redis manager, 6-layer stampede protection, Redlock
│   ├── common/              # omni-common: Base response, Fluent i18n, config, resilience, AppBuilder
│   ├── database/            # omni-database: SQLx PgPool, zero-round-trip Unit of Work, migrations
│   ├── errors/              # omni-errors: RFC 9457 Problem Details & error taxonomy
│   ├── logging/             # omni-logging: Structured ECS/OTel JSON logs, PII sanitizer, Tempo
│   ├── messaging/           # omni-messaging: CloudEvents v1.0.2, transactional outbox, inbox, DLQ
│   └── metrics/             # omni-metrics: Prometheus RED metrics, K8s Tri-Probe, runtime telemetry
├── services/                # Domain Microservices
│   ├── gateway/             # API Gateway: Reverse proxy, route dispatching, Swagger aggregator
│   ├── auth/                # Auth Service: Identity management, token issuance, RTR
│   ├── user/                # User Service: Profile management, multi-tenant isolation
│   ├── product/             # Product Service: Catalog, inventory tracking, caching
│   ├── order/               # Order Service: Order lifecycle state machine, checkout, outbox
│   ├── payment/             # Payment Service: Transaction ledger, idempotency gating
│   └── notification/        # Notification Service: Multi-channel dispatch (Email, SMS, Push)
├── tests/
│   └── integration/         # Integration test suite powered by Testcontainers (PostgreSQL, Redis)
├── workspace-hack/          # Dependency unification via Cargo Hakari
├── Cargo.toml               # Workspace manifest
├── process-compose.yaml     # Local multi-service development orchestration
├── CHANGELOG.md             # Project release history (Keep a Changelog standard)
└── RoadMap.md               # Engineering roadmap and milestone tracking
```

---

## Platform Foundation Capabilities

| Capability                        | Library                        | Technical Highlights                                                                                                                                                                                                                                                                                                                |
| :-------------------------------- | :----------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **API Protocol & i18n**           | `omni-common`<br>`omni-errors` | • RFC 9457 Problem Details (`application/problem+json`)<br>• Lock-free Mozilla Fluent localization via RCU (`vi-VN`, `en-US`, `ja-JP`)<br>• Zero-allocation dual pagination (Offset & Keyset Cursor)                                                                                                                                |
| **Distributed Tracing & Logging** | `omni-logging`                 | • Dual correlation: Request ID (UUIDv7) + W3C `traceparent`<br>• Non-blocking ECS/OTel JSON logs via `tracing-appender`<br>• Automated PII masking compliant with GDPR & Vietnam Decree 13/2023/ND-CP<br>• Native OTel gRPC bridge to Grafana Tempo                                                                                 |
| **Type-Safe Configuration**       | `omni-common`                  | • 5-layer hierarchy: Defaults → Base → Environment → Env Vars (`OMNI__*`) → Overrides<br>• 2-phase fail-fast invariant validation<br>• Zero-leak `SecretString` with automatic heap memory zeroization on drop                                                                                                                      |
| **Observability & Health Probes** | `omni-metrics`                 | • Axum RED middleware (Rate, Errors, Duration) and In-Flight tracking<br>• Cardinality explosion protection via zero-regex byte scanning<br>• Kubernetes Tri-Probe suite (`/livez`, `/readyz`, `/startupz`)<br>• Tokio runtime saturation metrics & safe Linux `/proc/self` telemetry                                               |
| **Relational Persistence**        | `omni-database`                | • Enterprise `PgPool` with exponential backoff & keepalive pinging<br>• Zero-round-trip Unit of Work merging tenant, actor & trace IDs into `BEGIN`<br>• Type-safe `Id<T>` UUIDv7 primary keys & optimistic locking (`version`)<br>• 3-mode migration runner (`Disabled`, `Apply`, `Verify`)                                        |
| **Distributed Caching**           | `omni-cache`                   | • Redis/Valkey async multiplexed backend with auto-reconnection<br>• 6-layer cache stampede mitigation (Single-Flight, TTL Jitter, XFetch, SWR, Lease, Negative)<br>• Fail-closed Redlock distributed locking with monotonic fencing tokens<br>• Zstd level 3 binary compression & decompression bomb protection (>16MB)            |
| **Security & Identity**           | `omni-auth`                    | • Multi-algorithm JWT engine (HS256/384/512, RS256, EdDSA, ES256/384/512)<br>• OWASP Argon2id password hashing executed on `spawn_blocking`<br>• Real-time token revocation (Redis JTI blacklist) & Refresh Token Rotation (RTR)<br>• Gateway Signed Context Protocol (HMAC-SHA256) preventing header spoofing                      |
| **Event-Driven Messaging**        | `omni-messaging`               | • CNCF CloudEvents v1.0.2 envelope specification (Structured & Binary)<br>• Transactional Outbox Pattern atomically bound to PostgreSQL transactions<br>• Hybrid Outbox Relay worker (`LISTEN/NOTIFY` + `SELECT ... FOR UPDATE SKIP LOCKED`)<br>• Dual-layer idempotent inbox engine (Redis + PostgreSQL) & Dead Letter Queue (DLQ) |
| **Resilience & Traffic Control**  | `omni-common`                  | • Dual-layer rate limiting: GCRA in-memory (<50ns) + Redis sliding window<br>• Adaptive load shedding derived from Little's Law<br>• Enterprise 3-state circuit breaker (`Closed`, `Open`, `HalfOpen`)<br>• 7-Stage Zero-Downtime Kubernetes Graceful Shutdown Coordinator                                                          |

---

## Architectural Specifications

In-depth technical architecture and design documents are available in the [`docs/architecture/`](./docs/architecture/) directory:

1. [Base Response & i18n Protocol](docs/architecture/base-response-architecture.md)
2. [Configuration Management](docs/architecture/configuration-management-architecture.md)
3. [Distributed Caching & Concurrency Control](docs/architecture/distributed-caching-concurrency-architecture.md)
4. [Event-Driven Messaging & Outbox Pattern](docs/architecture/event-driven-messaging-outbox-architecture.md)
5. [Identity & Security Context](docs/architecture/identity-security-context-architecture.md)
6. [Metrics Telemetry & Kubernetes Probes](docs/architecture/metrics-probes-telemetry-architecture.md)
7. [Observability & Structured Logging](docs/architecture/observability-logging-context-architecture.md)
8. [Relational Persistence Foundation](docs/architecture/relational-persistence-architecture.md)
9. [Resilience & Traffic Control](docs/architecture/resilience-traffic-control-architecture.md)
10. [Service Scaffold & Development Harness](docs/architecture/service-scaffold-dev-harness-architecture.md)

---

## Golden Path: Microservice Bootstrap (< 30 lines)

Using the built-in `AppBuilder` pattern from `omni-common`, creating a production-grade Axum service with complete middleware ordering, probes, and metrics takes less than 30 lines of code:

```rust
use omni_common::bootstrap::AppBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load("order-service")?;
    let _log_guard = omni_logging::init(&config.logging)?;

    let db_pool = omni_database::create_pool(&config.database).await?;
    let cache_manager = omni_cache::create_manager(&config.redis).await?;
    let metrics_handle = omni_metrics::init_prometheus_exporter()?;

    omni_database::run_migrations(&db_pool).await?;

    let app = AppBuilder::new("order-service")
        .with_state(AppState { db_pool, cache_manager })
        .with_observability_middleware()
        .with_metrics_middleware()
        .with_resilience_middleware()
        .with_security_headers()
        .with_k8s_probes()
        .with_metrics_endpoint(metrics_handle)
        .with_openapi_docs(ApiDoc::openapi())
        .mount_api_routes(routes::router())
        .build();

    omni_common::server::serve_with_graceful_shutdown(app, config.server.port).await?;
    Ok(())
}
```

---

## Getting Started

### Prerequisites

- **Rust:** Rust 1.85+ (Rust 2024 Edition recommended / `rust-version = 1.94`)
- **Docker & Docker Compose:** Required for local infrastructure dependencies and Testcontainers
- **Process Compose:** Recommended for local multi-service orchestration (`brew install process-compose` or equivalent)

### Local Development Environment

Start all supporting infrastructure (PostgreSQL 16, Redis 7, RabbitMQ 3, Grafana Observability Stack):

```bash
# Option A: Start unified stack via process-compose
process-compose up

# Option B: Start via Docker Compose
docker compose -f infra/docker/docker-compose.yaml up -d
```

### Verification & Quality Gates

Before submitting code, ensure that all automated quality gate checks pass cleanly:

```bash
# 1. Format code
cargo fmt --all -- --check

# 2. Check dependency unification (if Cargo.toml was touched)
cargo hakari generate --diff

# 3. Static analysis & strict linting (zero warnings permitted)
cargo clippy --all-targets --all-features -- -D warnings

# 4. Run entire test suite (unit + integration tests)
cargo test --all
```

---

## Roadmap & Releases

- **Roadmap:** Track ongoing development milestones in [`RoadMap.md`](./RoadMap.md).
- **Changelog:** Detailed version history is documented in [`CHANGELOG.md`](./CHANGELOG.md).

---

## License

This project is licensed under the [MIT License](LICENSE).
