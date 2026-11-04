# OmniKubernetes Architecture Specification: Event-Driven Messaging & Transactional Outbox Pattern (`omni-messaging`)

- **Document Version:** `1.0.0`
- **Status:** Draft for Review / Enterprise Architecture Standard
- **Author:** System Architecture Team
- **Roadmap Step:** Giai đoạn 3 — Bước 6: Kiến trúc Giao tiếp Hướng sự kiện & Khung gửi tin cậy ([`RoadMap.md`](../../RoadMap.md))
- **Target Platform:** Rust (2024 Edition, **Rust 1.94+**), Tokio Runtime (1.43+), CNCF CloudEvents (v1.0.2), SQLx 0.9+ (PostgreSQL 16+), Redis OSS 7.2+ / Valkey 8+ (`omni-cache`), Message Brokers (RabbitMQ AMQP 0-9-1, Apache Kafka / Redpanda, NATS JetStream, In-Memory Channel), Kubernetes & CNCF Ecosystem.

---

## 1. Executive Summary & Core Principles

Sau khi hoàn tất nền tảng bảo mật và nhận dạng truy cập (`omni-auth`), hệ sinh thái **OmniKubernetes** bước vào trụ cột tiếp theo của **Giai đoạn 3: Bảo mật & Truyền thông Bất đồng bộ**: **Kiến trúc Giao tiếp Hướng sự kiện & Khung gửi tin cậy (Event-Driven Messaging & Transactional Outbox Pattern - `omni-messaging`)**.

Trong kiến trúc Enterprise Microservices trên Kubernetes, việc các service trao đổi dữ liệu thông qua các cuộc gọi HTTP/gRPC đồng bộ (Synchronous RPC) dẫn tới nhiều hệ lụy nghiêm trọng:

1. **Khớp nối chặt chẽ (Tight Temporal Coupling):** Khi Service A gọi Service B qua HTTP, tính khả dụng của Service A phụ thuộc trực tiếp vào Service B. Nếu Service B bị chậm hoặc ngừng hoạt động, lỗi sẽ lan truyền dây chuyền (Cascading Failure), làm sụp đổ toàn bộ hệ thống.
2. **Nghịch lý Ghi Kép (The Dual-Write Dilemma):** Một nghiệp vụ kinh điển (ví dụ: tạo đơn hàng `OrderCreated`) đòi hỏi service vừa phải cập nhật cơ sở dữ liệu quan hệ (PostgreSQL), vừa phải phát hành một sự kiện thông báo ra Message Broker (Kafka/RabbitMQ).
   - Nếu ghi DB thành công nhưng phát hành ra Broker thất bại (do rớt mạng, broker restart): Dữ liệu downstream không được kích hoạt, dẫn đến đơn hàng bị treo (Lost Event).
   - Nếu phát hành ra Broker thành công nhưng ghi DB thất bại (do vi phạm constraint, rollback): Broker đã phát tán event giả mạo không tồn tại trong hệ thống (Phantom Event).
3. **Sự sụp đổ của 2-Phase Commit (2PC / XA Transactions):** Việc áp dụng giao thức phân tán 2PC giữa RDBMS và Message Broker vừa làm tăng độ trễ lên gấp hàng chục lần, vừa không được hỗ trợ bởi các broker hiện đại (Kafka, NATS), đồng thời biến hệ thống thành một điểm nghẽn chịu lỗi đơn độc (Single Point of Failure).

Crate `libs/messaging` (`omni-messaging`) cung cấp giải pháp toàn diện cho bài toán trên bằng cách hiện thực hóa mẫu thiết kế **Transactional Outbox & Inbox Pattern**, chuẩn hóa dữ liệu theo **CNCF CloudEvents v1.0.2**, duy trì tính liên tục của vết giám sát **W3C Distributed Tracing**, và cung cấp khung xử lý **Resilient Consumer** chống tắc nghẽn với cơ chế **Exponential Backoff with Full Jitter** và **Dead Letter Queue (DLQ)**.

```mermaid
flowchart TD
    subgraph Principles["9 NGUYÊN LÝ THIẾT KẾ CỐT LÕI (OMNI-MESSAGING)"]
        P1["1. Transactional Outbox by Default<br/>(Triệt tiêu Dual-Write bằng ACID Local Tx)"]
        P2["2. Strict CloudEvents v1.0.2 Envelope<br/>(Chuẩn hóa Schema, UUIDv7 & Metadata)"]
        P3["3. Deterministic Ordering by Key<br/>(Bảo toàn thứ tự nhân quả theo Aggregate ID)"]
        P4["4. Dual-Layer Idempotent Inbox<br/>(Lọc trùng 2 lớp: Redis Cache + PG Inbox)"]
        P5["5. Poison Pill Isolation & DLQ Routing<br/>(Cách ly thông điệp độc, không nghẽn queue)"]
        P6["6. End-to-End W3C Trace Propagation<br/>(Bảo toàn vết APM xuyên biên giới mạng)"]
        P7["7. Broker-Agnostic Pluggable Drivers<br/>(Trừu tượng hóa RabbitMQ / Kafka / NATS / Memory)"]
        P8["8. High-Throughput Hybrid Relay<br/>(Kết hợp LISTEN/NOTIFY và SKIP LOCKED)"]
        P9["9. Deep Observability & OTel Semantics<br/>(Metrics RED, Consumer Lag, PII Sanitization)"]
    end
```

### 9 Nguyên lý Kiến trúc Cốt lõi:

1. **Transactional Outbox by Default (Khắc phục triệt để Dual-Write):**
   - Mọi sự kiện phát sinh từ domain nghiệp vụ bắt buộc phải được ghi nhận vào bảng `outbox_events` trong **cùng một Transaction PostgreSQL** với thực thể nghiệp vụ thông qua `omni_database::UnitOfWork`.
   - Cam kết phân phối ít nhất một lần (**At-Least-Once Delivery**) mà không cần bất kỳ giao thức khóa phân tán 2PC phức tạp nào.
2. **Strict CloudEvents v1.0.2 Data Contract:**
   - Tất cả các event đều tuân thủ 100% đặc tả chuẩn quốc tế **CNCF CloudEvents v1.0.2**.
   - Định danh event duy nhất bằng **UUIDv7** (time-ordered), tự sinh dấu thời gian UTC, nguồn phát hành định dạng URN (`urn:omni:service:{name}`), và phân loại schema định danh theo quy tắc phân cấp ngược (`com.omni.{service}.{entity}.{action}.v{version}`).
3. **Deterministic Partition Ordering (Bảo toàn Thứ tự Nhân quả):**
   - Mọi event liên quan đến cùng một Aggregate Root (ví dụ: các biến động của cùng một `order_id` hay `account_id`) đều mang chung một `partition_key`.
   - Message Broker và Consumer Worker bảo đảm xử lý tuần tự theo đúng trình tự thời gian sinh ra của các event này.
4. **Dual-Layer Idempotent Inbox (Khử trùng lặp 2 tầng):**
   - Do tính chất phân phối At-Least-Once, Consumer luôn đối mặt với nguy cơ nhận trùng lặp bản tin (Duplicate Delivery).
   - Hệ thống triển khai bộ lọc trùng lặp 2 lớp:
     - **Tầng 1 (Fast Path):** Kiểm tra và xác lập khóa nguyên tử trên Redis (`SET NX EX`) qua `omni-cache` với thời gian phản hồi dưới 1ms.
     - **Tầng 2 (Strong Consistency Path):** Ghi nhận bản ghi vào bảng `inbox_events` bên trong transaction nghiệp vụ của Consumer tại PostgreSQL.
5. **Poison Pill Isolation & Resilient DLQ Routing (Cách ly bản tin độc):**
   - Các thông điệp bị lỗi định dạng, dữ liệu hỏng hoặc lỗi logic không thể tự hồi phục (Unrecoverable Errors) không được phép làm treo tiến trình Consumer hoặc gây hiện tượng Head-of-Line Blocking.
   - Khi vượt quá số lần thử lại tối đa (theo thuật toán Exponential Backoff kèm Full Jitter), bản tin tự động được định tuyến sang **Dead Letter Queue (DLQ)** kèm đầy đủ thông tin phân tích nguyên nhân để đội ngũ vận hành can thiệp.
6. **End-to-End W3C Context Propagation (Bảo toàn Vết Giám sát Phân tán):**
   - Header W3C `traceparent` và `tracestate` từ `omni_common::RequestContext` được tự động tiêm (Inject) vào CloudEvent extensions và Broker transport headers khi ghi Outbox.
   - Khi Consumer tiếp nhận thông điệp, ngữ cảnh này được trích xuất (Extract) để kích hoạt một `tracing::Span` dạng `Consumer`, tạo nên một chuỗi APM Trace liền mạch từ HTTP Gateway xuyên qua hàng đợi đến các worker hạ tầng.
7. **Broker-Agnostic Pluggable Drivers (Độc lập Hạ tầng Truyền dẫn):**
   - Nghiệp vụ không phụ thuộc trực tiếp vào SDK của bất kỳ Message Broker nào.
   - Lớp giao tiếp được trừu tượng hóa qua các Rust trait kiểu tĩnh `MessagePublisher` và `MessageConsumer`, hỗ trợ cắm-rút linh hoạt giữa **RabbitMQ (AMQP 0-9-1)**, **Apache Kafka / Redpanda**, **NATS JetStream** và **In-Memory Channel** phục vụ kiểm thử.
8. **High-Throughput Hybrid Relay (`SKIP LOCKED` + `LISTEN/NOTIFY`):**
   - Tiến trình Outbox Relay kết hợp thông minh giữa cơ chế **PostgreSQL `LISTEN/NOTIFY`** để thức tỉnh tức thì (Zero-Latency Wakeup khi có transaction commit) và câu lệnh **`SELECT ... FOR UPDATE SKIP LOCKED`** để kéo dữ liệu theo lô (Micro-batching) mà không gây khóa tranh chấp giữa nhiều Pod Relay chạy song song.
   - Tích hợp Safety Ticker định kỳ ngăn ngừa trễ bản ghi khi tải cao hoặc rớt kết nối socket thông báo.
9. **Deep Observability & OpenTelemetry Messaging Conventions:**
   - Tuân thủ toàn diện chuẩn **OpenTelemetry Semantic Conventions for Messaging Systems v1.27+**.
   - Cung cấp bộ chỉ số Prometheus chuyên dụng đo lường: Outbox Queue Depth, Dispatch Latency, Consumer Processing Time, Consumer Lag, Retry Count và DLQ Ingestion Rate. Tích hợp bộ lọc **PII Masking** ngăn rò rỉ dữ liệu nhạy cảm trong payload event ra log.

---

## 2. Scope of Applicability & Workspace Dependency Architecture

### 2.1. Nằm trong phạm vi (In-Scope)

