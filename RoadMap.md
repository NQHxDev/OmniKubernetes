# OmniKubernetes Engineering Roadmap

- **Document Version:** `2.1.0`
- **Target Platform:** Rust (2024 Edition, Rust 1.85+ / 1.94), Tokio Runtime (1.43+), Axum (0.8+), Kubernetes & CNCF Ecosystem
- **Overall Status:**
  - **Phase 1 (Platform Foundation & Shared Libraries):** `[COMPLETED]` (Delivered in `v0.1.0`)
  - **Phase 2 (Domain Microservices Implementation):** `[PLANNED / IN PROGRESS]`
- **Repository:** [OmniKubernetes GitHub](https://github.com/NQHxDev/OmniKubernetes)

---

## Executive Overview

OmniKubernetes adheres to a **"Foundation-First Architecture"**. With the platform foundation, shared libraries (`libs/*`), local multi-service orchestration harness, and integration test frameworks fully completed and verified in release `v0.1.0`, the system is now entering **Phase 2: Domain Microservices Implementation**.

---

## Phase 1: Platform Foundation (Completed)

> For full technical details, refer to [`CHANGELOG.md`](./CHANGELOG.md) and [`docs/architecture/`](./docs/architecture/).

| Component            |  Status  | Description                                                                    |
| :------------------- | :------: | :----------------------------------------------------------------------------- |
| **`omni-common`**    | `[DONE]` | Base response contract, Fluent i18n, 5-layer config, resilience suite, OpenAPI |
| **`omni-errors`**    | `[DONE]` | RFC 9457 Problem Details (`application/problem+json`), RFC 6901 JSON pointer   |
| **`omni-logging`**   | `[DONE]` | ECS/OTel structured logging, W3C traceparent, PII sanitizer, Tempo exporter    |
| **`omni-metrics`**   | `[DONE]` | Axum RED middleware, zero-regex URI sanitization, K8s Tri-Probe, Tokio metrics |
| **`omni-database`**  | `[DONE]` | SQLx PgPool factory, zero-round-trip Unit of Work, auditing, keyset pagination |
| **`omni-cache`**     | `[DONE]` | Redis multiplexed manager, 6-layer stampede protection, fail-closed Redlock    |
| **`omni-auth`**      | `[DONE]` | Multi-algo JWT, Argon2id, JTI blacklist revocation, RTR, signed context        |
| **`omni-messaging`** | `[DONE]` | CloudEvents v1.0.2, transactional outbox & relay, idempotent inbox, DLQ        |
| **Dev Harness**      | `[DONE]` | `AppBuilder`, `process-compose.yaml`, Docker Compose, Testcontainers harness   |

---

## Phase 2: Domain Microservices Implementation (Active Roadmap)

### Milestone 1: API Gateway Service (`services/gateway`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - Dynamic reverse proxy and path-based routing to internal domain services.
  - Gateway Signed Context Protocol injection (HMAC-SHA256 signature) forwarding authenticated caller context.
  - Centralized OpenAPI / Swagger UI aggregator (`/swagger-config.json`) discovering downstream service schemas.
  - Edge rate limiting and client IP extraction behind trusted Kubernetes ingress.

### Milestone 2: Auth & IAM Service (`services/auth`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - User registration, credential authentication, and session issuance.
  - Dual-token lifecycle: Access Token (short-lived) + Refresh Token with Refresh Token Rotation (RTR).
  - Real-time token revocation via Redis JTI blacklist and token family theft detection.
  - Secure password hashing with Argon2id and email-based password reset workflow.

### Milestone 3: User & Organization Service (`services/user`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - User profile management, address book, and localized preferences.
  - Multi-tenant organization boundaries and tenancy isolation.
  - Role-Based Access Control (RBAC) role and permission management.
  - Keyset-paginated search and auditing logs for account mutations.

### Milestone 4: Product Catalog Service (`services/product`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - Product catalog CRUD with category hierarchies, tags, and attribute schemas.
  - Inventory management with optimistic locking to prevent stock overselling.
  - High-performance read path with `omni-cache` stampede protection (XFetch + SWR).
  - Outbox event publication (`product.created`, `product.updated`, `inventory.depleted`).

### Milestone 5: Order Processing Service (`services/order`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - Shopping cart management with distributed Redis persistence.
  - Order checkout lifecycle state machine (`Pending` → `PaymentProcessing` → `Confirmed` → `Completed` / `Cancelled`).
  - Unit of Work transaction linking order creation with Transactional Outbox events.
  - Idempotent event inbox processing incoming payment and inventory signals.

### Milestone 6: Payment & Ledger Service (`services/payment`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - Payment gateway adapter integration (Stripe, VNPay, Sandbox providers).
  - Double-entry transaction ledger tracking debits, credits, and balance holds.
  - Strict idempotency gating (`Idempotency-Key` header) preventing duplicate charges.
  - Webhook ingestion pipeline with cryptographic signature validation.

### Milestone 7: Notification Service (`services/notification`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - CloudEvents event consumer listening to domain topics across RabbitMQ / Kafka.
  - Multi-channel delivery providers: Email (SMTP / SendGrid), SMS, and Push notifications.
  - Template engine with multi-language localization via Mozilla Fluent.
  - Resilient retry pipeline with exponential backoff and Dead Letter Queue (DLQ).

### Milestone 8: Cloud-Native K8s Deployment & GitOps (`infra/kubernetes`)

- **Status:** `[PLANNED]`
- **Target Deliverables:**
  - Production-grade Helm charts for each domain microservice.
  - GitOps delivery configuration with ArgoCD / Flux.
  - End-to-end integration testing suite running against ephemeral Kubernetes test clusters.
  - Centralized Grafana dashboards for RED metrics, distributed traces, and log aggregation.

---

## Quality Gate Checklist

Every milestone deliverable must satisfy the repository Definition of Done before merging:

```bash
# 1. Format code
cargo fmt --all -- --check

# 2. Dependency unification
cargo hakari generate --diff

# 3. Static analysis & linting
cargo clippy --all-targets --all-features -- -D warnings

# 4. Automated test suite
cargo test --all
```

---

_Last Updated: 2026-11-04_  
_Phase: Domain Services Implementation_