| #   | Năng lực / Module                                        | Mô tả kỹ thuật chi tiết                                                                                                                                                                                                      |
| :-- | :------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **CloudEvents Engine (`cloudevent::*`)**                 | Triển khai đặc tả CNCF CloudEvents v1.0.2: `CloudEvent<T>`, `CloudEventBuilder`, validation quy tắc đặt tên, hỗ trợ cả hai chế độ mã hóa **Structured Content Mode** và **Binary Content Mode**, serialization zero-copy.    |
| 2   | **Transactional Outbox Engine (`outbox::*`)**            | Quản lý vòng đời bản ghi `outbox_events` trên PostgreSQL. Cung cấp API `OutboxRepository::record` gắn liền với `omni_database::UnitOfWork`. Định nghĩa schema DDL kèm partial indexes tối ưu hóa cho Worker Polling.         |
| 3   | **Hybrid Outbox Relay Worker (`outbox::relay`)**         | Tiến trình nền quét các event trạng thái `PENDING` sử dụng `FOR UPDATE SKIP LOCKED`. Cơ chế đánh thức tức thì qua kênh PostgreSQL `LISTEN/NOTIFY` (`omni_outbox_tick`). Cơ chế backoff và retry cho các event phát hành lỗi. |
| 4   | **Broker Transport Abstraction (`broker::*`)**           | Các trait cốt lõi `MessagePublisher`, `MessageConsumer`, `RawBrokerMessage`. Triển khai các Driver cắm-rút: In-Memory (Test/CI), RabbitMQ (Lapin), Kafka (Rdkafka), NATS JetStream (async-nats).                             |
| 5   | **Distributed Tracing & Context Carrier (`tracing::*`)** | Inject và Extract W3C `traceparent`, `tracestate` và OTel baggage giữa `omni_common::RequestContext`, `tracing::Span`, CloudEvent Extensions và Broker Message Headers.                                                      |
| 6   | **Idempotent Inbox Engine (`inbox::*`)**                 | Bộ lọc khử trùng lặp 2 lớp: Redis atomic locking (`omni-cache`) và bảng cơ sở dữ liệu `inbox_events` (`omni-database`). Chống tấn công replay và bảo vệ tính bất biến của dữ liệu.                                           |
| 7   | **Resilient Consumer Pipeline (`consumer::*`)**          | Pipeline xử lý thông điệp chịu lỗi: Concurrency Controller, Exponential Backoff with Full Jitter, Poison Pill Detection, Automatic DLQ Routing và Dead Letter Headers enrichment.                                            |
| 8   | **Declarative Message Handlers (`consumer::handler`)**   | Trait `MessageHandler<T>` thân thiện với developer, tự động deserialize payload CloudEvent sang Rust struct kiểu tĩnh, bọc xử lý trong span giám sát và trả về kết quả Ack/Nack/Retry/DLQ.                                   |
| 9   | **Housekeeping & Retention Engine (`outbox::cleanup`)**  | Cơ chế dọn dẹp các bản ghi Outbox và Inbox đã hoàn tất: Hard-delete tức thời, Soft-delete lưu vết, hoặc Batch Retention Pruner định kỳ để chống phình to bảng (Table Bloat).                                                 |
| 10  | **Messaging Observability & Metrics (`instrument::*`)**  | Xuất bản bộ chỉ số Prometheus chuẩn OTel: `messaging_outbox_queue_depth`, `messaging_publish_duration_seconds`, `messaging_consumer_duration_seconds`, `messaging_consumer_errors_total`, `messaging_dlq_routed_total`.      |
| 11  | **Error Taxonomy & RFC 9457 Adapter (`error::*`)**       | Định nghĩa `MessagingError` phân loại chi tiết các tình huống lỗi hạ tầng và lỗi xử lý, tự động chuyển đổi sang `omni_errors::AppError` khi cần tương tác qua tầng Web/API.                                                  |
| 12  | **Integration Test Harness (`test_utils::*`)**           | Driver In-Memory Broker giả lập, Testcontainers Postgres/RabbitMQ/Kafka fixtures, helper mô phỏng rớt mạng, mô phỏng xung đột transaction và xác thực tính lũy thừa (Idempotency Verification).                              |

### 2.2. Ngoài phạm vi (Out-of-Scope)

- **Định nghĩa Event Nghiệp vụ cụ thể của từng Domain (`services/*`):** `omni-messaging` không chứa các schema dữ liệu cụ thể như `OrderCreatedPayload`, `UserRegisteredPayload` hay `PaymentCompletedPayload`. Các struct này được định nghĩa trực tiếp tại các crate nghiệp vụ hoặc shared contracts crate giữa các microservices.
- **Hạ tầng Quản lý Schema Registry Tập trung (Confluent / Apicurio):** v1.0 tập trung hoàn thiện định dạng CloudEvents v1.0.2 với JSON Schema. Việc quản lý Schema Registry qua Avro/Protobuf binary schema sẽ được mở rộng trong v1.2 khi hệ thống có yêu cầu nén payload cực hạn.
- **Phức hợp Saga Orchestrator (Temporal / Camunda):** `omni-messaging` cung cấp cơ chế gửi nhận Choreography-based Events. Việc điều phối Saga Orchestration tập trung bằng state engine riêng biệt nằm ngoài phạm vi của thư viện messaging cơ sở.

### 2.3. Sơ đồ Phụ thuộc Kiến trúc (Acyclic Crate Dependency Graph)

```mermaid
flowchart TD
    subgraph Services["Domain Microservices (services/*)"]
        ORDER_SVC["services/order<br/>(Transactional Outbox Writer)"]
        PAYMENT_SVC["services/payment<br/>(Idempotent Consumer + Publisher)"]
        NOTIF_SVC["services/notification<br/>(Resilient Consumer + DLQ)"]
    end

    subgraph MessagingCrate["libs/messaging (omni-messaging)"]
        CE["cloudevent<br/>(CloudEvent, Builder, Extensions)"]
        OUTBOX["outbox<br/>(Repository, SKIP LOCKED Relay, LISTEN/NOTIFY)"]
        INBOX["inbox<br/>(Dual-Layer Deduplication Engine)"]
        CONSUMER["consumer<br/>(Worker Pool, Resilient Backoff, DLQ)"]
        BROKER["broker<br/>(Publisher/Consumer Traits, Drivers)"]
        TRACE["tracing<br/>(W3C Carrier Injector/Extractor)"]
        INSTR["instrument<br/>(Prometheus RED Metrics, OTel Spans)"]
        ERR["error<br/>(MessagingError -> AppError)"]
    end

    subgraph SharedFoundation["Shared Foundation Libraries (libs/*)"]
        COMMON["omni-common<br/>(RequestContext, SecretString, Config)"]
        ERRORS["omni-errors<br/>(AppError, RFC 9457 Problem Details)"]
        DB["omni-database<br/>(UnitOfWork, PgPool, AuditColumns)"]
        CACHE["omni-cache<br/>(TypedCache, Redis Distributed Lock)"]
        METRICS["omni-metrics<br/>(Prometheus Registry Facade)"]
        LOGGING["omni-logging<br/>(PII Sanitizer, OTLP Tracing Appender)"]
    end

    ORDER_SVC --> MessagingCrate
    PAYMENT_SVC --> MessagingCrate
    NOTIF_SVC --> MessagingCrate

    MessagingCrate --> COMMON
    MessagingCrate --> ERRORS
    MessagingCrate --> METRICS
    MessagingCrate --> LOGGING
    MessagingCrate -->|"feature: postgres (default)"| DB
    MessagingCrate -->|"feature: cache-dedup (default)"| CACHE
```

### 2.4. Cargo Feature Matrix

Để đảm bảo microservice chỉ compile đúng những gì mình thực sự sử dụng, `omni-messaging` thiết kế hệ thống Cargo Features dạng module hóa:

```toml
[features]
default = ["postgres", "cache-dedup", "rabbitmq"]

# Cung cấp Transactional Outbox & Inbox lưu trữ trên PostgreSQL qua omni-database
postgres = ["dep:omni-database", "dep:sqlx"]

# Cung cấp bộ lọc trùng lặp nhanh Tầng 1 qua Redis/Valkey (omni-cache)
cache-dedup = ["dep:omni-cache"]

# Các Driver Message Broker cắm-rút
in-memory = []
rabbitmq = ["dep:lapin"]
kafka = ["dep:rdkafka"]
nats = ["dep:async-nats"]

# Tiện ích kiểm thử giả lập, Testcontainers fixtures
test-utils = ["in-memory", "dep:testcontainers"]
```

---

## 3. Standards & Compliance References

| Tiêu chuẩn / Đặc tả                             | Cơ quan ban hành           | Nội dung áp dụng trong `omni-messaging`                                                                                                                                                                            |
| :---------------------------------------------- | :------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **CNCF CloudEvents v1.0.2**                     | Cloud Native Computing FDN | Định dạng cấu trúc bản tin chuẩn cho các trường bắt buộc (`id`, `source`, `specversion`, `type`), trường tùy chọn (`time`, `datacontenttype`, `subject`) và phần mở rộng extensions.                               |
| **W3C Trace Context (Level 1)**                 | W3C Recommendation         | Định dạng header `traceparent` (32-hex Trace ID, 16-hex Span ID, Trace Flags) và `tracestate` dùng để duy trì vết APM xuyên qua các Message Broker.                                                                |
| **AMQP 0-9-1 Specification**                    | OASIS / RabbitMQ           | Mô hình Exchange-Queue-Binding, Topic Exchange định tuyến theo Routing Key, Publisher Confirms, Manual Acknowledgment (`Basic.Ack` / `Basic.Nack` / `Basic.Reject`), Quorum Queues và Dead Letter Exchanges (DLX). |
| **Apache Kafka Protocol**                       | Apache Software FDN        | Khái niệm Topics, Partitions, Offset Commit, Consumer Groups, Partition Keys và Idempotent Producer Semantics.                                                                                                     |
| **NATS JetStream Protocol**                     | Synadia / CNCF             | Stream Persistence, Subject Hierarchies (`omni.events.>`), Consumer Ack Policy, Max Deliveries và Dead Letter Subjects.                                                                                            |
| **OpenTelemetry Semantic Conventions (v1.27+)** | OpenTelemetry Community    | Quy ước đặt tên thuộc tính span: `messaging.system`, `messaging.destination.name`, `messaging.operation.type`, `messaging.message.id`, `messaging.message.body.size`.                                              |
| **Enterprise Integration Patterns (EIP)**       | Gregor Hohpe & Bobby Woolf | Transactional Outbox, Transactional Inbox, Idempotent Receiver, Message Router, Competing Consumers, Dead Letter Channel.                                                                                          |
| **RFC 9457 Problem Details**                    | IETF                       | Cấu trúc phản hồi lỗi chuẩn khi các tác vụ messaging gặp sự cố cần trả về qua HTTP Gateway.                                                                                                                        |

---

## 4. Current State & Gap Analysis

### 4.1. Những gì đã có sẵn trong Workspace (Reused Foundation)

1. **`omni-database` (Unit of Work & PgPool):**
   - Đã sở hữu cơ chế `with_transaction` và `UnitOfWork` cho phép thực thi truy vấn SQL trong transaction quản lý bởi `sqlx::Transaction`.
   - `uow.conn()` trả về `&mut sqlx::PgConnection`, là điểm neo hoàn hảo để gắn câu lệnh chèn vào bảng `outbox_events` cùng transaction với entity nghiệp vụ.
   - Cơ chế xử lý lỗi `DbError` chuẩn hóa từ mã SQLSTATE PostgreSQL.
2. **`omni-cache` (Distributed Cache & Redlock):**
   - Sở hữu `TypedCache` và `DistributedLock` đã được kiểm chứng với Valkey/Redis.
   - Cung cấp nền tảng lý tưởng cho tầng lọc trùng lặp nhanh Tầng 1 (Fast Deduplication) với Redis atomic key TTL.
3. **`omni-logging` & `omni-common` (Context & PII Masking):**
   - `RequestContext` mang sẵn `request_id`, `trace_id`, `span_id`, `tracestate`, `user_id`, `tenant_id`.
   - Tiện ích trích xuất/sinh mã W3C `traceparent` tại `omni_common::middleware::traceparent`.
   - Bộ lọc `PIISanitizer` sẵn sàng để che giấu các trường nhạy cảm trong payload của event trước khi ghi log.
4. **`omni-metrics` (Prometheus Registry):**
   - Facade `metrics` đã được tích hợp sẵn sàng trong workspace, cho phép khai báo counter, histogram và gauge tức thì mà không cần setup boilerplate.

### 4.2. Các khoảng trống kỹ thuật (Gaps) cần giải quyết trong `omni-messaging`

1. **Thiếu Data Contract Chuẩn cho Sự kiện:**
   - Hiện tại chưa có struct nào đại diện cho một Event phân tán. Nếu các service tự định nghĩa struct riêng sẽ dẫn đến hỗn loạn về tên trường (`event_id` vs `id`, `timestamp` vs `created_at`, `type` vs `event_name`).
   - Cần chuẩn hóa dứt điểm qua đặc tả `CloudEvent<T>`.
2. **Chưa có Bảng và Cơ chế Transactional Outbox:**
   - Chưa có migration DDL tạo bảng `outbox_events`.
   - Chưa có tiến trình Outbox Polling Relay để kéo event từ PostgreSQL ra Broker.
   - Chưa có cơ chế giải phóng khóa tranh chấp giữa nhiều pod instance chạy cùng một lúc.
3. **Chưa có Cơ chế Bắt tay với Message Broker:**
   - Chưa có lớp trừu tượng Driver để code nghiệp vụ không bị "dính chặt" vào RabbitMQ hay Kafka SDK.
   - Cần thiết kế Driver Pattern với các adapter cụ thể.
4. **Nguy cơ Xử lý Trùng lặp (Duplicate Message Processing):**
   - Mạng phân tán luôn có lúc bị retry khiến một event được bắn tới Consumer 2 hay nhiều lần. Chưa có giải pháp Inbox Pattern để bảo đảm Idempotency.
5. **Chưa có Cơ chế Bảo vệ Khỏi Poison Pill:**
   - Nếu một consumer gặp event lỗi không thể xử lý, nếu cứ retry liên tục sẽ gây nghẽn toàn bộ hàng đợi. Cần xây dựng Resilient Pipeline với Exponential Backoff và Dead Letter Queue.

---

## 5. Configuration Model (`MessagingConfig`)

### 5.1. Mô hình Cấu hình Phân tầng (Layered Hierarchical Configuration)

Mọi tham số cấu hình của `omni-messaging` được tích hợp liền mạch vào hệ thống cấu hình phân tầng 5 lớp của `omni-common::AppConfig`, nạp từ tệp `config/base.yaml`, ghi đè theo môi trường `config/{env}.yaml`, và ghi đè cấp cao nhất bằng biến môi trường tiền tố `OMNI__MESSAGING__*`.

```yaml
# config/base.yaml (Phần cấu hình omni-messaging)
messaging:
  # Driver Message Broker lựa chọn: in_memory | rabbitmq | kafka | nats
  broker_type: "rabbitmq"

  # Thông tin kết nối tới Message Broker
  broker:
    url: "amqp://guest:guest@localhost:5672/%2f"
    connection_timeout_ms: 5000
    heartbeat_secs: 30
    pool_size: 10
    tls_enabled: false

  # Cấu hình Transactional Outbox Engine
  outbox:
    enabled: true
    # Chu kỳ quét fallback khi không có trigger LISTEN/NOTIFY (ms)
    poll_interval_ms: 1000
    # Kích thước lô bản tin quét mỗi chu kỳ
    batch_size: 100
    # Thời hạn tối đa khóa xử lý trước khi coi là worker bị crash (ms)
    lock_duration_ms: 30000
    # Số lần thử lại tối đa trước khi đánh dấu DEAD
    max_retries: 5
    # Kênh PostgreSQL LISTEN/NOTIFY để đánh thức relay tức thì
    notify_channel: "omni_outbox_tick"
    # Chiến lược dọn dẹp bản ghi đã hoàn tất: HardDelete | SoftDelete | RetentionPruning
    cleanup_strategy: "RetentionPruning"
    # Thời gian lưu trữ bản ghi đã gửi trước khi xóa (giờ)
    retention_hours: 72

  # Cấu hình Khung nhận tin cậy (Resilient Consumer Framework)
  consumer:
    # Số lượng worker tác vụ Tokio chạy đồng thời cho mỗi topic
    concurrency: 16
    # Số bản tin nạp trước từ Broker về bộ nhớ đệm (Prefetch Count / QoS)
    prefetch_count: 50
    # Số lần thử lại trước khi chuyển sang DLQ
    max_retries: 5
    # Thời gian chờ backoff lần đầu tiên (ms)
    initial_retry_backoff_ms: 100
    # Trần thời gian chờ backoff tối đa (ms)
    max_retry_backoff_ms: 60000
    # Hệ số nhân lũy tiến backoff
    backoff_multiplier: 2.0
    # Bật/tắt Full Jitter ngẫu nhiên
    backoff_jitter: true
    # Hậu tố quy ước cho Dead Letter Queue / Topic
    dlq_topic_suffix: ".dlq"
    # Thời gian sống của cache lọc trùng lặp Tầng 1 (giây)
    dedup_ttl_secs: 86400
```

### 5.2. Rust Struct Representation

```rust
use std::time::Duration;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Cấu hình tổng thể của phân hệ Event-Driven Messaging.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MessagingConfig {
    /// Loại broker sử dụng: `in_memory`, `rabbitmq`, `kafka`, `nats`.
    pub broker_type: BrokerType,

    /// Tham số kết nối broker.
    #[validate(nested)]
    pub broker: BrokerConnectionConfig,

    /// Tham số của Transactional Outbox Engine.
    #[validate(nested)]
    pub outbox: OutboxConfig,

    /// Tham số của Resilient Consumer Framework.
    #[validate(nested)]
    pub consumer: ConsumerConfig,
}

/// Loại Message Broker được hỗ trợ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrokerType {
    #[default]
    InMemory,
    Rabbitmq,
    Kafka,
    Nats,
}

/// Cấu hình kết nối tới Broker.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BrokerConnectionConfig {
    /// URL kết nối được bảo vệ trong SecretString chống rò rỉ.
    pub url: SecretString,

    /// Thời gian chờ kết nối tối đa.
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: Duration,

    /// Tần suất gửi heartbeat giữ kết nối sống (giây).
    #[serde(default = "default_heartbeat_secs")]
    pub heartbeat_secs: u64,

    /// Kích thước pool kết nối nội bộ.
    #[validate(range(min = 1, max = 100))]
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,

    /// Bật/tắt bảo mật tầng vận chuyển TLS.
    #[serde(default)]
    pub tls_enabled: bool,
}

/// Cấu hình Transactional Outbox.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OutboxConfig {
    /// Bật hoặc tắt tính năng Outbox Publisher Worker trên tiến trình này.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Chu kỳ quét cơ sở dữ liệu định kỳ (ms).
    #[serde(default = "default_poll_interval")]
    pub poll_interval: Duration,

    /// Số lượng bản tin lấy ra tối đa trong mỗi mẻ quét.
    #[validate(range(min = 1, max = 1000))]
    #[serde(default = "default_batch_size")]
    pub batch_size: i64,

    /// Thời gian khóa một bản tin trước khi cho phép worker khác lấy lại.
    #[serde(default = "default_lock_duration")]
    pub lock_duration: Duration,

    /// Số lần thử gửi lại tối đa khi broker từ chối hoặc lỗi mạng.
    #[validate(range(min = 1, max = 20))]
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,

    /// Tên kênh PostgreSQL LISTEN/NOTIFY.
    #[serde(default = "default_notify_channel")]
    pub notify_channel: String,

    /// Chiến lược dọn dẹp dữ liệu outbox đã gửi thành công.
    #[serde(default)]
    pub cleanup_strategy: CleanupStrategy,

    /// Thời hạn lưu trữ dữ liệu trước khi dọn dẹp (giờ).
    #[validate(range(min = 1, max = 720))]
    #[serde(default = "default_retention_hours")]
    pub retention_hours: i64,
}

/// Chiến lược dọn dẹp bản tin Outbox / Inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupStrategy {
    /// Xóa vật lý ngay lập tức khi gửi thành công sang Broker (Tối ưu lưu trữ nhất).
    HardDelete,
    /// Đánh dấu trạng thái `PUBLISHED` và giữ nguyên (Cần công cụ ngoài prune).
    SoftDelete,
    /// Định kỳ quét và xóa các bản tin đã hoàn tất vượt quá retention_hours (Khuyến nghị).
    #[default]
    RetentionPruning,
}

/// Cấu hình Resilient Consumer Framework.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConsumerConfig {
    /// Số lượng tác vụ worker song song xử lý bản tin.
    #[validate(range(min = 1, max = 256))]
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    /// Số bản tin lấy về sẵn trong bộ nhớ đệm của socket.
    #[validate(range(min = 1, max = 1000))]
    #[serde(default = "default_prefetch_count")]
    pub prefetch_count: u16,

    /// Số lần thử lại tối đa cho mỗi thông điệp bị lỗi trước khi chuyển sang DLQ.
    #[validate(range(min = 0, max = 20))]
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Thời gian chờ thử lại ban đầu.
    #[serde(default = "default_initial_backoff")]
    pub initial_retry_backoff: Duration,

    /// Thời gian chờ thử lại tối đa.
    #[serde(default = "default_max_backoff")]
    pub max_retry_backoff: Duration,

    /// Hệ số nhân Exponential Backoff.
    #[validate(range(min = 1.0, max = 10.0))]
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// Bật jitter ngẫu nhiên để chống thundering herd.
    #[serde(default = "default_true")]
    pub backoff_jitter: bool,

    /// Hậu tố gắn vào topic để tạo DLQ topic tương ứng.
    #[serde(default = "default_dlq_suffix")]
    pub dlq_topic_suffix: String,

    /// Thời gian sống của bản ghi kiểm tra trùng lặp trên Cache (giây).
    #[serde(default = "default_dedup_ttl")]
    pub dedup_ttl: Duration,
}

// Helper default values functions...
const fn default_true() -> bool { true }
const fn default_connection_timeout() -> Duration { Duration::from_millis(5000) }
const fn default_heartbeat_secs() -> u64 { 30 }
const fn default_pool_size() -> usize { 10 }
const fn default_poll_interval() -> Duration { Duration::from_millis(1000) }
const fn default_batch_size() -> i64 { 100 }
const fn default_lock_duration() -> Duration { Duration::from_secs(30) }
const fn default_max_retries() -> i32 { 5 }
fn default_notify_channel() -> String { "omni_outbox_tick".to_string() }
const fn default_retention_hours() -> i64 { 72 }
const fn default_concurrency() -> usize { 16 }
const fn default_prefetch_count() -> u16 { 50 }
const fn default_initial_backoff() -> Duration { Duration::from_millis(100) }
const fn default_max_backoff() -> Duration { Duration::from_secs(60) }
const fn default_backoff_multiplier() -> f64 { 2.0 }
fn default_dlq_suffix() -> String { ".dlq".to_string() }
const fn default_dedup_ttl() -> Duration { Duration::from_secs(86400) }
```

### 5.3. Fail-Fast Startup Guardrails

Trong quá trình khởi tạo ứng dụng, `omni-messaging` thực hiện kiểm tra chéo các bất biến cấu hình (**Phase 2 Validation**) trước khi cấp phát tài nguyên:

1. **Broker URL Sanity Check:** Nếu `broker_type` khác `InMemory`, URL kết nối bắt buộc phải có tiền tố hợp lệ tương ứng (`amqp://` hoặc `amqps://` cho RabbitMQ, chuỗi host:port cho Kafka, `nats://` cho NATS).
2. **Backoff Invariant:** Bắt buộc `initial_retry_backoff <= max_retry_backoff`.
3. **Lock Duration vs Poll Interval:** `lock_duration` phải lớn hơn ít nhất gấp 3 lần `poll_interval` để tránh việc một batch đang được gửi sang broker thì bị worker khác tranh chấp xử lý lại (False Duplication).
4. **Zero-Leak Secret:** URL chứa thông tin xác thực bắt buộc phải được bọc trong `SecretString`.

---

## 6. CNCF CloudEvents Specification & Data Contract

### 6.1. Cấu trúc Envelope Chuẩn (`CloudEvent<T>`)

Mọi sự kiện trong hệ thống OmniKubernetes đều bắt buộc phải được bọc trong một phong bì (Envelope) chuẩn hóa tuân thủ **CNCF CloudEvents v1.0.2**.

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Phong bì sự kiện chuẩn CNCF CloudEvents v1.0.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudEvent<T> {
    /// Định danh sự kiện duy nhất (Bắt buộc phải là UUIDv7 time-ordered).
    pub id: Uuid,

    /// Định danh thành phần phát hành sự kiện (URI-reference dạng URN).
    /// Ví dụ: `urn:omni:service:order`
    pub source: String,

    /// Phiên bản của đặc tả CloudEvents. Luôn cố định là "1.0".
    pub specversion: String,

    /// Tên loại sự kiện theo quy tắc phân cấp ngược miền (Reverse-DNS taxonomy).
    /// Ví dụ: `com.omni.order.created.v1`
    #[serde(rename = "type")]
    pub event_type: String,

    /// Thời điểm sinh ra sự kiện theo chuẩn RFC 3339 UTC.
    pub time: DateTime<Utc>,

    /// Kiểu nội dung của payload dữ liệu. Mặc định luôn là "application/json".
    #[serde(default = "default_content_type")]
    pub datacontenttype: String,

    /// URI trỏ tới JSON Schema định nghĩa cấu trúc dữ liệu của payload (tùy chọn).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataschema: Option<String>,

    /// Định danh thực thể nghiệp vụ là đối tượng của sự kiện (thường là Aggregate ID).
    /// Ví dụ: Mã đơn hàng "018f4a2b-8a5e-7a1b"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    /// Payload dữ liệu nghiệp vụ thực tế.
    pub data: T,

    /// Các thuộc tính mở rộng chuẩn hóa (CloudEvents Extension Context Attributes).
    #[serde(flatten)]
    pub extensions: CloudEventExtensions,
}

fn default_content_type() -> String {
    "application/json".to_string()
}
```

### 6.2. Extensions Model & Chuẩn hóa Metadata

Để phục vụ phân luồng, giám sát phân tán và mô hình đa người thuê (Multi-Tenancy), `CloudEventExtensions` định nghĩa các trường bắt buộc và mở rộng:

```rust
/// Các thuộc tính mở rộng chuẩn hóa đính kèm trong CloudEvent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudEventExtensions {
    /// W3C Distributed Trace Context: `traceparent` (32-hex trace_id + 16-hex span_id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,

    /// W3C `tracestate` lưu trữ routing context của các nhà cung cấp APM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,

    /// Định danh Tenant sở hữu dữ liệu (UUIDv7) trong kiến trúc Multi-Tenant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<Uuid>,

    /// Định danh Actor/User khởi tạo hành động (UUIDv7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<Uuid>,

    /// Khóa phân vùng để Broker định tuyến vào cùng partition (Aggregate ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partition_key: Option<String>,

    /// Khóa định danh chống trùng lặp (Idempotency Key). Nếu không điền, mặc định bằng `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,

    /// Bảng lưu các thuộc tính mở rộng tùy biến khác.
    #[serde(flatten)]
    pub custom: HashMap<String, String>,
}
```

### 6.3. Content Modes: Structured Mode vs. Binary Mode

CloudEvents hỗ trợ 2 chế độ truyền tải dữ liệu trên đường truyền Message Broker:

| Tiêu chí                 | Structured Content Mode                                                                             | Binary Content Mode (Khuyến nghị cho Production)                                                 |
| :----------------------- | :-------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------- |
| **Cơ chế đóng gói**      | Toàn bộ Envelope CloudEvent (bao gồm metadata và payload) nằm trong body bản tin JSON.              | Metadata CloudEvent được ánh xạ thành **Message Headers**, body chỉ chứa payload dữ liệu thô.    |
| **Hiệu năng định tuyến** | Kém: Message Broker phải giải mã (parse) toàn bộ JSON body nếu muốn định tuyến dựa trên event_type. | Tối ưu tuyệt đối: Broker lọc và định tuyến trực tiếp dựa trên Header mà không cần đụng vào Body. |
| **Khả năng tương thích** | Dễ gỡ lỗi: Nhìn vào body bản tin thấy toàn bộ thông tin ngữ cảnh.                                   | Tiêu chuẩn công nghiệp: Tương thích hoàn hảo với AMQP, Kafka, NATS và gRPC gateways.             |
| **Kích thước gói tin**   | Tốn dung lượng hơn do trùng lặp cấu trúc JSON bọc ngoài.                                            | Nhỏ hơn: Tiết kiệm băng thông mạng và chi phí lưu trữ trên hàng đợi.                             |

> [!TIP]
> `omni-messaging` mặc định sử dụng **Binary Content Mode** khi giao tiếp với RabbitMQ (AMQP headers), Kafka (Record Headers) và NATS (Message Headers). Chế độ **Structured Content Mode** được sử dụng khi serialize lưu trữ vào bảng cơ sở dữ liệu `outbox_events` (cột `payload`) hoặc xuất bản ra các Webhook công khai.

### 6.4. Type-Safe Event Builder Pattern

Để loại bỏ hoàn toàn boilerplate code, `omni-messaging` cung cấp `CloudEventBuilder`:

```rust
pub struct CloudEventBuilder<T> {
    source: String,
    event_type: String,
    subject: Option<String>,
    data: T,
    extensions: CloudEventExtensions,
}

impl<T> CloudEventBuilder<T> {
    pub fn new(source: impl Into<String>, event_type: impl Into<String>, data: T) -> Self {
        Self {
            source: source.into(),
            event_type: event_type.into(),
            subject: None,
            data,
            extensions: CloudEventExtensions::default(),
        }
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn with_request_context(mut self, ctx: &omni_common::middleware::RequestContext) -> Self {
        if let Some(ref tp) = ctx.traceparent {
            self.extensions.traceparent = Some(tp.to_string());
        }
        if let Some(ref ts) = ctx.tracestate {
            self.extensions.tracestate = Some(ts.to_string());
        }
        if let Some(ref tenant) = ctx.tenant_id {
            if let Ok(tid) = Uuid::parse_str(tenant) {
                self.extensions.tenant_id = Some(tid);
            }
        }
        if let Some(ref user) = ctx.user_id {
            if let Ok(uid) = Uuid::parse_str(user) {
                self.extensions.actor_id = Some(uid);
            }
        }
        self
    }

    pub fn partition_key(mut self, key: impl Into<String>) -> Self {
        self.extensions.partition_key = Some(key.into());
        self
    }

    pub fn build(self) -> CloudEvent<T> {
        let event_id = Uuid::now_v7();
        let mut extensions = self.extensions;
        if extensions.idempotency_key.is_none() {
            extensions.idempotency_key = Some(event_id.to_string());
        }

        CloudEvent {
            id: event_id,
            source: self.source,
            specversion: "1.0".to_string(),
            event_type: self.event_type,
            time: Utc::now(),
            datacontenttype: default_content_type(),
            dataschema: None,
            subject: self.subject,
            data: self.data,
            extensions,
        }
    }
}
```

---

## 7. Transactional Outbox Engine (`omni-messaging::outbox`)

### 7.1. Database Schema `outbox_events`

Bảng `outbox_events` được thiết kế tối ưu hóa đặc biệt cho các câu lệnh `SELECT ... FOR UPDATE SKIP LOCKED` và việc phân mảnh theo thời gian (Time-range Partitioning ready):

```sql
-- Migration: V20261102_000001__create_outbox_events.sql

CREATE TABLE IF NOT EXISTS outbox_events (
    id UUID PRIMARY KEY,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(128) NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    destination_topic VARCHAR(256) NOT NULL,
    partition_key VARCHAR(128),
    payload JSONB NOT NULL,
    headers JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    retry_count INT NOT NULL DEFAULT 0,
    max_retries INT NOT NULL DEFAULT 5,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    locked_until TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at TIMESTAMPTZ
);

-- Partial Index cực kỳ quan trọng: Chỉ đánh chỉ mục các bản tin đang chờ gửi.
-- Giúp truy vấn tìm bản tin PENDING đạt O(log N) với N chỉ là số bản tin chưa xử lý,
-- hoàn toàn không bị suy giảm hiệu năng khi bảng đã tích lũy hàng triệu bản ghi cũ.
CREATE INDEX IF NOT EXISTS idx_outbox_pending_relay
ON outbox_events (next_retry_at, created_at)
WHERE status = 'PENDING';

-- Index phục vụ kiểm tra trạng thái và truy vết sự cố theo Aggregate
CREATE INDEX IF NOT EXISTS idx_outbox_aggregate
ON outbox_events (aggregate_type, aggregate_id);

-- Index phục vụ tiến trình dọn dẹp (Retention Pruning)
CREATE INDEX IF NOT EXISTS idx_outbox_cleanup
ON outbox_events (published_at)
WHERE status = 'PUBLISHED';
```

### 7.2. Vòng đời Trạng thái Event (Outbox State Machine)

```mermaid
stateDiagram-v2
    [*] --> PENDING: Inserted atomically with Business Entity in local Tx

    state PENDING {
        [*] --> WaitingForPoll
        WaitingForPoll --> DispatchedToRelay: Acquired by Relay via SKIP LOCKED
    }

    PENDING --> PROCESSING: Worker acquires lock (locked_until set)

    PROCESSING --> PUBLISHED: Broker ACK confirmed
    PROCESSING --> PENDING: Broker error, retry_count < max_retries (Backoff set)
    PROCESSING --> FAILED: retry_count >= max_retries (Terminal error)

    FAILED --> DEAD: Moved to DLQ / Manual intervention

    PUBLISHED --> [*]: Hard deleted or Pruned by retention worker
    DEAD --> [*]: Audited and resolved by Platform team
```

- **`PENDING`:** Trạng thái ban đầu sau khi transaction nghiệp vụ `COMMIT`. Đang chờ Outbox Relay kéo đi gửi.
- **`PROCESSING`:** Đã được một luồng worker khóa thông qua `locked_until` để chuẩn bị bắn sang Message Broker.
- **`PUBLISHED`:** Broker đã xác nhận tiếp nhận thành công (Publisher Confirm / ACK). Bản tin sẵn sàng cho dọn dẹp.
- **`FAILED`:** Quá trình gửi sang Broker thất bại vượt quá `max_retries`. Đánh dấu để điều tra sự cố.
- **`DEAD`:** Bản tin đã được chuyển tiếp sang Dead Letter Topic hoặc cần can thiệp thủ công.

### 7.3. Atomic Recording API tích hợp `UnitOfWork`

Để loại bỏ hoàn toàn khả năng người dùng gọi nhầm ghi outbox ngoài transaction, API của `OutboxRepository` bắt buộc phải nhận vào tham chiếu mutable của `UnitOfWork`:

```rust
use omni_database::{transaction::UnitOfWork, DbError};
use serde::Serialize;
use uuid::Uuid;

/// Thực thể lưu trữ trong bảng outbox_events.
#[derive(Debug, Clone)]
pub struct OutboxRecord {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub destination_topic: String,
    pub partition_key: Option<String>,
    pub payload: serde_json::Value,
    pub headers: serde_json::Value,
}

pub struct OutboxRepository;

impl OutboxRepository {
    /// Ghi nhận CloudEvent vào bảng `outbox_events` bên trong UnitOfWork hiện tại.
    pub async fn record<T: Serialize>(
        uow: &mut UnitOfWork,
        topic: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        event: CloudEvent<T>,
    ) -> Result<Uuid, DbError> {
        let topic = topic.into();
        let aggregate_type = aggregate_type.into();
        let aggregate_id = aggregate_id.into();
        let partition_key = event.extensions.partition_key.clone();

        let payload_json = serde_json::to_value(&event)
            .map_err(|e| DbError::SerializationError(e.to_string()))?;
        let headers_json = serde_json::to_value(&event.extensions)
            .map_err(|e| DbError::SerializationError(e.to_string()))?;

        let event_id = event.id;

        // Câu lệnh INSERT chuẩn hóa thực thi trên connection của UnitOfWork
        sqlx::query!(
            r#"
            INSERT INTO outbox_events (
                id, aggregate_type, aggregate_id, event_type,
                destination_topic, partition_key, payload, headers,
                status, created_at, next_retry_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'PENDING', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
            event_id,
            aggregate_type,
            aggregate_id,
            event.event_type,
            topic,
            partition_key,
            payload_json,
            headers_json
        )
        .execute(uow.conn())
        .await
        .map_err(DbError::from)?;

        Ok(event_id)
    }
}
```

### 7.4. Hybrid Outbox Relay Worker (`SKIP LOCKED` + `LISTEN/NOTIFY`)

Để đạt thông lượng hàng chục nghìn event mỗi giây với độ trễ cận zero (Near-Zero Latency) mà không gây quá tải CPU do Polling liên tục, Outbox Relay kết hợp 2 kỹ thuật:

```mermaid
sequenceDiagram
    autonumber
    participant App as Domain Business Service
    participant PG as PostgreSQL (outbox_events)
    participant Relay as Omni-Messaging Outbox Relay
    participant Broker as Message Broker (RabbitMQ/Kafka)

    App->>PG: BEGIN Transaction
    App->>PG: INSERT INTO orders VALUES (...)
    App->>PG: INSERT INTO outbox_events (status='PENDING')
    App->>PG: NOTIFY omni_outbox_tick, 'new_event'
    App->>PG: COMMIT Transaction

    Note over PG,Relay: LISTEN/NOTIFY đánh thức Relay Worker ngay tức thì!
    PG-->>Relay: Notification("omni_outbox_tick")

    Relay->>PG: SELECT ... FOR UPDATE SKIP LOCKED LIMIT 100
    PG-->>Relay: Trả về batch 100 records (Locked)

    Relay->>Broker: Batch Publish (AMQP Confirm / Kafka Producer)
    Broker-->>Relay: Batch ACK Received

    alt CleanupStrategy == HardDelete
        Relay->>PG: DELETE FROM outbox_events WHERE id = ANY($ids)
    else CleanupStrategy == RetentionPruning
        Relay->>PG: UPDATE outbox_events SET status='PUBLISHED', published_at=NOW() WHERE id = ANY($ids)
    end
```

#### Thuật toán Polling với `FOR UPDATE SKIP LOCKED`:

```sql
-- Lấy lô bản tin và khóa lại, bỏ qua các bản ghi đang bị luồng worker khác khóa.
-- Hoàn toàn không xảy ra Deadlock hay Block Thread giữa nhiều Pod Relay!
SELECT
    id, destination_topic, partition_key, payload, headers, retry_count, max_retries
FROM outbox_events
WHERE status = 'PENDING'
  AND next_retry_at <= CURRENT_TIMESTAMP
  AND (locked_until IS NULL OR locked_until < CURRENT_TIMESTAMP)
ORDER BY created_at ASC
LIMIT $1
FOR UPDATE SKIP LOCKED;
```

#### Triển khai Loop Worker chịu lỗi trong Rust:

```rust
pub async fn run_relay_loop(
    pool: omni_database::DbPool,
    broker: Arc<dyn MessagePublisher>,
    config: OutboxConfig,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    let mut ticker = tokio::time::interval(config.poll_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Lắng nghe kênh NOTIFY từ PostgreSQL
    let mut listener = match sqlx::postgres::PgListener::connect_with(&pool.inner()).await {
        Ok(mut l) => {
            let _ = l.listen(&config.notify_channel).await;
            Some(l)
        }
        Err(e) => {
            tracing::warn!("Failed to establish PgListener for Outbox notifications: {e}. Falling back to pure polling.");
            None
        }
    };

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                tracing::info!("Outbox relay received shutdown signal. Exiting loop.");
                break;
            }
            _ = ticker.tick() => {
                // Quét định kỳ bảo đảm không sót bản tin
                if let Err(e) = dispatch_batch(&pool, broker.as_ref(), &config).await {
                    tracing::error!("Error dispatching outbox batch: {e:?}");
                }
            }
            notification = async {
                match listener.as_mut() {
                    Some(l) => l.recv().await,
                    None => futures::future::pending().await,
                }
            } => {
                match notification {
                    Ok(_notif) => {
                        // Thức tỉnh ngay khi có event mới commit
                        if let Err(e) = dispatch_batch(&pool, broker.as_ref(), &config).await {
                            tracing::error!("Error dispatching outbox batch on notify: {e:?}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("PgListener connection lost: {e}. Poller will continue on ticker.");
                    }
                }
            }
        }
    }
}
```

---

## 8. Pluggable Broker Transport Layer (`omni-messaging::broker`)

### 8.1. Unified Publisher & Consumer Traits

Lớp truyền dẫn tách rời hoàn toàn khỏi mã nguồn nghiệp vụ bằng hai Rust Traits:

```rust
use async_trait::async_trait;
use crate::error::MessagingError;

/// Đại diện cho một thông điệp thô trên đường truyền Broker.
#[derive(Debug, Clone)]
pub struct RawBrokerMessage {
    pub topic: String,
    pub partition_key: Option<String>,
    pub payload: Vec<u8>,
    pub headers: std::collections::HashMap<String, String>,
}

/// Trait xuất bản thông điệp ra Message Broker.
#[async_trait]
pub trait MessagePublisher: Send + Sync {
    /// Xuất bản một thông điệp đơn lẻ.
    async fn publish(&self, message: RawBrokerMessage) -> Result<(), MessagingError>;

    /// Xuất bản một mẻ thông điệp (Batch Publishing) với xác nhận hàng loạt.
    async fn publish_batch(&self, messages: &[RawBrokerMessage]) -> Result<BatchPublishResult, MessagingError>;
}

/// Kết quả xuất bản theo lô.
#[derive(Debug, Clone)]
pub struct BatchPublishResult {
    pub successful_ids: Vec<uuid::Uuid>,
    pub failed_items: Vec<(uuid::Uuid, String)>,
}

/// Trait nhận thông điệp từ Message Broker.
#[async_trait]
pub trait MessageConsumer: Send + Sync {
    /// Khởi động việc đăng ký lắng nghe (Subscribe) trên topic chỉ định.
    async fn subscribe(
        &self,
        topic: &str,
        consumer_group: &str,
        handler: Arc<dyn RawMessageHandler>,
    ) -> Result<(), MessagingError>;
}

/// Handler xử lý thông điệp thô trước khi chuyển sang Typed Handler.
#[async_trait]
pub trait RawMessageHandler: Send + Sync {
    async fn handle_raw(&self, message: RawBrokerMessage) -> Result<(), MessagingError>;
}
```

### 8.2. So sánh và Chiến lược Adapter Message Brokers

```mermaid
flowchart LR
    Publisher["Outbox Relay / Publisher"] --> Trait["Trait: MessagePublisher"]

    Trait --> AMQP["RabbitMQ Driver<br/>(Lapin AMQP 0-9-1)<br/>• Topic Exchanges<br/>• Quorum Queues<br/>• Confirm Acks"]
    Trait --> KAFKA["Kafka Driver<br/>(Rdkafka)<br/>• Partition Hashing<br/>• Consumer Groups<br/>• Exact Partition Ordering"]
    Trait --> NATS["NATS JetStream Driver<br/>(async-nats)<br/>• Subject Hierarchies<br/>• Ultra-low latency<br/>• Key-Value Stream"]
    Trait --> MEM["In-Memory Driver<br/>(Tokio Channels)<br/>• Zero dependencies<br/>• Fast Unit/CI Tests<br/>• Deterministic verification"]
```

| Tiêu chí                     | RabbitMQ (AMQP 0-9-1)                                    | Apache Kafka / Redpanda                           | NATS JetStream                               | In-Memory Driver                                |
| :--------------------------- | :------------------------------------------------------- | :------------------------------------------------ | :------------------------------------------- | :---------------------------------------------- |
| **Mô hình định tuyến**       | Topic Exchange + Binding Keys (`#`, `*`)                 | Partitioned Topic + Consumer Groups               | Subject Namespaces (`omni.events.>`)         | In-process Broadcast Channels                   |
| **Bảo toàn thứ tự**          | Trong phạm vi 1 Queue (Single Consumer)                  | Bảo toàn tuyệt đối theo Partition Key             | Bảo toàn theo Stream Subject                 | FIFO trong channel                              |
| **Độ trễ P99**               | Thấp (~2-5ms)                                            | Trung bình (~5-15ms) do Batching                  | Cực thấp (<1ms)                              | Tức thì (<0.1ms)                                |
| **Xử lý Dead Letter Queue**  | Hỗ trợ gốc qua DLX (Dead Letter Exchange)                | Định tuyến sang topic riêng (vd: `.dlq`)          | Định tuyến sang DLQ Subject                  | Lưu vào danh sách Vector kiểm thử               |
| **Tình huống sử dụng chính** | Giao tiếp Microservices nghiệp vụ chuẩn, RPC bất đồng bộ | Stream dữ liệu lớn, Event Sourcing, Data Pipeline | Microservices hiệu năng cao, K8s native mesh | Unit Test, Integration Test không cần container |

---

## 9. Distributed Tracing & Context Propagation

### 9.1. Chuỗi Truy vết Phân tán Xuyên Biên giới Mạng

Một trong những sai lầm phổ biến nhất khi chuyển đổi sang Event-Driven Architecture là làm đứt gãy chuỗi APM Trace. Khi một HTTP Request kích hoạt phát sinh Event, nếu không tiêm vết W3C, Trace trên Grafana Tempo sẽ kết thúc ngay tại điểm commit DB của Service phát hành. Service tiêu thụ (Consumer) sẽ sinh ra một Trace ID hoàn toàn độc lập, khiến DevOps không thể xâu chuỗi sự cố (Broken Trace Graph).

`omni-messaging` giải quyết triệt để bài toán này qua mô hình **W3C Trace Carrier Injection & Extraction**:

```mermaid
sequenceDiagram
    autonumber
    actor Client as End User
    participant Gateway as API Gateway
    participant OrderSvc as Order Service
    participant Outbox as outbox_events
    participant Broker as Message Broker
    participant PaymentSvc as Payment Service

    Client->>Gateway: POST /api/v1/orders (trace_id = 4bf92f...)
    Gateway->>OrderSvc: Forward with traceparent header
    Note over OrderSvc: Tạo Span: "create_order"<br/>(Kind: Server)
    OrderSvc->>Outbox: INSERT (Payload + headers["traceparent"])
    OrderSvc-->>Gateway: 201 Created
    Gateway-->>Client: HTTP 201 Created

    Note over Outbox,Broker: Outbox Relay trích xuất traceparent từ DB headers
    Outbox->>Broker: Publish Message (Headers: traceparent=4bf92f...)

    Broker->>PaymentSvc: Consume Message
    Note over PaymentSvc: Trích xuất traceparent từ Broker Header!<br/>Tạo Span: "process_order_event"<br/>(Parent: create_order, Kind: Consumer)
    PaymentSvc->>PaymentSvc: Thực thi khấu trừ thanh toán
    Note over Client,PaymentSvc: Toàn bộ quy trình thuộc chung 1 Distributed Trace ID duy nhất!
```

### 9.2. Triển khai W3C Carrier Injection (Publisher Side)

```rust
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub fn inject_trace_context(headers: &mut std::collections::HashMap<String, String>) {
    let current_span = Span::current();
    let cx = current_span.context();

    // Tiêm W3C traceparent và tracestate vào map headers
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut HeaderInjector(headers));
    });
}

struct HeaderInjector<'a>(&'a mut std::collections::HashMap<String, String>);

impl<'a> opentelemetry::propagation::Injector for HeaderInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}
```

### 9.3. Triển khai W3C Carrier Extraction (Consumer Side)

```rust
pub fn extract_trace_context(
    headers: &std::collections::HashMap<String, String>
) -> opentelemetry::Context {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(headers))
    })
}

struct HeaderExtractor<'a>(&'a std::collections::HashMap<String, String>);

impl<'a> opentelemetry::propagation::Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|s| s.as_str()).collect()
    }
}
```

---

## 10. Resilient Consumer Framework & Idempotent Inbox Pattern

### 10.1. Nghịch lý At-Least-Once Delivery & Duplicate Messages

Trong mọi mạng phân tán thực tế, cam kết của Message Broker luôn là **At-Least-Once Delivery**:

- Nếu Consumer xử lý xong nhưng bị rớt mạng ngay trước khi kịp gửi lệnh `Ack` về Broker: Broker sẽ nhận định worker bị chết và re-deliver thông điệp này sang một Pod khác.
- Nếu Consumer không có cơ chế bảo vệ lũy thừa (**Idempotency**), các thao tác nguy hiểm như trừ tiền, tích điểm, gửi email hay xuất kho sẽ bị thực hiện nhiều lần.

### 10.2. Kiến trúc Khử trùng Lặp 2 Lớp (Dual-Layer Deduplication)

`omni-messaging` thiết lập mô hình phòng thủ 2 lớp:

```mermaid
flowchart TD
    Msg["Incoming Broker Message<br/>(idempotency_key = UUIDv7)"] --> L1{"TẦNG 1: Fast Path (omni-cache)<br/>Redis SET key token NX EX ttl"}

    L1 -- "Key Đã Tồn Tại (Duplicate)" --> AckFast["Bỏ qua xử lý & Ack ngay lập tức<br/>(Tiết kiệm 100% tài nguyên DB)"]
    L1 -- "Key Mới (Lock Thành Công)" --> L2["TẦNG 2: Strong Consistency (omni-database)<br/>Mở Transaction & Ghi bảng inbox_events"]

    L2 --> CheckDB{"INSERT INTO inbox_events<br/>ON CONFLICT (id) DO NOTHING"}
    CheckDB -- "Bị trùng khóa (Conflict)" --> Rollback["Rollback Transaction & Ack Broker"]
    CheckDB -- "Ghi thành công" --> Exec["Thực thi Business Logic trong cùng Tx"]

    Exec -- "Thành công" --> Commit["COMMIT Transaction & Ack Broker"]
    Exec -- "Lỗi tạm thời" --> Retry["Rollback & Xóa Redis Key để Retry"]
```

#### Database Schema cho Bảng `inbox_events`:

```sql
-- Migration: V20261102_000002__create_inbox_events.sql

CREATE TABLE IF NOT EXISTS inbox_events (
    id UUID PRIMARY KEY,
    event_type VARCHAR(128) NOT NULL,
    source VARCHAR(256) NOT NULL,
    consumer_group VARCHAR(128) NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(20) NOT NULL DEFAULT 'SUCCESS',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index phục vụ dọn dẹp các bản ghi inbox cũ
CREATE INDEX IF NOT EXISTS idx_inbox_cleanup
ON inbox_events (created_at);
```

### 10.3. Resilient Retry Pipeline: Exponential Backoff with Full Jitter

Khi Consumer gặp sự cố (ví dụ: Database downstream tạm thời quá tải, network timeout), không được phép retry liên tục ngay lập tức (Immediate Retry) vì sẽ gây ra hiện tượng **Thundering Herd** làm sập hoàn toàn hệ thống downstream.

Hệ thống bắt buộc áp dụng thuật toán **Exponential Backoff with Full Jitter** theo nghiên cứu của AWS Architecture:

$$\text{sleep} = \text{random\_between}\left(0, \min\left(\text{max\_backoff}, \text{initial\_backoff} \times 2^{\text{attempt} - 1}\right)\right)$$

```rust
use rand::Rng;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub multiplier: f64,
    pub jitter: bool,
}

impl BackoffPolicy {
    /// Tính toán thời gian chờ ngủ dựa trên số lần thử lại hiện tại.
    #[must_use]
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let exp = f64::from(attempt.saturating_sub(1));
        let factor = self.multiplier.powf(exp);
        let millis = self.initial_backoff.as_millis() as f64 * factor;
        let capped = millis.min(self.max_backoff.as_millis() as f64);

        if self.jitter {
            let mut rng = rand::rng();
            let jittered = rng.random_range(0.0..=capped);
            Duration::from_millis(jittered as u64)
        } else {
            Duration::from_millis(capped as u64)
        }
    }
}
```

### 10.4. Poison Pill Detection & Dead Letter Queue (DLQ) Routing

**Poison Pill (Bản tin thuốc độc):** Là thông điệp bị lỗi nghiêm trọng không thể khắc phục bằng cách thử lại:

- Lỗi cú pháp JSON (Malformed JSON Payload).
- Dữ liệu vi phạm nghiêm trọng ràng buộc nghiệp vụ (ví dụ: số tiền thanh toán âm).
- Schema version không còn được hỗ trợ và không thể deserialize.

Khi phát hiện Poison Pill hoặc khi số lần thử lại vượt quá `max_retries`:

1. Consumer **ngừng retry**, đóng gói bản tin kèm bộ **Dead Letter Diagnostic Headers**.
2. Xuất bản bản tin sang **DLQ Topic** (ví dụ: `order.events.dlq`).
3. Gửi lệnh `Ack` cho Broker đối với hàng đợi chính để giải phóng worker cho các bản tin hợp lệ tiếp theo.
4. Ghi nhận metric cảnh báo `messaging_dlq_routed_total` tăng lên và log cảnh báo mức `ERROR`.

```rust
/// Các Header chẩn đoán được tiêm vào bản tin khi định tuyến sang DLQ.
pub struct DeadLetterHeaders;

impl DeadLetterHeaders {
    pub const REASON: &'static str = "x-omni-death-reason";
    pub const ORIGINAL_TOPIC: &'static str = "x-omni-original-topic";
    pub const FAILURE_COUNT: &'static str = "x-omni-failure-count";
    pub const FAILED_AT: &'static str = "x-omni-failed-at";
    pub const STACK_TRACE: &'static str = "x-omni-stack-trace";
}
```

### 10.5. Declarative Message Handler Trait

Developer khi viết microservice chỉ cần hiện thực hóa trait `MessageHandler<T>` cực kỳ tinh gọn:

```rust
use async_trait::async_trait;
use crate::cloudevent::CloudEvent;
use crate::error::MessageProcessingError;

/// Ngữ cảnh cung cấp cho Handler trong quá trình xử lý.
#[derive(Debug, Clone)]
pub struct MessageContext {
    pub topic: String,
    pub partition_key: Option<String>,
    pub delivery_attempt: u32,
    pub trace_id: String,
}

/// Trait nghiệp vụ khai báo cho Consumer.
#[async_trait]
pub trait MessageHandler<T: Send + Sync + 'static>: Send + Sync {
    /// Hàm xử lý nghiệp vụ chính.
    /// Nếu trả về `Ok(())`, framework tự động Ack.
    /// Nếu trả về `Err(Transient)`, framework kích hoạt Exponential Backoff.
    /// Nếu trả về `Err(Terminal)`, framework lập tức chuyển sang DLQ.
    async fn handle(&self, ctx: MessageContext, event: CloudEvent<T>) -> Result<(), MessageProcessingError>;
}
```

---

## 11. Error Taxonomy & RFC 9457 Integration

### 11.1. Bảng Phân loại Mã lỗi Chi tiết (`MessagingError`)

Toàn bộ các lỗi phát sinh trong phân hệ `omni-messaging` được phân định rõ ràng về mặt ngữ nghĩa:

```rust
use thiserror::Error;

/// Bảng mã lỗi của phân hệ Messaging.
#[derive(Debug, Error)]
pub enum MessagingError {
    #[error("Broker connection failed: {0}")]
    BrokerConnectionFailed(String),

    #[error("Message publish timed out after {0:?}")]
    PublishTimeout(std::time::Duration),

    #[error("Broker rejected message: {0}")]
    PublishRejected(String),

    #[error("Failed to serialize CloudEvent: {0}")]
    SerializationFailed(String),

    #[error("Failed to deserialize CloudEvent: {0}")]
    DeserializationFailed(String),

    #[error("Database persistence error: {0}")]
    DatabaseError(#[from] omni_database::DbError),

    #[error("Cache deduplication error: {0}")]
    CacheError(String),

    #[error("Duplicate event detected with idempotency_key: {0}")]
    DuplicateEvent(String),

    #[error("Exceeded maximum retry attempts ({max_retries}). Routed to DLQ.")]
    DeadLetterThresholdExceeded {
        max_retries: u32,
        reason: String,
    },

    #[error("Unrecoverable poison pill detected: {0}")]
    PoisonPill(String),

    #[error("Outbox lock contention on record: {0}")]
    LockContention(uuid::Uuid),
}

/// Phân loại lỗi xử lý bản tin từ phía Consumer Handler.
#[derive(Debug, Error)]
pub enum MessageProcessingError {
    /// Lỗi tạm thời (Transient): Có thể tự hồi phục bằng cách thử lại sau backoff.
    #[error("Transient error (will retry): {0}")]
    Transient(String),

    /// Lỗi vĩnh viễn (Terminal / Poison Pill): Không thể khắc phục, chuyển thẳng sang DLQ.
    #[error("Terminal error (will route to DLQ): {0}")]
    Terminal(String),
}
```

### 11.2. Ánh xạ sang `omni_errors::AppError`

Khi một API HTTP của service cần truy vấn hoặc kích hoạt lại các bản tin Outbox/DLQ (ví dụ: Admin Dashboard Retry API), `MessagingError` tự động chuyển đổi sang chuẩn **RFC 9457 Problem Details**:

| `MessagingError`                 | HTTP Status                 | RFC 9457 Type URI                                         |
| :------------------------------- | :-------------------------- | :-------------------------------------------------------- |
| `DuplicateEvent`                 | `409 Conflict`              | `https://api.omni.internal/errors/duplicate-event`        |
| `PoisonPill` / `Deserialization` | `422 Unprocessable Entity`  | `https://api.omni.internal/errors/unprocessable-event`    |
| `PublishTimeout` / `Connection`  | `503 Service Unavailable`   | `https://api.omni.internal/errors/broker-unavailable`     |
| `DeadLetterThresholdExceeded`    | `500 Internal Server Error` | `https://api.omni.internal/errors/dlq-threshold-exceeded` |

---

## 12. Observability, Telemetry & OpenTelemetry Semantic Conventions

### 12.1. OpenTelemetry Semantic Conventions v1.27+

Tất cả các Span do `omni-messaging` sinh ra đều gắn các thuộc tính chuẩn hóa:

| Thuộc tính OTel                 | Kiểu dữ liệu | Ví dụ / Giá trị chuẩn                    | Mô tả                               |
| :------------------------------ | :----------- | :--------------------------------------- | :---------------------------------- |
| `messaging.system`              | `string`     | `"rabbitmq"`, `"kafka"`, `"nats"`        | Định danh hệ thống message broker   |
| `messaging.destination.name`    | `string`     | `"omni.order.events.v1"`                 | Tên topic / exchange / queue đích   |
| `messaging.operation.type`      | `string`     | `"publish"`, `"receive"`, `"process"`    | Loại thao tác xử lý                 |
| `messaging.message.id`          | `string`     | `"018f4a2b-8a5e-7a1b-9c2d-3e4f5a6b7c8d"` | CloudEvent UUIDv7 ID                |
| `messaging.message.body.size`   | `int`        | `1024`                                   | Kích thước payload tính theo bytes  |
| `messaging.consumer.group.name` | `string`     | `"payment-service-group"`                | Tên nhóm consumer đang nhận bản tin |

### 12.2. Bộ Chỉ số Prometheus Chuẩn Hóa

```rust
// Bộ chỉ số Transactional Outbox (Publisher Side)
pub const METRIC_OUTBOX_QUEUE_DEPTH: &str = "messaging_outbox_queue_depth";
pub const METRIC_OUTBOX_PUBLISHED_TOTAL: &str = "messaging_outbox_published_total";
pub const METRIC_OUTBOX_PUBLISH_DURATION: &str = "messaging_outbox_publish_duration_seconds";
pub const METRIC_OUTBOX_RETRIES_TOTAL: &str = "messaging_outbox_retries_total";
pub const METRIC_OUTBOX_FAILED_TOTAL: &str = "messaging_outbox_failed_total";

// Bộ chỉ số Resilient Consumer (Consumer Side)
pub const METRIC_CONSUMER_RECEIVED_TOTAL: &str = "messaging_consumer_received_total";
pub const METRIC_CONSUMER_PROCESS_DURATION: &str = "messaging_consumer_process_duration_seconds";
pub const METRIC_CONSUMER_ERRORS_TOTAL: &str = "messaging_consumer_errors_total";
pub const METRIC_CONSUMER_RETRIES_TOTAL: &str = "messaging_consumer_retries_total";
pub const METRIC_CONSUMER_DEDUP_HITS_TOTAL: &str = "messaging_consumer_dedup_hits_total";
pub const METRIC_DLQ_ROUTED_TOTAL: &str = "messaging_dlq_routed_total";
```

### 12.3. PII Sanitization trong Event Payloads

Tương tự như chính sách trong `omni-logging`, mọi thông tin cá nhân (Họ tên, Số điện thoại, Email, Số thẻ căn cước, Địa chỉ) chứa trong event payload khi được in ra log trong quá trình debug bắt buộc phải được quét qua `omni_logging::masking::PIISanitizer` để tuân thủ Nghị định 13/2023/NĐ-CP và GDPR.

---

## 13. Data Retention, Housekeeping & Archival Strategies

### 13.1. Nguy cơ Phình Bảng (Table Bloat) trong PostgreSQL

Trong một hệ thống xử lý 500 requests/giây, bảng `outbox_events` và `inbox_events` sẽ tích lũy hơn **43 triệu bản ghi mỗi ngày**. Nếu không có chiến lược dọn dẹp chủ động:

- Dung lượng ổ đĩa sẽ cạn kiệt nhanh chóng.
- Chỉ mục (B-Tree Indexes) bị phân mảnh nặng nề.
- Tiến trình PostgreSQL Autovacuum bị bão hòa, làm suy giảm hiệu năng toàn bộ cơ sở dữ liệu.

### 13.2. Ba Chiến lược Quản lý Vòng đời Bản ghi

```mermaid
flowchart TD
    Strategy{"LỰA CHỌN CHIẾN LƯỢC RETENTION"}

    Strategy --> S1["1. Hard-Delete Ngay Khi Ack<br/>(DELETE WHERE id = ANY($ids))<br/>• Ưu điểm: Bảng luôn có 0-100 dòng, không bao giờ phình to.<br/>• Nhược điểm: Mất lịch sử event tại tầng DB (phải xem ở Broker log)."]
    Strategy --> S2["2. Retention Pruning Worker (Mặc định Khuyến nghị)<br/>(Cập nhật status='PUBLISHED', Worker định kỳ dọn sau 72h)<br/>• Ưu điểm: Giữ lại vài ngày để điều tra sự cố, DBA kiểm tra đối soát.<br/>• Nhược điểm: Cần index idx_outbox_cleanup và worker định kỳ."]
    Strategy --> S3["3. Time-Range Partitioning<br/>(PARTITION BY RANGE (created_at))<br/>• Ưu điểm: Xóa hàng triệu dòng trong 1ms bằng DROP TABLE partition.<br/>• Nhược điểm: Phức tạp trong việc quản lý partition tự động."]
```

`omni-messaging` cung cấp sẵn struct `OutboxRetentionWorker` chạy nền định kỳ để thực thi dọn dẹp an toàn theo mẻ (Chunked Deletion) nhằm tránh khóa bảng lâu:

```sql
-- Dọn dẹp theo mẻ 5000 dòng để không làm nghẽn replication stream và WAL log
DELETE FROM outbox_events
WHERE id IN (
    SELECT id FROM outbox_events
    WHERE status = 'PUBLISHED'
      AND published_at < CURRENT_TIMESTAMP - INTERVAL '72 hours'
    LIMIT 5000
);
```

---

## 14. Implementation Blueprint & Crate Layout (`libs/messaging`)

### 14.1. Cấu trúc Cây Thư mục Mã nguồn Chuẩn

```text
libs/messaging/
├── Cargo.toml
├── README.md
├── tests/
│   ├── cloudevent_tests.rs
│   ├── outbox_repository_tests.rs
│   ├── retry_backoff_tests.rs
│   ├── w3c_propagation_tests.rs
│   ├── memory_broker_tests.rs
│   └── integration_outbox_relay.rs
└── src/
    ├── lib.rs
    ├── config.rs
    ├── error.rs
    ├── test_utils.rs
    ├── cloudevent/
    │   ├── mod.rs
    │   ├── event.rs
    │   ├── builder.rs
    │   ├── extensions.rs
    │   └── serde_mode.rs
    ├── outbox/
    │   ├── mod.rs
    │   ├── model.rs
    │   ├── repository.rs
    │   ├── relay.rs
    │   ├── listener.rs
    │   └── cleanup.rs
    ├── inbox/
    │   ├── mod.rs
    │   ├── model.rs
    │   ├── repository.rs
    │   └── deduplicator.rs
    ├── broker/
    │   ├── mod.rs
    │   ├── publisher.rs
    │   ├── consumer.rs
    │   ├── message.rs
    │   ├── memory.rs
    │   ├── rabbitmq.rs
    │   ├── kafka.rs
    │   └── nats.rs
    ├── consumer/
    │   ├── mod.rs
    │   ├── handler.rs
    │   ├── pipeline.rs
    │   ├── backoff.rs
    │   └── dlq.rs
    ├── tracing/
    │   ├── mod.rs
    │   ├── carrier.rs
    │   └── extractor.rs
    └── instrument/
        ├── mod.rs
        └── metrics.rs
```

### 14.2. Khai báo `libs/messaging/Cargo.toml` Hoàn chỉnh

```toml
[package]
name = "omni-messaging"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
# Nền tảng chia sẻ nội bộ workspace
omni-common = { workspace = true }
omni-errors = { workspace = true }
omni-logging = { workspace = true }
omni-metrics = { workspace = true }
omni-database = { workspace = true, optional = true }
omni-cache = { workspace = true, optional = true }

# Asynchronous Runtime & Synchronization
tokio = { workspace = true, features = ["full"] }
tokio-util = { workspace = true }
async-trait = { workspace = true }
futures = { workspace = true }
parking_lot = { workspace = true }

# Serialization & CloudEvents
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
uuid = { workspace = true, features = ["v4", "v7", "serde"] }
chrono = { workspace = true, features = ["serde", "clock"] }

# Configuration & Validation
secrecy = { workspace = true, features = ["serde"] }
validator = { workspace = true, features = ["derive"] }
thiserror = { workspace = true }
rand = { workspace = true }

# Database (PostgreSQL)
sqlx = { workspace = true, optional = true, features = [
    "runtime-tokio",
    "tls-rustls",
    "postgres",
    "chrono",
    "uuid",
    "json",
    "macros",
] }

# Observability & Distributed Tracing
tracing = { workspace = true }
tracing-opentelemetry = { workspace = true }
opentelemetry = { workspace = true }
metrics = { workspace = true }

# Optional Message Broker Drivers
lapin = { version = "2.5", default-features = false, features = ["rustls"], optional = true }
rdkafka = { version = "0.37", features = ["tokio"], optional = true }
async-nats = { version = "0.38", optional = true }

[dev-dependencies]
testcontainers = "0.23"
tokio-test = "0.4"

[features]
default = ["postgres", "cache-dedup", "rabbitmq"]

postgres = ["dep:omni-database", "dep:sqlx"]
cache-dedup = ["dep:omni-cache"]

in-memory = []
rabbitmq = ["dep:lapin"]
kafka = ["dep:rdkafka"]
nats = ["dep:async-nats"]

test-utils = ["in-memory"]
```

---

## 15. Testing Strategy & Verification Matrix

Để đạt chất lượng chuẩn doanh nghiệp, bộ kiểm thử của `omni-messaging` bao phủ 4 cấp độ:

```mermaid
flowchart TD
    subgraph TestingMatrix["4 TẦNG KIỂM THỬ TOÀN DIỆN"]
        T1["Tầng 1: Unit Tests (Pure Rust, Zero Network)<br/>• CloudEvent serialization/deserialization<br/>• Exponential Backoff & Jitter math distributions<br/>• W3C carrier injection & extraction headers"]
        T2["Tầng 2: In-Memory Integration Tests<br/>• Đăng ký Consumer với MemoryBroker<br/>• Kiểm tra xử lý song song và Ack/Nack semantics<br/>• Mô phỏng điều hướng DLQ khi gặp lỗi Terminal"]
        T3["Tầng 3: Database & Transactional Outbox Tests<br/>• Atomic Rollback: Huỷ Tx nghiệp vụ -> outbox_events không được ghi<br/>• SKIP LOCKED: Chạy 5 worker song song không tranh chấp khóa<br/>• LISTEN/NOTIFY trigger thức tỉnh tức thì"]
        T4["Tầng 4: Chaos & End-to-End Testcontainers<br/>• Rớt mạng Broker đột ngột giữa chừng khi Publish<br/>• Replay Attack: Gửi cùng 1 message 100 lần -> Chỉ thực thi 1 lần<br/>• Bão Poison Pill không làm nghẽn hàng đợi chính"]
    end
```

### Kịch bản Kiểm thử Tính Lũy thừa & Khử trùng lặp (Idempotency Simulation):

```rust
#[tokio::test]
async fn test_idempotent_consumer_suppresses_duplicate_messages() {
    // 1. Dựng test container Redis và Postgres
    // 2. Tạo một CloudEvent với idempotency_key cố định
    // 3. Gửi cùng event đó 10 lần liên tiếp vào consumer pipeline
    // 4. Assert: Business handler chỉ được kích hoạt ĐÚNG 1 LẦN DUY NHẤT
    // 5. Assert: Metric messaging_consumer_dedup_hits_total tăng chính xác 9 lần
    // 6. Assert: Toàn bộ 10 lần gọi đều được Ack về broker để không bị nghẽn
}
```

---

## 16. The Golden Path Integration (Mẫu Ứng dụng Thực tế)

### 16.1. Service Đặt hàng (`services/order`) — Atomic Outbox Publisher

```rust
// services/order/src/service.rs
use omni_common::middleware::RequestContext;
use omni_database::transaction::with_transaction;
use omni_messaging::{
    cloudevent::CloudEventBuilder,
    outbox::OutboxRepository,
};
use uuid::Uuid;

pub struct OrderService {
    db: omni_database::DbPool,
}

impl OrderService {
    pub async fn create_order(
        &self,
        ctx: &RequestContext,
        customer_id: Uuid,
        amount: f64,
    ) -> Result<Uuid, omni_errors::AppError> {
        let order_id = Uuid::now_v7();

        // Thực thi toàn bộ thao tác trong 1 Transaction duy nhất!
        with_transaction(&self.db, |mut uow| async move {
            // 1. Ghi nhận thực thể Đơn hàng vào bảng orders
            sqlx::query!(
                "INSERT INTO orders (id, customer_id, amount, status) VALUES ($1, $2, $3, 'CREATED')",
                order_id, customer_id, amount
            )
            .execute(uow.conn())
            .await?;

            // 2. Đóng gói sự kiện chuẩn CloudEvents v1.0.2 kèm W3C Trace Context
            let event = CloudEventBuilder::new(
                "urn:omni:service:order",
                "com.omni.order.created.v1",
                serde_json::json!({
                    "order_id": order_id,
                    "customer_id": customer_id,
                    "amount": amount,
                }),
            )
            .subject(order_id.to_string())
            .partition_key(order_id.to_string())
            .with_request_context(ctx)
            .build();

            // 3. Ghi Outbox Event cùng Transaction
            OutboxRepository::record(
                &mut uow,
                "omni.order.events.v1",
                "order",
                order_id.to_string(),
                event,
            )
            .await?;

            Ok(order_id)
        })
        .await
        .map_err(Into::into)
    }
}
```

### 16.2. Service Thanh toán (`services/payment`) — Idempotent Consumer

```rust
// services/payment/src/consumer.rs
use async_trait::async_trait;
use omni_messaging::{
    cloudevent::CloudEvent,
    consumer::{MessageHandler, MessageContext},
    error::MessageProcessingError,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct OrderCreatedPayload {
    pub order_id: Uuid,
    pub customer_id: Uuid,
    pub amount: f64,
}

pub struct OrderEventsConsumer {
    payment_processor: PaymentProcessor,
}

#[async_trait]
impl MessageHandler<OrderCreatedPayload> for OrderEventsConsumer {
    async fn handle(
        &self,
        ctx: MessageContext,
        event: CloudEvent<OrderCreatedPayload>,
    ) -> Result<(), MessageProcessingError> {
        tracing::info!(
            order_id = %event.data.order_id,
            amount = event.data.amount,
            "Received OrderCreated event. Processing payment authorization..."
        );

        // Khấu trừ thanh toán nghiệp vụ (đã được bọc Idempotency tự động ở tầng khung)
        self.payment_processor
            .authorize_charge(event.data.customer_id, event.data.amount)
            .await
            .map_err(|e| {
                if e.is_network_timeout() {
                    MessageProcessingError::Transient(e.to_string())
                } else {
                    MessageProcessingError::Terminal(e.to_string())
                }
            })?;

        tracing::info!(order_id = %event.data.order_id, "Payment authorized successfully.");
        Ok(())
    }
}
```

---

## 17. Quality Gate Checklist (Definition of Done)

Theo quy định bắt buộc tại [`AGENTS.md`](file://AGENTS.md) và [`.agents/rules/quality-gate.md`](file://.agents/rules/quality-gate.md), trước khi chuyển sang giai đoạn code và nghiệm thu hoàn tất `omni-messaging`:

1. **Chuẩn hóa định dạng mã nguồn:**

   ```bash
   cargo fmt --all
   ```

2. **Đồng bộ cây phụ thuộc workspace:**

   ```bash
   cargo hakari generate
   ```

3. **Kiểm tra Linter nghiêm ngặt (Zero warnings allowed):**

   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

4. **Vượt qua 100% bộ kiểm thử tự động:**

   ```bash
   cargo test --all
   ```

5. **Cập nhật tài liệu thay đổi:** Ghi nhận mốc bàn giao kiến trúc vào mục `[Unreleased]` trong [`CHANGELOG.md`](file://CHANGELOG.md).
