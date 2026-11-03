# OmniKubernetes Architecture Specification: Microservice Scaffold & Local Development Harness Standard (`omni-common::bootstrap`, Dev Harness & Testcontainers)

- **Document Version:** `1.1.0`
- **Status:** Approved / Enterprise Architecture Standard
- **Author:** System Architecture Team
- **Roadmap Step:** Giai đoạn 4 — Bước 9: Bộ khung Service mẫu & Môi trường phát triển cục bộ ([`RoadMap.md`](../../RoadMap.md))
- **Target Platform:** Rust (2024 Edition, **Rust 1.94+**), Tokio Runtime (1.43+), Axum Framework (0.8+), Tower (0.5+), Tower HTTP (0.7+), PostgreSQL 16+, Redis OSS 7.2+ / Valkey 8+, RabbitMQ 3.13+, OpenTelemetry / Grafana Stack (Loki, Tempo, Prometheus), Docker Compose v2.24+, Process Compose 1.0+, Testcontainers 0.16+, Kubernetes 1.30+.

---

## 1. Executive Summary & Vấn Đề Kiến Trúc (Architectural Problem Statement)

### 1.1. Bối cảnh & Mảnh ghép Cuối cùng của BaseCode

Trải qua 8 bước nền tảng vững chắc trong lộ trình [`RoadMap.md`](../../RoadMap.md), hệ sinh thái **OmniKubernetes** đã hoàn thiện toàn bộ các trụ cột cốt lõi:

1. Chuẩn hóa phong bì phản hồi dữ liệu kép và đa ngôn ngữ RCU lock-free (`omni-common::response`, `omni-common::i18n`).
2. Cấu hình phân tầng 5 lớp chống rò rỉ bí mật bộ nhớ (`omni-common::config`).
3. Chuẩn hóa nhật ký phi phong tỏa ECS/OTel và bối cảnh phân tán (`omni-logging`, `omni-common::middleware`).
4. Tầng dữ liệu quan hệ với cơ chế quản trị kết nối SQLx tự tối ưu và migrations tích hợp (`omni-database`).
5. Bộ nhớ đệm phân tán 2 tầng, khóa phân tán Redlock và chống giẫm đạp cache (`omni-cache`).
6. Hệ thống xác thực danh tính JWT, phân quyền RBAC và Claims chuẩn doanh nghiệp (`omni-auth`).
7. Cơ chế truyền thông bất đồng bộ dựa trên sự kiện, CloudEvents CNCF và Transactional Outbox / Idempotent Inbox (`omni-messaging`).
8. Khả năng phục hồi mạng, chống bão thử lại, cắt tải và hạ cánh an toàn 7 giai đoạn (`omni-common::resilience`).
9. Tự động hóa tài liệu API tập trung và phân giải động đường dẫn Ingress (`omni-common::openapi`).

Tuy nhiên, **một hệ sinh thái thư viện mạnh mẽ sẽ trở nên vô nghĩa nếu thiếu đi một cơ chế lắp ghép chuẩn hóa, nhất quán và công thái học (ergonomic)**. Khi một kỹ sư bắt đầu xây dựng một Domain Service mới (như `services/order`, `services/payment`, `services/auth`), nếu không có một **Bộ khung Service mẫu (Scaffold)**, **Nguyên tắc Cô lập Dữ liệu & Tài nguyên Tuyệt đối**, và **Môi trường phát triển cục bộ chuẩn mực (Dev Harness)**, dự án sẽ ngay lập tức đối mặt với những rủi ro kiến trúc nghiêm trọng.

```mermaid
flowchart TD
    subgraph FoundationLibs["8 NỀN TẢNG ĐÃ HOÀN THIỆN"]
        direction TB
        L1["omni-common<br/>(Response, Config, I18n)"]
        L2["omni-logging<br/>(ECS, OTel, Tracing)"]
        L3["omni-errors<br/>(RFC 9457 Problem Details)"]
        L4["omni-database<br/>(SQLx, Migrations)"]
        L5["omni-cache<br/>(Redis, Locks, Cache-Aside)"]
        L6["omni-auth<br/>(JWT, Claims, RBAC)"]
        L7["omni-messaging<br/>(CloudEvents, Outbox/Inbox)"]
        L8["omni-metrics & resilience<br/>(RED Metrics, Traffic Control)"]
    end

    subgraph Step9["BƯỚC 9: BỘ KHUNG SERVICE MẪU & DEV HARNESS (MẢNH GHÉP KẾT NỐI)"]
        direction TB
        Isolation["Data & Resource Isolation Pattern<br/>(Database-per-Service & Cache-per-Service)"]
        AppBuilder["AppBuilder Pattern<br/>(9-Layer Middleware Onion)"]
        ServerRuntime["Production HTTP Server<br/>(7-Stage K8s Graceful Shutdown)"]
        DevHarness["Local Multi-Service Compose Harness<br/>(Docker / Process Compose)"]
        TestHarness["Automated Integration Testing<br/>(Testcontainers Isolation)"]
        Scaffold["Domain Service Scaffold<br/>(The Golden Path Template)"]
    end

    subgraph DomainServices["CÁC DOMAIN SERVICES (services/*)"]
        direction LR
        S1["services/auth"]
        S2["services/order"]
        S3["services/payment"]
        S4["services/product"]
        S5["services/user"]
        S6["services/notification"]
    end

    FoundationLibs ==> Step9
    Step9 ==> DomainServices
```

---

### 1.2. 6 Thảm họa Kiến trúc khi thiếu Service Scaffold, Dev Harness & Phân lập Dữ liệu

Nếu không thiết lập tiêu chuẩn kiến trúc cho Bước 9, hệ thống sẽ rơi vào 6 cạm bẫy kinh điển sau:

1. **Bẫy Đảo lộn Thứ tự Middleware (Middleware Onion Inversion):**
   - Trong Axum và Tower, các tầng middleware bọc lấy nhau như các lớp vỏ củ hành (_onion architecture_). Việc áp dụng sai thứ tự tầng sẽ gây ra những lỗ hổng thầm lặng nhưng chết người:
     - Đặt Tracing sau Auth: Toàn bộ các request bị từ chối 401/403 sẽ hoàn toàn không có `trace_id` trong hệ thống log, làm tê liệt khả năng điều tra sự cố của SOC/DevOps.
     - Đặt Metrics bên ngoài Timeout: Các request bị ngắt kết nối do quá hạn sẽ không được ghi nhận vào biểu đồ độ trễ hoặc bị báo sai trạng thái phản hồi.
     - Đặt Catch Panic bên trong Router: Khi một worker thread bị panic, kết nối TCP bị ngắt đột ngột (drop connection) thay vì trả về RFC 9457 Problem Details `500 Internal Server Error`, khiến Client không thể phân biệt giữa lỗi mạng và lỗi ứng dụng.
     - Đặt Load Shedding sau Request Body Deserialization: Pod đã tốn hàng megabyte bộ nhớ và chu kỳ CPU để parse JSON payload lớn trước khi nhận ra hệ thống đã bão hòa và vứt bỏ request, làm mất hoàn toàn giá trị của nguyên lý _Fail Fast, Fail Cheap_.

2. **Thảm họa Cơ sở Dữ liệu Dùng chung & Vỡ vụn Ranh giới Dữ liệu (The Shared Database Anti-Pattern / Distributed Monolith):**
   - Khi các microservice kết nối chung vào một database hoặc thực hiện câu lệnh truy vấn chéo bảng (Cross-Database SQL JOIN), ranh giới dịch vụ (_bounded context_) bị phá hủy hoàn toàn. Một thay đổi schema của `order-service` có thể làm sập `payment-service` hoặc `auth-service`, biến hệ thống thành một **Distributed Monolith** mang toàn bộ nhược điểm của Monolith lẫn sự phức tạp của Microservices.
   - Tương tự với bộ nhớ đệm: Nếu các service chia sẻ một Redis chung không có cơ chế phân lập keyspace hoặc không có ACL, `order-service` có thể vô tình ghi đè (overwrite), xóa nhầm (`FLUSHDB`), hoặc giẫm đạp khóa phân tán của `auth-service`, tạo ra những lỗi dữ liệu ngầm cực kỳ nguy hiểm.

3. **Hội chứng Sao chép Khởi tạo (Boilerplate Fatigue & Code Drift):**
   - Mỗi Domain Service tự viết lại 150–300 dòng code khởi tạo tại `main.rs`: kết nối PostgreSQL, kết nối Redis, nạp Fluent i18n, dựng Signal Handler, gắn CORS, gắn Probes.
   - Khi một tiêu chuẩn an ninh hoặc một bản vá hạ tầng thay đổi (ví dụ: cập nhật header CSP hoặc chỉnh sửa thuật toán Drain Gate), đội ngũ phát triển phải mở hàng chục file `main.rs` để sửa thủ công, dẫn tới phân mảnh phiên bản (architecture drift).

4. **Hiện tượng Lệch pha Môi trường Cục bộ (Environment Drift & "Works on My Machine"):**
   - Nhà phát triển này cài PostgreSQL 14 trên macOS qua Homebrew, nhà phát triển khác dùng Redis 6 trên Linux, người khác lại chạy RabbitMQ không có dead-letter exchange plugin.
   - Các lỗi liên quan đến sai khác collation PostgreSQL, cú pháp Redis Lua scripting hoặc hành vi hàng đợi tin nhắn chỉ phát tác khi code được đẩy lên môi trường Staging/Production.

5. **Ảo tưởng Kiểm thử bằng Mock (The Mocking Fallacy & Flaky Tests):**
   - Sử dụng Mock cho Database và Redis trong các bài kiểm thử tích hợp: Các test case "xanh" 100% nhưng khi chạy thật thì sập do vi phạm ràng buộc Foreign Key, lỗi tương thích kiểu dữ liệu JSONB, hoặc deadlock câu lệnh SQL transaction isolation.
   - Hoặc ngược lại: Chạy test tích hợp dựa vào một database dùng chung cài sẵn trên máy. Khi chạy song song `cargo test --all`, các bài test ghi đè dữ liệu của nhau, gây ra hiện tượng _Flaky Tests_ (lúc pass lúc fail mà không rõ nguyên nhân).

6. **Phá vỡ Chu trình Vòng đời Kubernetes (Broken Container Lifecycle):**
   - Service container biên dịch cồng kềnh (1.5GB+), chạy dưới quyền `root`, không có PID 1 reaper (dẫn đến zombie processes), và bỏ qua tín hiệu `SIGTERM` của Kubelet, dẫn tới việc rơi rụng kết nối của người dùng mỗi khi Kubernetes thực hiện Rolling Update.

---

### 1.3. 11 Nguyên lý Thiết kế Cốt lõi (11 Core Architectural Tenets)

Tiêu chuẩn kiến trúc này xác lập **11 nguyên lý bắt buộc** chi phối toàn bộ mã nguồn của Bộ khung Service mẫu và Môi trường phát triển cục bộ:

```mermaid
flowchart TD
    subgraph Principles["11 NGUYÊN LÝ THIẾT KẾ SCAFFOLD & DEV HARNESS"]
        P1["1. Declarative Type-Safe AppBuilder<br/>(Khởi tạo ứng dụng trực quan, kiểm soát kiểu dữ liệu tĩnh)"]
        P2["2. Strict 9-Layer Middleware Onion<br/>(Cố định thứ tự bọc middleware, triệt tiêu lỗi logic bối cảnh)"]
        P3["3. Strict Data & Resource Autonomy<br/>(Database-per-Service & Cache-per-Service, cấm Cross-DB JOIN)"]
        P4["4. Zero-Boilerplate Golden Path<br/>(Service mới khởi tạo dưới 35 dòng mã nguồn tại main.rs)"]
        P5["5. Deterministic 7-Stage K8s Shutdown<br/>(Tích hợp mặc định hạ cánh an toàn, triệt tiêu 502 Bad Gateway)"]
        P6["6. Ephemeral Infrastructure Testing<br/>(Kiểm thử với Testcontainers thật, không mock stateful components)"]
        P7["7. Zero Test Pollution & Parallel Safety<br/>(Phân lập schema/database per-test, chạy cargo test --all siêu tốc)"]
        P8["8. Single-Command Local Environment<br/>(1 lệnh khởi tạo toàn bộ hạ tầng: Postgres, Redis, RabbitMQ, OTel)"]
        P9["9. Unified Observability Topology<br/>(Tự động liên kết Metrics, Logs, Traces từ local tới production)"]
        P10["10. Hermetic Multi-Stage OCI Containers<br/>(Ảnh container distroless siêu nhẹ, không root, bảo mật tối đa)"]
        P11["11. Strict Quality Gate Enforcement<br/>(Khép kín chu trình kiểm thử, linter, formatting trước khi commit)"]
    end
```

1. **Declarative Type-Safe `AppBuilder` (Khởi tạo trực quan, bảo đảm an toàn kiểu):**
   - Cung cấp fluent builder API có khả năng kiểm tra lúc biên dịch (compile-time safety). Các thành phần bắt buộc (như Database, State) không thể bị bỏ quên; các tầng middleware được kích hoạt có chọn lọc nhưng vị trí thực thi luôn được cố định nghiêm ngặt.
2. **Strict 9-Layer Middleware Onion (Cố định thứ tự bọc Middleware):**
   - Định hình thứ tự bất biến cho 9 tầng middleware: Từ ngoài cùng (Trace Context, Panic Catching) xuyên qua các tầng bảo vệ (Metrics, Rate Limit, Security Headers, CORS, I18n) vào đến bên trong (Auth Context, Domain Route Handlers). Nhà phát triển không thể vô tình đảo lộn thứ tự này.
3. **Strict Data & Resource Autonomy (Cô lập Dữ liệu & Tự chủ Tài nguyên Tuyệt đối):**
   - Thực thi triệt để mô hình **Database-per-Service** và **Cache-per-Service**. Mỗi service sở hữu độc quyền kho dữ liệu và không gian bộ nhớ đệm của mình. Cấm tuyệt đối hành vi truy vấn chéo cơ sở dữ liệu (Cross-Database SQL Queries/JOINs) hoặc xâm phạm Redis keyspace giữa các service.
4. **Zero-Boilerplate Golden Path (Đường đi hoàng kim - Không mã rác):**
   - Một Domain Service đạt chuẩn chỉ cần tối đa 35 dòng mã nguồn tại `src/main.rs`. Toàn bộ các tác vụ hạ tầng phức tạp (nạp cấu hình, logging guard, TCP socket binding, graceful shutdown signal, probe endpoints) đều được trừu tượng hóa sạch sẽ.
5. **Deterministic 7-Stage K8s Shutdown Protocol (Hạ cánh an toàn 7 giai đoạn mặc định):**
   - Tích hợp sẵn sàng `omni-common::server::serve_with_graceful_shutdown`. Mọi service tự động tuân thủ quy trình rút chân khỏi Kubernetes Ingress: Đóng `/readyz` $\rightarrow$ Ngủ chờ Ingress đồng bộ $\rightarrow$ Xả kiệt In-flight HTTP $\rightarrow$ Dừng Background Workers $\rightarrow$ Đóng Database/Redis $\rightarrow$ Flush OTel $\rightarrow$ Thoát an toàn.
6. **Ephemeral Infrastructure Testing (Kiểm thử hạ tầng thực dùng một lần):**
   - Tuyệt đối không mock database hay cache trong các bài kiểm thử tích hợp quan trọng. Mọi test suite tích hợp đều sử dụng **Testcontainers** để tự động kéo container Docker thực tế (PostgreSQL 16, Redis 7.2, RabbitMQ), thực hiện kiểm thử trên môi trường thật và tự hủy sau khi kết thúc.
7. **Zero Test Pollution & Parallel Safety (Triệt tiêu ô nhiễm dữ liệu kiểm thử):**
   - Mỗi test case hoặc test suite chạy trên một dynamic port ngẫu nhiên và một database/schema cô lập. Cho phép chạy `cargo test --all` tận dụng tối đa số lượng CPU cores mà không bao giờ xảy ra tình trạng xung đột dữ liệu.
8. **Single-Command Local Environment (Khởi động môi trường bằng 1 lệnh duy nhất):**
   - Cung cấp cả Docker Compose v2 lẫn Process Compose (`process-compose.yaml`). Lập trình viên chỉ cần gõ đúng một câu lệnh để dựng toàn bộ hệ thống hỗ trợ: 6 cơ sở dữ liệu quan hệ, cụm cache có ACL, hàng đợi tin nhắn và bộ tứ giám sát (Prometheus, Loki, Tempo, Grafana).
9. **Unified Observability Topology (Đồng nhất hóa hạ tầng giám sát từ Local tới Production):**
   - Ngay trên máy lập trình viên, dữ liệu logs (Loki), traces (Tempo/Jaeger), metrics (Prometheus) được kết nối mạch lạc thông qua W3C `traceparent` và hiển thị trực quan trên Grafana dashboards được cấu hình sẵn.
10. **Hermetic Multi-Stage OCI Containerization (Đóng gói container bảo mật tuyệt đối):**
    - Thiết kế Dockerfile chuẩn mực sử dụng kỹ thuật `cargo-chef` giúp cache dependencies siêu việt (giảm thời gian build CI/CD từ 15 phút xuống dưới 30 giây). Chạy trên nền tảng Distroless hoặc Alpine tối giản, không chứa công cụ shell, non-root user `omni:10001`.
11. **Strict Quality Gate Enforcement (Thực thi nghiêm ngặt cổng chất lượng):**
    - Mọi mã nguồn tạo ra từ template scaffold bắt buộc phải vượt qua 100% Quality Gate định nghĩa tại [`AGENTS.md`](../../AGENTS.md): `cargo fmt`, `cargo hakari generate`, `cargo clippy --all-targets --all-features -- -D warnings`, và `cargo test --all`.

---

## 2. Scope of Applicability & Workspace Dependency Architecture

### 2.1. Phạm vi Thiết kế (Scope)

| STT | Thành phần Kỹ thuật                            | Crate / Thư mục Phụ trách                    | Mô tả Chức năng Chi tiết                                                                                                                                                          |
| :-- | :--------------------------------------------- | :------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Data & Resource Isolation Architecture**     | `omni-database`, `omni-cache`, `infra/`      | Chuẩn hóa quy tắc Database-per-Service, Cache Keyspace Isolation, Redis ACL per service, và cơ chế cấm Cross-Database JOIN.                                                       |
| 2   | **Standard `AppBuilder` Pattern**              | `libs/common/src/bootstrap/`                 | Cung cấp struct `AppBuilder<S>` để khởi tạo, cấu hình và lắp ghép ứng dụng Axum với 9 tầng middleware theo đúng thứ tự chuẩn hóa.                                                 |
| 3   | **Production HTTP Server Runtime**             | `libs/common/src/server/`                    | Cung cấp hàm `serve_with_graceful_shutdown` tích hợp Tokio TCP listener, tùy chọn `SO_REUSEPORT`, và bộ điều phối hạ cánh an toàn 7 giai đoạn.                                    |
| 4   | **Local Multi-Service Compose Harness**        | `infra/docker/` & root `docker-compose*.yml` | Cung cấp 3 profile Docker Compose (`infra`, `full`, `tools`) cùng `process-compose.yaml` để chạy toàn bộ Postgres, Redis, RabbitMQ, Loki, Tempo, Prometheus, Grafana, Swagger UI. |
| 5   | **Testcontainers Integration Testing Harness** | `libs/common/src/testing/` & `tests/`        | Cung cấp framework `TestContext`, `TestApp`, `TestClient` và module Testcontainers (Postgres, Redis, RabbitMQ) cho kiểm thử tích hợp tự động không phụ thuộc hạ tầng thật.        |
| 6   | **The Golden Path Service Scaffold**           | `services/order/` (mẫu chuẩn)                | Đặc tả cấu trúc thư mục chuẩn, clean architecture, mẫu `main.rs`, `config.rs`, `state.rs`, `handlers/`, `routes/`, `models/`, `repositories/` và `Dockerfile` tối ưu.             |

### 2.2. Vị trí trong Cây Phụ thuộc Workspace

```mermaid
graph TD
    subgraph SharedLibraries["Thư viện Dùng chung (libs/*)"]
        Common["omni-common<br/>(bootstrap, server, testing, middleware, resilience)"]
        Logging["omni-logging"]
        Errors["omni-errors"]
        Database["omni-database"]
        Cache["omni-cache"]
        Auth["omni-auth"]
        Messaging["omni-messaging"]
        Metrics["omni-metrics"]
    end

    subgraph ServiceScaffold["Khung Dịch vụ Chuẩn (Scaffold)"]
        Isolation["Resource Isolation (DB & Redis)"]
        AppBuilder["AppBuilder<AppState>"]
        Server["ServerRuntime (Graceful Shutdown)"]
    end

    subgraph DomainServices["Microservices (services/*)"]
        OrderSvc["services/order"]
        AuthSvc["services/auth"]
        PaymentSvc["services/payment"]
        OtherSvc["services/..."]
    end

    subgraph TestHarness["Kiểm thử Tích hợp (tests/*)"]
        Testcontainers["Testcontainers Harness<br/>(Postgres, Redis, RabbitMQ)"]
        IntegrationTests["omni-integration-tests"]
    end

    Common --> AppBuilder
    Common --> Server
    Logging --> Common
    Errors --> Common
    Database --> Isolation
    Cache --> Isolation
    Isolation --> AppBuilder
    Auth --> AppBuilder
    Messaging --> AppBuilder
    Metrics --> AppBuilder

    AppBuilder --> DomainServices
    Server --> DomainServices
    Testcontainers --> IntegrationTests
    DomainServices -.-> IntegrationTests
```

---

## 3. Đặc tả Kiến trúc Cô lập Dữ liệu & Tự chủ Tài nguyên (Service Data & Resource Isolation Standard)

Để ngăn ngừa thảm họa _Distributed Monolith_, hệ thống **OmniKubernetes** xác lập nguyên tắc bất biến: **Mỗi Microservice là một Đơn vị Tự chủ Hoàn toàn về Tài nguyên Dữ liệu (Autonomous Data Unit)**.

```mermaid
flowchart TD
    subgraph ClientAndGateway["CLIENT & API GATEWAY LAYER"]
        Client["Browser / Mobile Client"] --> Gateway["API Gateway / Ingress Router"]
    end

    subgraph AuthService["AUTH SERVICE DOMAIN"]
        AuthApp["auth-service Pod<br/>(AppState: auth_pool, auth_cache)"]
        AuthDB[("auth_db<br/>(User: auth_user)")]
        AuthRedis[("Redis Keyspace:<br/>omni:auth:*<br/>(ACL: auth_user)")]
        AuthApp -->|SQL Queries ONLY| AuthDB
        AuthApp -->|Redis Commands ONLY| AuthRedis
    end

    subgraph OrderService["ORDER SERVICE DOMAIN"]
        OrderApp["order-service Pod<br/>(AppState: order_pool, order_cache)"]
        OrderDB[("order_db<br/>(User: order_user)")]
        OrderRedis[("Redis Keyspace:<br/>omni:order:*<br/>(ACL: order_user)")]
        OrderApp -->|SQL Queries ONLY| OrderDB
        OrderApp -->|Redis Commands ONLY| OrderRedis
    end

    subgraph CrossServiceComm["GIAO TIẾP LIÊN DỊCH VỤ (HỢP PHÁP DUY NHẤT)"]
        RESTSync["1. Synchronous REST Call<br/>(qua Ingress / Internal ClusterIP + Circuit Breaker)"]
        AsyncEvents["2. Asynchronous Domain Events<br/>(CloudEvents qua RabbitMQ Outbox/Inbox)"]
    end

    Gateway --> AuthApp
    Gateway --> OrderApp
    OrderApp -.->|HTTP GET /users/:id| RESTSync -.-> AuthApp
    AuthApp -.->|Event: UserSuspended| AsyncEvents -.-> OrderApp

    ForbiddenLink["❌ NGHIÊM CẤM TUYỆT ĐỐI:<br/>1. OrderApp kết nối AuthDB hoặc SQL JOIN<br/>2. OrderApp đọc/ghi omni:auth:*<br/>3. Dùng chung Database User"]
    OrderApp x-.-x|BLOCKED BY ARCHITECTURE & ACL| AuthDB
    OrderApp x-.-x|BLOCKED BY REDIS ACL| AuthRedis
```

---

### 3.1. Nguyên lý Cơ sở Dữ liệu Riêng biệt (`Database-per-Service`)

1. **Ranh giới Dữ liệu Độc quyền:**
   - Mỗi microservice sở hữu một cơ sở dữ liệu logic (hoặc physical instance) riêng biệt:
     - `auth-service` $\longrightarrow$ `auth_db`
     - `order-service` $\longrightarrow$ `order_db`
     - `payment-service` $\longrightarrow$ `payment_db`
     - `product-service` $\longrightarrow$ `product_db`
     - `user-service` $\longrightarrow$ `user_db`
     - `notification-service` $\longrightarrow$ `notification_db`
   - Bất kỳ thực thể dữ liệu nào của một service chỉ được phép đọc và ghi bởi chính service đó.

2. **Nghiêm cấm Tuyệt đối Truy vấn Chéo Database (No Cross-Database Queries/JOINs):**
   - **Quy tắc:** Tuyệt đối không bao giờ viết câu lệnh SQL có dạng `SELECT * FROM order_items JOIN user_db.users ON ...`.
   - **Cơ chế thay thế chuẩn mực:**
     - **Cách 1 (Claims Data Inlining):** Dữ liệu định danh cơ bản của user (`user_id`, `email`, `roles`, `tenant_id`) đã được đóng gói sẵn trong JWT token và giải nén thành `AuthUserClaims` tại Layer 8. Handler có thể sử dụng ngay mà không cần truy vấn sang `user-service`.
     - **Cách 2 (Synchronous REST API):** Gọi HTTP REST qua Client nội bộ (kèm Circuit Breaker và Timeout đã định nghĩa tại Bước 7).
     - **Cách 3 (Materialized Read View via Domain Events):** Khi `user-service` có thay đổi, nó bắn sự kiện `UserUpdatedEvent` (CloudEvents) qua `omni-messaging`. `order-service` tiêu thụ sự kiện này và cập nhật bảng ánh xạ cục bộ (_read-optimized cache/projection_).

3. **Phân quyền Động cơ Tối thiểu (Least Privilege DB Roles):**
   - Không dùng một tài khoản `superuser` hay `postgres` cho toàn bộ ứng dụng.
   - Mỗi service được cấp một DB User riêng:
     - `auth-service` dùng user `auth_user` (chỉ có quyền trên `auth_db`).
     - `order-service` dùng user `order_user` (chỉ có quyền trên `order_db`).
   - Ngay cả khi xảy ra lỗ hổng SQL Injection ở `order-service`, kẻ tấn công cũng **không thể** đọc hoặc phá hoại dữ liệu tài khoản trong `auth_db`.

4. **Chiến lược Phân bổ theo Môi trường:**
   - **Local Development:** 1 container PostgreSQL 16 vật lý chạy chung, nhưng chia thành 6 logical databases riêng biệt với 6 DB users độc lập được khởi tạo tự động qua `01-init-databases.sql`.
   - **Production Kubernetes:** Phân tách thành các PostgreSQL Cluster độc lập (ví dụ sử dụng CloudNativePG Operator hoặc Managed AWS RDS / GCP Cloud SQL). Các domain chịu tải lớn như `order` và `payment` có cụm DB riêng, không bao giờ chia sẻ CPU, RAM hay Connection Limit với các domain khác.

---

### 3.2. Nguyên lý Phân lập Bộ nhớ đệm Redis (`Cache-per-Service`)

Trong kiến trúc Microservices, việc phân lập bộ nhớ đệm phân tán Redis phức tạp hơn nhiều so với Database quan hệ và thường bị các đội ngũ phát triển làm sai.

#### Bẫy Kiến trúc: Lời nguyền của `SELECT <db_index>` trong Redis

- Trong môi trường Redis đơn lẻ (Single Instance), nhiều lập trình viên có thói quen gán mỗi service một số thứ tự database (ví dụ: Service A dùng DB 0, Service B dùng DB 1 qua lệnh `SELECT 1`).
- **Thảm họa:** Trong môi trường Enterprise chạy **Redis Cluster** (AWS ElastiCache Cluster Mode, Valkey Cluster, Redis Sentinel Sharding), **lệnh `SELECT` bị vô hiệu hóa hoàn toàn** (Redis Cluster chỉ hỗ trợ duy nhất Database `0`).
- Nếu ứng dụng được thiết kế dựa vào số thứ tự DB, toàn bộ hệ sinh thái sẽ sập ngay khi nâng cấp lên Redis Cluster với lỗi: `ERR SELECT is not allowed in cluster mode`.

#### 3 Tầng Phân lập Redis Chuẩn mực trong OmniKubernetes:

```mermaid
flowchart TD
    subgraph RedisIsolationStrategy["3 TẦNG PHÂN LẬP REDIS CHUẨN DOANH NGHIỆP"]
        direction TB
        T1["TẦNG 1: MANDATORY KEYSPACE NAMESPACING<br/>(Cú pháp chuẩn hóa: omni:<service-name>:<entity>:<id>)"]
        T2["TẦNG 2: REDIS 7.2+ ACCESS CONTROL LISTS (ACL)<br/>(Giới hạn quyền thao tác chỉ trên pattern ~omni:<service>:*)"]
        T3["TẦNG 3: DEDICATED REDIS INSTANCES / CLUSTERS<br/>(Tách vật lý cho các domain nhạy cảm: Rate Limiting & Auth)"]
    end
```

1. **Tầng 1: Bắt buộc gắn Tiền tố Keyspace (Mandatory Keyspace Namespacing):**
   - Mọi key được lưu trữ qua thư viện `omni-cache` bắt buộc phải tuân theo cấu trúc phân cấp:
     $$\mathbf{omni}:\{\text{service-name}\}:\{\text{domain-entity}\}:\{\text{entity-id}\}[:\{\text{sub-key}\}]$$
   - Ví dụ chuẩn hóa:
     - Auth Service: `omni:auth:session:usr_9981a8b:token`
     - Order Service: `omni:order:cart:usr_9981a8b`
     - Payment Service: `omni:payment:idempotency:tx_a1b2c3d4`
     - Rate Limiter: `omni:rate_limit:ip:192.168.1.100`
   - `CacheConfig` của từng service tự động tiêm tiền tố `keyspace_prefix` vào mọi thao tác `GET`, `SET`, `DEL`. Service A không thể vô tình chạm vào key của Service B.

2. **Tầng 2: Thực thi Bảo mật bằng Redis ACL (Access Control Lists):**
   - Từ Redis 6.0 và 7.2+, hệ thống hỗ trợ cơ chế ACL mạnh mẽ. Mỗi microservice được cấu hình một tài khoản Redis riêng biệt với quyền thao tác bị giới hạn nghiêm ngặt theo key pattern:

     ```text
     user order_user on >order_redis_secret ~omni:order:* +@all -flushdb -flushall
     user auth_user  on >auth_redis_secret  ~omni:auth:*  +@all -flushdb -flushall
     ```

   - Nếu `order-service` cố tình đọc hoặc xóa key `omni:auth:session:...`, động cơ Redis sẽ chặn đứng ngay lập tức và trả về mã lỗi:
     `NOPERM User order_user has no permissions to access the 'omni:auth:session' key`.
   - Vô hiệu hóa triệt để các lệnh nguy hiểm như `FLUSHDB`, `FLUSHALL`, `KEYS *` cho toàn bộ tài khoản microservices.

3. **Tầng 3: Tách biệt Thực thể Vật lý cho các Service Nhạy cảm (Dedicated Clusters):**
   - Để triệt tiêu hiện tượng **"Láng giềng ồn ào" (Noisy Neighbor)**:
     - **Cụm 1 (Dedicated Resilience & Rate Limiting):** Cụm Redis riêng biệt dành cho Traffic Control (`omni-common::resilience`). Lưu lượng kiểm tra rate limit có thể lên tới 50.000 req/s, được cô lập để không làm nghẽn CPU đơn luồng của Redis nghiệp vụ.
     - **Cụm 2 (Domain Cache Cluster):** Dành cho caching nghiệp vụ (Order, Product Catalog, User Profile) với chính sách xóa dữ liệu `volatile-lru`.
     - **Cụm 3 (Persistent / Session Cluster):** Dành cho Auth Sessions, Blacklisted Tokens, Idempotency Keys với cấu hình AOF/RDB bền vững.

---

### 3.3. Phân lập Hàng đợi Tin nhắn (Broker Isolation & Namespace Partitioning)

Đối với truyền thông bất đồng bộ qua RabbitMQ / AMQP:

1. **Phân lập Virtual Host (VHost):**
   - Môi trường phát triển cục bộ và Staging/Production sử dụng vhost phân quyền riêng: `/omni` (hoặc `/omni-order`, `/omni-payment`).
2. **Quy tắc Đặt tên Sàn giao dịch & Hàng đợi (Exchange & Queue Namespacing):**
   - **Sàn giao dịch sự kiện dùng chung (Domain Events Topic Exchange):** `omni.events.topic`.
   - **Hàng đợi độc quyền của từng Service:**
     - Cú pháp: `q.omni.{consumer-service}.{event-domain}.{event-name}`
     - Ví dụ: `q.omni.order.payment.payment_completed` (Hàng đợi của `order-service` lắng nghe sự kiện thanh toán hoàn tất).
   - **Dead-Letter Queue riêng biệt:**
     - Cú pháp: `q.omni.{consumer-service}.dlx`
   - Tuyệt đối cấm nhiều microservice cùng đọc chung một hàng đợi công việc (_Competing Consumers Anti-Pattern_) trừ khi các worker đó là các bản sao replica của cùng một microservice.

---

### 3.4. Ràng buộc An toàn Kiểu dữ liệu trong Rust `AppState`

Sự cô lập tài nguyên không chỉ nằm ở cấu hình hạ tầng mà được **đóng băng ngay tại hệ thống kiểu dữ liệu tĩnh của Rust**.

Mỗi Domain Service chỉ định nghĩa một struct `AppState` chứa đúng những tài nguyên thuộc quyền sở hữu của nó:

```rust
// services/order/src/state.rs
use std::sync::Arc;
use sqlx::PgPool;
use omni_cache::CacheManager;
use omni_messaging::outbox::OutboxPublisher;

/// Trạng thái ứng dụng độc quyền của Order Service.
/// Tuyệt đối KHÔNG chứa connection string, pool hay client của bất kỳ service nào khác!
#[derive(Clone)]
pub struct AppState {
    /// Kết nối độc quyền tới order_db (chạy dưới quyền order_user)
    pub db_pool: PgPool,
    /// Bộ nhớ đệm được khóa cứng namespace "omni:order:*"
    pub cache: Arc<CacheManager>,
    /// Transactional Outbox Publisher bắn sự kiện domain của Order
    pub outbox_publisher: Arc<OutboxPublisher>,
}

impl AppState {
    pub fn new(db_pool: PgPool, cache: Arc<CacheManager>, outbox_publisher: Arc<OutboxPublisher>) -> Self {
        Self {
            db_pool,
            cache,
            outbox_publisher,
        }
    }
}
```

---

## 4. Standard `AppBuilder` Pattern Architecture

### 4.1. Triết lý Thiết kế: Declarative & Typestate Pattern

Việc cấu hình một ứng dụng Axum trong môi trường doanh nghiệp thường đòi hỏi rất nhiều thành phần: Router chính, Router probes, Router OpenAPI, Router metrics, Router i18n reload, cùng với hàng loạt Tower Layers.

Mẫu thiết kế `AppBuilder` áp dụng nguyên lý **Declarative Configuration** kết hợp **Typestate Pattern**:

- `AppBuilder::new(service_name)`: Khởi tạo builder gắn liền với định danh của service.
- Quản lý state của ứng dụng thông qua kiểu dữ liệu generic `S: Clone + Send + Sync + 'static`.
- Cung cấp các phương thức fluent (`.with_*()`) để cấu hình linh hoạt nhưng kiểm soát chặt chẽ vị trí của từng middleware.
- Phương thức `.build()` sẽ tự động kết nối các sub-routers và bọc các middleware theo **thứ tự toán học bất biến**.

```mermaid
flowchart LR
    A["AppBuilder::new('order-service')"] --> B[".with_state(app_state)"]
    B --> C[".with_observability_middleware()"]
    C --> D[".with_metrics_middleware()"]
    D --> E[".with_resilience_middleware()"]
    E --> F[".with_security_headers()"]
    F --> G[".with_cors(cors_layer)"]
    G --> H[".with_k8s_probes()"]
    H --> I[".with_metrics_endpoint()"]
    I --> J[".with_openapi_docs()"]
    J --> K[".mount_api_routes('/api/v1/orders', routes)"]
    K --> L[".build() -> axum::Router"]
```

---

### 4.2. Cấu trúc 9 Tầng Middleware Onion (The Strict 9-Layer Middleware Onion)

Trong Tower và Axum, middleware được thực thi theo mô hình hai chiều (_Inbound_ từ ngoài vào trong, _Outbound_ từ trong ra ngoài):

$$\text{Request} \longrightarrow \text{Layer 1} \longrightarrow \dots \longrightarrow \text{Layer 9} \longrightarrow \text{Route Handler} \longrightarrow \text{Layer 9} \longrightarrow \dots \longrightarrow \text{Layer 1} \longrightarrow \text{Response}$$

Để đảm bảo an toàn tuyệt đối, thứ tự 9 tầng middleware trong `AppBuilder` được chuẩn hóa như sau:

```mermaid
sequenceDiagram
    autonumber
    actor Client as Ingress / Client
    box rgb(30, 41, 59) "TẦNG NGOẠI BIÊN & QUAN SÁT (OUTER ONION)"
        participant L1 as Layer 1: Trace Context & Request ID
        participant L2 as Layer 2: Catch Panic & RFC 9457 Fallback
        participant L3 as Layer 3: Server-Timing & Latency Tracker
        participant L4 as Layer 4: Prometheus RED Metrics
    end
    box rgb(51, 65, 85) "TẦNG PHỤC HỒI MẠNG & AN NINH (TRAFFIC & SECURITY)"
        participant L5 as Layer 5: Traffic Control (Timeout, LoadShed, RateLimit)
        participant L6 as Layer 6: Security Headers & CORS
    end
    box rgb(71, 85, 105) "TẦNG NGỮ CẢNH & PHÂN QUYỀN (CONTEXT & AUTH)"
        participant L7 as Layer 7: I18n & Request Locale Waterfall
        participant L8 as Layer 8: Auth Claims & Tenant Context
    end
    box rgb(15, 23, 42) "TẦNG NGHIỆP VỤ (CORE)"
        participant L9 as Layer 9: Domain Route Handler
    end

    Client->>L1: Inbound HTTP Request
    L1->>L1: Trích xuất W3C traceparent / Sinh X-Request-Id & Span
    L1->>L2: Forward RequestContext
    L2->>L3: Forward (Wrap CatchPanic)
    L3->>L4: Bắt đầu bấm giờ Stopwatch
    L4->>L4: Tăng bộ đếm In-flight Requests
    L4->>L5: Forward Request
    L5->>L5: Kiểm tra Rate Limit (GCRA/Redis) & Concurrency Limit
    L5->>L6: Forward (Áp đặt Timeout Deadline)
    L6->>L6: Xử lý CORS Preflight OPTIONS (nếu có)
    L6->>L7: Forward
    L7->>L7: Phân giải BCP 47 RequestLocale (5-step waterfall)
    L7->>L8: Forward
    L8->>L8: Xác thực Bearer JWT & Trích xuất Claims / TenantId
    L8->>L9: Gọi Route Handler với AppState & Extractors
    L9-->>L9: Thực thi nghiệp vụ (Database, Cache, Outbox)
    L9-->>L8: Trả về ApiResponse / Domain Result
    L8-->>L7: Trả về
    L7-->>L6: Trả về
    L6-->>L6: Gắn Security Headers (HSTS, CSP, X-Frame-Options)
    L5-->>L5: Giải phóng Concurrency Slot
    L4-->>L4: Ghi nhận Prometheus RED metrics (Duration, Status, Method)
    L3-->>L3: Gắn header Server-Timing: total;dur=X và X-Response-Time
    L2-->>L2: Bắt Exception nếu có panic -> Chuyển thành 500 RFC 9457
    L1-->>L1: Gắn traceparent & X-Request-Id vào Response Headers
    L1-->>Client: Outbound HTTP Response hoàn chỉnh
```

#### Phân tích Chi tiết 9 Tầng Middleware:

1. **Layer 1: Tracing Context & Correlation ID (`omni_common::middleware::observability_middleware`)**
   - **Nhiệm vụ:** Trích xuất hoặc khởi tạo W3C `traceparent` và `X-Request-Id`. Gắn `RequestContext` vào request extensions và tạo root tracing span cho toàn bộ vòng đời của request.
   - **Vị trí:** Tầng ngoài cùng tuyệt đối (Layer 1). Mọi log ghi ra từ các tầng bên trong (kể cả log từ chối xác thực hay log panic) đều bắt buộc phải mang `trace_id` và `request_id`.
2. **Layer 2: Catch Panic & RFC 9457 Fallback Handler (`tower_http::catch_panic`)**
   - **Nhiệm vụ:** Bắt các tình huống `std::panic::catch_unwind` không mong muốn trong mã nguồn Rust. Thay vì để sập worker thread và ngắt kết nối TCP đột ngột, middleware này biến panic thành đối tượng `ApiErrorResponse` chuẩn RFC 9457 với HTTP status `500 Internal Server Error`.
   - **Vị trí:** Ngay sau Layer 1 để đảm bảo khi xảy ra panic, mã phản hồi lỗi 500 vẫn được gắn đầy đủ header tracing và request ID.
3. **Layer 3: Timing & Server-Timing Collector (`omni_common::middleware::timing_middleware`)**
   - **Nhiệm vụ:** Khởi động đồng hồ đo thời gian có độ chính xác cao (`Instant::now()`). Khi response quay ra, tính toán tổng thời gian xử lý và tiêm các header: `Server-Timing: total;dur=12.45` và `X-Response-Time: 12.45ms`.
4. **Layer 4: Prometheus RED Metrics Middleware (`omni_metrics::middleware`)**
   - **Nhiệm vụ:** Thu thập các chỉ số RED (Rate, Errors, Duration) phục vụ việc giám sát:
     - `http_requests_total{method, route, status}`
     - `http_requests_in_flight{service}`
     - `http_request_duration_seconds{method, route, status}` (Histogram)
   - **Vị trí:** Đứng trước tầng Resilience để đo lường chính xác cả những request bị từ chối do Rate Limiting hoặc Timeout.
5. **Layer 5: Traffic Control & Resilience Middleware (`omni_common::resilience`)**
   - **Nhiệm vụ:** Thực thi 3 cơ chế tự vệ theo nguyên tắc _Fail Fast, Fail Cheap_:
     - **Request Timeout:** Áp đặt trần thời gian xử lý (mặc định 15s-30s). Tự động ngắt Future và giải phóng tài nguyên khi chạm ngưỡng.
     - **Adaptive Load Shedding:** Giới hạn số lượng request đồng thời bằng Semaphore. Từ chối ngay lập tức bằng `503 Service Unavailable` khi hàng đợi quá tải.
     - **Dual-Layer Rate Limiting:** GCRA in-memory + Redis Sliding Window Counter. Trả về `429 Too Many Requests` kèm header `Retry-After`.
6. **Layer 6: Security Headers & Cross-Origin Resource Sharing (CORS)**
   - **Nhiệm vụ:**
     - Xử lý các request `OPTIONS` (CORS Preflight) an toàn thông qua cấu hình `tower_http::cors::CorsLayer`.
     - Tiêm các header bảo mật chuẩn OWASP: `Strict-Transport-Security` (HSTS), `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Content-Security-Policy: default-src 'self'`, `Referrer-Policy: strict-origin-when-cross-origin`.
7. **Layer 7: Internationalization & Locale Resolution (`omni_common::i18n`)**
   - **Nhiệm vụ:** Thực thi thuật toán 5 bước thác nước (Waterfall Locale Resolution) dựa trên: Query parameter `?lang=` $\rightarrow$ Header tùy biến `X-Locale` $\rightarrow$ Auth User Profile claims $\rightarrow$ Header chuẩn `Accept-Language` $\rightarrow$ Fallback mặc định `DEFAULT_LOCALE` (`en-US`).
   - Tiêm đối tượng `RequestLocale` vào request extensions để các handler nghiệp vụ tự động dịch thông báo theo ngôn ngữ mong muốn của Client.
8. **Layer 8: Authentication, Claims & Tenant Extraction (`omni-auth`)**
   - **Nhiệm vụ:** Kiểm tra và giải mã Bearer JWT token từ header `Authorization`. Trích xuất định danh người dùng (`user_id`), quyền hạn (`roles`), và mã khách hàng doanh nghiệp (`tenant_id`).
   - Gắn cấu trúc `AuthUserClaims` vào request extensions hoặc chặn đứng bằng lỗi RFC 9457 `401 Unauthorized` / `403 Forbidden` nếu route yêu cầu bảo vệ.
9. **Layer 9: Domain Business Routes & Fallback Handler**
   - **Nhiệm vụ:** Định tuyến request tới các hàm xử lý nghiệp vụ cụ thể (`handlers`).
   - **Fallback:** Bất kỳ request nào không khớp route sẽ được chuyển tới `fallback_handler`, tự động trả về lỗi RFC 9457 `404 Not Found` kèm danh sách endpoints gợi ý nếu có.

---

### 4.3. Đặc tả Giao diện `AppBuilder` (`omni_common::bootstrap`)

Dưới đây là đặc tả kiểu dữ liệu (Rust API Contract) chuẩn mực cho `AppBuilder`:

```rust
// libs/common/src/bootstrap/builder.rs

use std::sync::Arc;
use std::time::Duration;
use axum::{Router, response::IntoResponse};
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use crate::config::AppConfig;
use crate::openapi::DocsConfig;
use crate::resilience::ShutdownDrainGate;

/// Mẫu thiết kế chuẩn hóa để khởi tạo ứng dụng Axum trong hệ sinh thái OmniKubernetes.
pub struct AppBuilder<S = ()> {
    service_name: &'static str,
    state: Option<S>,
    custom_routes: Vec<(String, Router<S>)>,
    enable_observability: bool,
    enable_metrics: bool,
    enable_resilience: bool,
    enable_security_headers: bool,
    cors_layer: Option<CorsLayer>,
    drain_gate: Option<Arc<ShutdownDrainGate>>,
    metrics_recorder: Option<metrics_exporter_prometheus::PrometheusHandle>,
    openapi_spec: Option<utoipa::openapi::OpenApi>,
    docs_config: Option<DocsConfig>,
    fallback_handler: Option<axum::handler::Handler<(), ()>>,
}

impl AppBuilder<()> {
    /// Khởi tạo một AppBuilder mới với tên dịch vụ chuẩn.
    #[must_use]
    pub fn new(service_name: &'static str) -> Self {
        Self {
            service_name,
            state: None,
            custom_routes: Vec::new(),
            enable_observability: true,
            enable_metrics: true,
            enable_resilience: true,
            enable_security_headers: true,
            cors_layer: None,
            drain_gate: None,
            metrics_recorder: None,
            openapi_spec: None,
            docs_config: None,
            fallback_handler: None,
        }
    }
}

impl<S> AppBuilder<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Gắn Application State dùng chung (Database Pool, Cache Manager, Message Publisher).
    #[must_use]
    pub fn with_state<NewS>(self, state: NewS) -> AppBuilder<NewS>
    where
        NewS: Clone + Send + Sync + 'static,
    {
        AppBuilder {
            service_name: self.service_name,
            state: Some(state),
            custom_routes: Vec::new(),
            enable_observability: self.enable_observability,
            enable_metrics: self.enable_metrics,
            enable_resilience: self.enable_resilience,
            enable_security_headers: self.enable_security_headers,
            cors_layer: self.cors_layer,
            drain_gate: self.drain_gate,
            metrics_recorder: self.metrics_recorder,
            openapi_spec: self.openapi_spec,
            docs_config: self.docs_config,
            fallback_handler: self.fallback_handler,
        }
    }

    /// Kích hoạt Kubernetes Probes (/livez, /readyz, /startupz) phối hợp với ShutdownDrainGate.
    #[must_use]
    pub fn with_k8s_probes(mut self, drain_gate: Arc<ShutdownDrainGate>) -> Self {
        self.drain_gate = Some(drain_gate);
        self
    }

    /// Kích hoạt endpoint /metrics cho Prometheus Scraper.
    #[must_use]
    pub fn with_metrics_endpoint(
        mut self,
        recorder: metrics_exporter_prometheus::PrometheusHandle,
    ) -> Self {
        self.metrics_recorder = Some(recorder);
        self
    }

    /// Tích hợp đặc tả OpenAPI (phục vụ /v3/api-docs/openapi.json).
    #[must_use]
    pub fn with_openapi_docs(
        mut self,
        spec: utoipa::openapi::OpenApi,
        docs_config: DocsConfig,
    ) -> Self {
        self.openapi_spec = Some(spec);
        self.docs_config = Some(docs_config);
        self
    }

    /// Gắn cấu hình CORS Layer tùy chỉnh.
    #[must_use]
    pub fn with_cors(mut self, cors: CorsLayer) -> Self {
        self.cors_layer = Some(cors);
        self
    }

    /// Mount các route nghiệp vụ của Domain Service theo prefix.
    #[must_use]
    pub fn mount_routes(mut self, path_prefix: &str, router: Router<S>) -> Self {
        self.custom_routes.push((path_prefix.to_string(), router));
        self
    }

    /// Lắp ráp hoàn chỉnh Axum Router với đầy đủ 9 tầng Middleware theo đúng thứ tự.
    pub fn build(self) -> Router {
        let state = self.state.expect("AppState bắt buộc phải được cung cấp trước khi build");
        let mut app_router = Router::new();

        // 1. Mount các route nghiệp vụ kèm State
        for (prefix, router) in self.custom_routes {
            if prefix.is_empty() || prefix == "/" {
                app_router = app_router.merge(router);
            } else {
                app_router = app_router.nest(&prefix, router);
            }
        }
        let mut app_router = app_router.with_state(state);

        // 2. Mount hạ tầng không yêu cầu domain state (Probes, Metrics, OpenAPI)
        if let Some(drain_gate) = self.drain_gate {
            app_router = app_router.merge(crate::resilience::create_probes_router(drain_gate));
        }

        if let Some(recorder) = self.metrics_recorder {
            app_router = app_router.route("/metrics", axum::routing::get(move || {
                let metrics_data = recorder.render();
                async move { metrics_data }
            }));
        }

        if let (Some(spec), Some(docs_config)) = (self.openapi_spec, self.docs_config) {
            app_router = app_router.merge(crate::openapi::create_docs_router_with_config(spec, docs_config));
        }

        // 3. Fallback Route mặc định chuẩn RFC 9457
        app_router = app_router.fallback(crate::bootstrap::default_not_found_handler);

        // 4. Áp dụng các tầng Middleware (Tower wrapping: Middleware thêm sau cùng sẽ chạy trước)
        // Để đạt được thứ tự Inbound mong muốn: L1 -> L2 -> L3 -> L4 -> L5 -> L6
        // Chúng ta phải layer() theo thứ tự ngược lại (L6 -> L5 -> L4 -> L3 -> L2 -> L1):

        if let Some(cors) = self.cors_layer {
            app_router = app_router.layer(cors);
        }

        if self.enable_security_headers {
            app_router = app_router.layer(axum::middleware::from_fn(crate::middleware::security_headers_middleware));
        }

        if self.enable_resilience {
            // Tầng phục hồi: Timeout & Load Shedding
            app_router = app_router.layer(axum::middleware::from_fn(crate::resilience::load_shed_layer));
        }

        if self.enable_metrics {
            app_router = app_router.layer(axum::middleware::from_fn(crate::middleware::metrics_middleware));
        }

        app_router = app_router.layer(axum::middleware::from_fn(crate::middleware::timing_middleware));
        app_router = app_router.layer(tower_http::catch_panic::CatchPanicLayer::custom(crate::bootstrap::handle_panic));

        if self.enable_observability {
            app_router = app_router.layer(axum::middleware::from_fn(crate::middleware::observability_middleware));
        }

        app_router
    }
}
```

---

## 5. Production HTTP Server & Graceful Shutdown Orchestration

### 5.1. Kiến trúc Server Runtime (`omni_common::server`)

Trong môi trường Kubernetes, việc khởi động một web server không chỉ đơn thuần là `TcpListener::bind` rồi gọi hàm lắng nghe. Một kiến trúc chuẩn doanh nghiệp đòi hỏi:

1. **Cấu hình Socket nâng cao (`socket2`):**
   - Hỗ trợ `SO_REUSEPORT` và `SO_REUSEADDR` cho phép nhiều tiến trình hoặc nhiều thread chia sẻ cùng một port, hỗ trợ rolling update mượt mà trên môi trường bare-metal hoặc VM.
   - Thiết lập `TCP_NODELAY = true` để vô hiệu hóa thuật toán Nagle, tối ưu hóa độ trễ xử lý các request HTTP REST/JSON nhỏ.
2. **Hạ cánh An toàn Phối hợp (Coordinated Graceful Shutdown):**
   - Lắng nghe đồng thời cả hai tín hiệu hệ điều hành POSIX `SIGTERM` (do Kubelet gửi khi scale-down / rolling update) và `SIGINT` (Ctrl+C từ lập trình viên).
   - Tích hợp chặt chẽ với máy trạng thái của `GracefulShutdownCoordinator` đã được định nghĩa tại Bước 7 ([`resilience-traffic-control-architecture.md`](resilience-traffic-control-architecture.md)).

```mermaid
sequenceDiagram
    autonumber
    actor K8s as Kubelet / OS
    participant Signal as Signal Listener (SIGTERM)
    participant Coord as GracefulShutdownCoordinator
    participant Gate as ShutdownDrainGate (/readyz)
    participant Ingress as K8s Ingress / Envoy
    participant Server as Axum Server (hyper::auto)
    participant Pool as Stateful Pools (DB & Redis)
    participant Telemetry as OTel & Loki Tracing

    K8s->>Signal: Gửi tín hiệu SIGTERM
    Signal->>Coord: Kích hoạt quy trình hạ cánh an toàn
    Note over Coord: GIAI ĐOẠN 1: ĐÓN BẮT TÍN HIỆU

    Coord->>Gate: Đóng Drain Gate (kích hoạt Draining State)
    Note over Gate: GIAI ĐOẠN 2: CHUYỂN /readyz SANG 503
    K8s->>Gate: Kubelet thăm dò /readyz định kỳ
    Gate-->>K8s: Trả về HTTP 503 Service Unavailable
    K8s->>Ingress: Rút IP của Pod khỏi EndpointSlice

    Note over Coord: GIAI ĐOẠN 3: NGỦ ĐỒNG BỘ INGRESS (5s - 10s)
    Coord->>Coord: tokio::time::sleep(ingress_drain_sleep)

    Note over Coord: GIAI ĐOẠN 4: XẢ KIỆT IN-FLIGHT HTTP REQUESTS
    Coord->>Server: Phát tín hiệu graceful_shutdown cho Hyper
    Server->>Server: Ngừng accept kết nối TCP mới
    Server->>Server: Chờ các in-flight requests xử lý xong (tối đa timeout)
    Server-->>Coord: Hoàn tất xả kết nối HTTP

    Note over Coord: GIAI ĐOẠN 5: DỪNG WORKERS BẤT ĐỒNG BỘ
    Coord->>Coord: CancellationToken::cancel() (Dừng Outbox Relay / Inbox Consumers)

    Note over Coord: GIAI ĐOẠN 6: ĐÓNG KẾT NỐI STATEFUL
    Coord->>Pool: db_pool.close().await & redis_conn.close()

    Note over Coord: GIAI ĐOẠN 7: XẢ BỘ ĐỆM OBSERVABILITY
    Coord->>Telemetry: opentelemetry::global::shutdown_tracer_provider()
    Coord->>Telemetry: Flush bộ đệm log sang Loki
    Coord-->>K8s: Tiến trình kết thúc an toàn (Exit Code 0)
```

### 5.2. Đặc tả Hàm Khởi chạy Server (`serve_with_graceful_shutdown`)

```rust
// libs/common/src/server/mod.rs

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn, error};
use crate::resilience::GracefulShutdownCoordinator;

/// Khởi chạy Axum Server trên port chỉ định kèm quy trình hạ cánh an toàn 7 giai đoạn.
pub async fn serve_with_graceful_shutdown(
    app: Router,
    port: u16,
    coordinator: Arc<GracefulShutdownCoordinator>,
    shutdown_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!(
        transport.port = port,
        transport.bind_address = %addr,
        "Production HTTP server đã sẵn sàng tiếp nhận kết nối"
    );

    // Kênh truyền tín hiệu từ bộ lắng nghe POSIX OS sang Hyper Server
    let (server_shutdown_tx, server_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn tác vụ điều phối tín hiệu tắt hệ thống
    let coord_clone = coordinator.clone();
    let token_clone = shutdown_token.clone();
    tokio::spawn(async move {
        // Đón bắt SIGINT (Ctrl+C) hoặc SIGTERM (Kubernetes)
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Nhận tín hiệu SIGINT (Ctrl+C). Bắt đầu quy trình tắt...");
            }
            _ = wait_for_sigterm() => {
                info!("Nhận tín hiệu SIGTERM từ Kubernetes Kubelet. Bắt đầu quy trình tắt...");
            }
            _ = token_clone.cancelled() => {
                info!("Nhận tín hiệu hủy từ CancellationToken nội bộ. Bắt đầu quy trình tắt...");
            }
        }

        // Thực thi Giai đoạn 1, 2, 3 của quy trình Shutdown
        coord_clone.start_draining().await;

        // Báo cho Axum/Hyper bắt đầu từ chối kết nối mới và drain in-flight requests
        let _ = server_shutdown_tx.send(());
    });

    // Khởi chạy Axum server với Hyper Graceful Shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = server_shutdown_rx.await;
        })
        .await?;

    info!("Axum HTTP server đã hoàn tất xả toàn bộ In-flight HTTP requests.");

    // Tiếp tục thực thi các giai đoạn còn lại (Dừng workers, đóng DB, flush logs)
    coordinator.complete_shutdown().await;
    info!("Toàn bộ tiến trình microservice đã hạ cánh an toàn tuyệt đối (Clean Exit).");

    Ok(())
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{signal, SignalKind};
    if let Ok(mut sig) = signal(SignalKind::terminate()) {
        sig.recv().await;
    } else {
        std::future::pending::<()>().await;
    }
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    std::future::pending::<()>().await;
}
```

---

## 6. Local Multi-Service Development Harness Architecture

### 6.1. Triết lý "Zero-Friction Local Environment"

Một trong những nguyên nhân phổ biến nhất khiến năng suất của đội ngũ kỹ sư bị suy giảm là sự phức tạp khi dựng môi trường chạy thử cục bộ. Mục tiêu thiết kế của **Local Dev Harness**:

1. **Khởi động 1 lệnh (`Single-Command Bootstrap`):** `docker compose up -d` hoặc `process-compose up`.
2. **Cô lập Dữ liệu Hoàn chỉnh:** Mỗi microservice sở hữu một Database riêng biệt và tài khoản DB riêng biệt trong cụm PostgreSQL cục bộ; Redis được phân quyền keyspace qua ACL.
3. **Cung cấp Toàn diện Trục Quan sát (Observability Spine):** Lập trình viên có thể kiểm tra trực tiếp metrics, traces, logs và swagger docs ngay trên máy của mình giống hệt trên Production Kubernetes.

```mermaid
flowchart TD
    subgraph DevMachine["MÁY PHÁT TRIỂN CỤC BỘ (LOCAL WORKSTATION)"]
        direction TB

        subgraph Orchestration["CƠ CHẾ ĐIỀU PHỐI (ORCHESTRATION)"]
            PC["Process Compose (process-compose.yaml)<br/>Điều phối Rust Dev Binaries + Hot Reload"]
            DC["Docker Compose (docker-compose.yml)<br/>Điều phối Cụm Hạ tầng Containerized"]
        end

        subgraph StorageLayer["TẦNG LƯU TRỮ & TRUYỀN THÔNG (ISOLATED CONTAINERS)"]
            direction LR
            PG["PostgreSQL 16 Multi-DB<br/>(:5432)<br/>auth_db, order_db, ...<br/>Users: auth_user, order_user..."]
            RD["Redis 7.2 Cache with ACL<br/>(:6379)<br/>Namespaces: omni:auth:*, omni:order:*"]
            RMQ["RabbitMQ 3.13 Management<br/>(:5672, :15672)<br/>Vhost: /omni, Dedicated Queues"]
        end

        subgraph TelemetryLayer["TẦNG GIÁM SÁT TẬP TRUNG (CONTAINERS)"]
            direction LR
            PROM["Prometheus<br/>(:9090)<br/>Scrapes /metrics"]
            LOKI["Grafana Loki<br/>(:3100)<br/>Logs Ingestion"]
            TEMPO["Grafana Tempo<br/>(:3200, OTLP:4317)<br/>Traces Collector"]
            GRAF["Grafana Dashboard<br/>(:3000)<br/>Unified UI"]
        end

        subgraph GatewayDocs["TẦNG TỔNG HỢP TÀI LIỆU"]
            SWAG["Swagger UI Aggregator<br/>(:8080)<br/>/swagger-config.json"]
        end

        subgraph LocalApps["MICROSERVICES CHẠY CỤC BỘ (CARGO / RUST)"]
            APP1["services/auth (:8081)"]
            APP2["services/order (:8082)"]
            APP3["services/... (:808X)"]
        end

        Orchestration --> StorageLayer
        Orchestration --> TelemetryLayer
        Orchestration --> GatewayDocs
        LocalApps --> StorageLayer
        LocalApps --> TelemetryLayer
        GatewayDocs -.-> LocalApps
    end
```

---

### 6.2. Cấu trúc Thư mục Hạ tầng Cục bộ (`infra/docker/`)

```structure
infra/docker/
├── docker-compose.yml              # Profile chính: Toàn bộ infra phụ trợ + Observability
├── docker-compose.infra.yml        # Chỉ chạy Database + Redis + RabbitMQ (siêu nhẹ)
├── docker-compose.full.yml         # Chạy cả Infra lẫn các Pod Service đóng gói container
├── process-compose.yaml            # Công cụ hiện đại chạy Native Rust + Docker Infra
├── postgres/
│   ├── 01-init-databases.sql       # Tạo 6 DBs riêng biệt, cấp 6 users riêng biệt, extensions
│   └── postgresql.conf             # Tối ưu hóa memory, max_connections cho local dev
├── redis/
│   ├── redis.conf                  # Cấu hình ACL, maxmemory-policy volatile-lru, AOF
│   └── users.acl                   # Định nghĩa quyền truy cập Keyspace cho từng microservice
├── rabbitmq/
│   ├── rabbitmq.conf               # Cấu hình guest user, memory high watermark
│   └── definitions.json            # Định nghĩa sẵn Exchange, Queues, Routing Keys
├── prometheus/
│   └── prometheus.yml              # Scrape targets cho toàn bộ microservices cục bộ
├── grafana/
│   ├── provisioning/
│   │   ├── datasources/
│   │   │   └── datasources.yaml    # Tự động nạp Prometheus, Loki, Tempo
│   │   └── dashboards/
│   │       ├── dashboards.yaml     # Tự động nạp dashboard JSON
│   │       └── omni-services.json  # Dashboard giám sát RED metrics & Latency
│   └── dashboards/
├── loki/
│   └── loki-config.yaml            # Cấu hình lưu trữ in-memory/filesystem cho Loki
└── tempo/
    └── tempo.yaml                  # Cấu hình tiếp nhận OTLP gRPC 4317 & HTTP 4318
```

---

### 6.3. Kịch bản Khởi tạo Đa Cơ sở Dữ liệu & Phân quyền Người dùng (`01-init-databases.sql`)

Tệp SQL khởi tạo không chỉ tạo 6 database độc lập mà còn **tạo ra 6 tài khoản người dùng riêng biệt với nguyên tắc Least Privilege**:

```sql
-- infra/docker/postgres/01-init-databases.sql
-- Tự động khởi tạo cơ sở dữ liệu và người dùng riêng biệt cho từng Microservice

-- 1. Tạo 6 Cơ sở dữ liệu riêng biệt
CREATE DATABASE auth_db;
CREATE DATABASE order_db;
CREATE DATABASE payment_db;
CREATE DATABASE product_db;
CREATE DATABASE user_db;
CREATE DATABASE notification_db;

-- 2. Tạo 6 Người dùng độc lập với mật khẩu riêng
CREATE USER auth_user WITH ENCRYPTED PASSWORD 'auth_dev_secret_pass';
CREATE USER order_user WITH ENCRYPTED PASSWORD 'order_dev_secret_pass';
CREATE USER payment_user WITH ENCRYPTED PASSWORD 'payment_dev_secret_pass';
CREATE USER product_user WITH ENCRYPTED PASSWORD 'product_dev_secret_pass';
CREATE USER user_user WITH ENCRYPTED PASSWORD 'user_dev_secret_pass';
CREATE USER notification_user WITH ENCRYPTED PASSWORD 'notification_dev_secret_pass';

-- 3. Cấu hình Extension và Phân quyền Độc quyền cho từng Database
\c auth_db;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
GRANT ALL PRIVILEGES ON DATABASE auth_db TO auth_user;
GRANT ALL ON SCHEMA public TO auth_user;

\c order_db;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
GRANT ALL PRIVILEGES ON DATABASE order_db TO order_user;
GRANT ALL ON SCHEMA public TO order_user;

\c payment_db;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
GRANT ALL PRIVILEGES ON DATABASE payment_db TO payment_user;
GRANT ALL ON SCHEMA public TO payment_user;

\c product_db;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
GRANT ALL PRIVILEGES ON DATABASE product_db TO product_user;
GRANT ALL ON SCHEMA public TO product_user;

\c user_db;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
GRANT ALL PRIVILEGES ON DATABASE user_db TO user_user;
GRANT ALL ON SCHEMA public TO user_user;

\c notification_db;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
GRANT ALL PRIVILEGES ON DATABASE notification_db TO notification_user;
GRANT ALL ON SCHEMA public TO notification_user;
```

---

### 6.4. Cấu hình Redis ACL & Phân quyền Keyspace (`infra/docker/redis/users.acl`)

Để đảm bảo tính cô lập tuyệt đối của tầng bộ nhớ đệm ngay cả khi chạy trên một cụm Redis dùng chung:

```text
# infra/docker/redis/users.acl
# Phân quyền truy cập Keyspace cho từng Microservice

# User quản trị hệ thống (Chỉ dùng cho DevOps / Migration)
user default on >omni_admin_secret ~* +@all

# 1. Auth Service: CHỈ được thao tác trên keyspace omni:auth:*
user auth_user on >auth_redis_secret ~omni:auth:* +@all -@dangerous -flushdb -flushall

# 2. Order Service: CHỈ được thao tác trên keyspace omni:order:*
user order_user on >order_redis_secret ~omni:order:* +@all -@dangerous -flushdb -flushall

# 3. Payment Service: CHỈ được thao tác trên keyspace omni:payment:*
user payment_user on >payment_redis_secret ~omni:payment:* +@all -@dangerous -flushdb -flushall

# 4. Product Service: CHỈ được thao tác trên keyspace omni:product:*
user product_user on >product_redis_secret ~omni:product:* +@all -@dangerous -flushdb -flushall

# 5. User Service: CHỈ được thao tác trên keyspace omni:user:*
user user_user on >user_redis_secret ~omni:user:* +@all -@dangerous -flushdb -flushall

# 6. Notification Service: CHỈ được thao tác trên keyspace omni:notification:*
user notification_user on >notification_redis_secret ~omni:notification:* +@all -@dangerous -flushdb -flushall

# 7. Rate Limiter Middleware Dùng chung
user limiter_user on >limiter_redis_secret ~omni:rate_limit:* +@all -@dangerous -flushdb -flushall
```

---

### 6.5. Đặc tả Tệp `docker-compose.yml` Hoàn chỉnh

```yaml
# infra/docker/docker-compose.yml
version: "3.9"

networks:
  omni-network:
    name: omni-network
    driver: bridge

volumes:
  postgres_data:
  redis_data:
  rabbitmq_data:
  grafana_data:
  tempo_data:

services:
  # -------------------------------------------------------------
  # 1. TẦNG CƠ SỞ DỮ LIỆU & BỘ NHỚ ĐỆM (ISOLATED DATA ENGINES)
  # -------------------------------------------------------------
  postgres:
    image: postgres:16-alpine
    container_name: omni-postgres
    restart: unless-stopped
    environment:
      POSTGRES_USER: omni_admin
      POSTGRES_PASSWORD: omni_dev_admin_password
      POSTGRES_DB: omni_default
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./postgres/01-init-databases.sql:/docker-entrypoint-initdb.d/01-init-databases.sql:ro
    ports:
      - "5432:5432"
    networks:
      - omni-network
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U omni_admin"]
      interval: 5s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7.2-alpine
    container_name: omni-redis
    restart: unless-stopped
    command: ["redis-server", "--appendonly", "yes", "--aclfile", "/etc/redis/users.acl"]
    volumes:
      - redis_data:/data
      - ./redis/users.acl:/etc/redis/users.acl:ro
    ports:
      - "6379:6379"
    networks:
      - omni-network
    healthcheck:
      test: ["CMD", "redis-cli", "-a", "omni_admin_secret", "ping"]
      interval: 5s
      timeout: 3s
      retries: 5

  rabbitmq:
    image: rabbitmq:3.13-management-alpine
    container_name: omni-rabbitmq
    restart: unless-stopped
    environment:
      RABBITMQ_DEFAULT_USER: omni
      RABBITMQ_DEFAULT_PASS: omni_rabbit_secret
      RABBITMQ_DEFAULT_VHOST: /omni
    volumes:
      - rabbitmq_data:/var/lib/rabbitmq
    ports:
      - "5672:5672" # AMQP protocol
      - "15672:15672" # Web Management UI
    networks:
      - omni-network
    healthcheck:
      test: ["CMD", "rabbitmq-diagnostics", "-q", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

  # -------------------------------------------------------------
  # 2. TẦNG QUAN SÁT TẬP TRUNG (OBSERVABILITY STACK)
  # -------------------------------------------------------------
  prometheus:
    image: prom/prometheus:v2.50.0
    container_name: omni-prometheus
    restart: unless-stopped
    command:
      - "--config.file=/etc/prometheus/prometheus.yml"
      - "--storage.tsdb.path=/prometheus"
      - "--web.enable-lifecycle"
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
    ports:
      - "9090:9090"
    networks:
      - omni-network

  loki:
    image: grafana/loki:2.9.4
    container_name: omni-loki
    restart: unless-stopped
    volumes:
      - ./loki/loki-config.yaml:/etc/loki/local-config.yaml:ro
    ports:
      - "3100:3100"
    networks:
      - omni-network

  tempo:
    image: grafana/tempo:2.4.0
    container_name: omni-tempo
    restart: unless-stopped
    command: ["-config.file=/etc/tempo.yaml"]
    volumes:
      - ./tempo/tempo.yaml:/etc/tempo.yaml:ro
      - tempo_data:/var/tempo
    ports:
      - "3200:3200" # Tempo HTTP
      - "4317:4317" # OTLP gRPC receiver
      - "4318:4318" # OTLP HTTP receiver
    networks:
      - omni-network

  grafana:
    image: grafana/grafana:10.4.0
    container_name: omni-grafana
    restart: unless-stopped
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_USERS_ALLOW_SIGN_UP=false
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/provisioning/:/etc/grafana/provisioning/:ro
    ports:
      - "3000:3000"
    networks:
      - omni-network
    depends_on:
      - prometheus
      - loki
      - tempo

  # -------------------------------------------------------------
  # 3. TẦNG TỔNG HỢP TÀI LIỆU API (SWAGGER UI AGGREGATOR)
  # -------------------------------------------------------------
  swagger-ui:
    image: swaggerapi/swagger-ui:v5.17.14
    container_name: omni-swagger-ui
    restart: unless-stopped
    environment:
      CONFIG_URL: "/swagger-config.json"
      PERSIST_AUTHORIZATION: "true"
      DISPLAY_REQUEST_DURATION: "true"
    volumes:
      - ../kubernetes/configmaps/swagger-config.json:/usr/share/nginx/html/swagger-config.json:ro
    ports:
      - "8080:8080"
    networks:
      - omni-network
```

---

### 6.6. Trải nghiệm Hiện đại với Process Compose (`process-compose.yaml`)

Đối với các lập trình viên Rust, việc phải đóng gói Docker mỗi khi sửa 1 dòng code Rust là một cực hình gây lãng phí thời gian. Giải pháp tối ưu là sử dụng **Process Compose**: Chạy hạ tầng phụ trợ trong Docker, nhưng chạy các microservices Rust trực tiếp trên máy (_native host_) kèm công cụ hot-reload (`bacon` hoặc `cargo-watch`).

```yaml
# process-compose.yaml
version: "0.5"

processes:
  # Hạ tầng nền tảng (chạy qua Docker Compose)
  infrastructure:
    command: "docker compose -f infra/docker/docker-compose.yml up"
    readiness_probe:
      exec:
        command: "docker compose -f infra/docker/docker-compose.yml ps --status running | grep -q omni-postgres"
      initial_delay_seconds: 5
      period_seconds: 3
      timeout_seconds: 2

  # Dịch vụ Order (Chạy native với Cargo Watch)
  order-service:
    command: "cargo watch -x 'run -p services-order'"
    working_dir: "."
    depends_on:
      infrastructure:
        condition: process_healthy
    environment:
      - APP_ENV=local
      # Sử dụng tài khoản và database ĐỘC QUYỀN của Order Service
      - APP_DATABASE__URL=postgres://order_user:order_dev_secret_pass@localhost:5432/order_db
      - APP_REDIS__URL=redis://order_user:order_redis_secret@localhost:6379/0
      - APP_REDIS__KEYSPACE_PREFIX=omni:order
      - APP_SERVER__PORT=8082
      - APP_LOGGING__LEVEL=debug
      - OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

  # Dịch vụ Auth
  auth-service:
    command: "cargo watch -x 'run -p services-auth'"
    working_dir: "."
    depends_on:
      infrastructure:
        condition: process_healthy
    environment:
      - APP_ENV=local
      # Sử dụng tài khoản và database ĐỘC QUYỀN của Auth Service
      - APP_DATABASE__URL=postgres://auth_user:auth_dev_secret_pass@localhost:5432/auth_db
      - APP_REDIS__URL=redis://auth_user:auth_redis_secret@localhost:6379/0
      - APP_REDIS__KEYSPACE_PREFIX=omni:auth
      - APP_SERVER__PORT=8081
```

---

## 7. Automated Integration Testing Framework (`tests/` & Testcontainers)

### 7.1. Triết lý "Production-Like Ephemeral Infrastructure"

Để triệt tiêu hoàn toàn thảm họa _The Mocking Fallacy_ và ngăn chặn _Test Pollution_, hệ thống **OmniKubernetes** thiết lập tiêu chuẩn kiểm thử tích hợp tự động dựa trên **Testcontainers**:

1. Không yêu cầu lập trình viên phải cài đặt hay chạy sẵn bất kỳ database nào trước khi thực hiện `cargo test --all`.
2. Khi test bắt đầu, Rust framework tự động kết nối Docker Daemon để spin up các container tạm thời (PostgreSQL 16, Redis 7.2).
3. Mỗi test case được cấp phát một dynamic port ngẫu nhiên và một database hoàn toàn sạch sẽ.
4. Cơ chế **Ryuk Container Reaper** của Testcontainers đảm bảo 100% container rác sẽ bị dọn dẹp sạch sẽ ngay cả khi tiến trình test bị crash hoặc ngắt giữa chừng.

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer / CI Pipeline
    participant Cargo as cargo test -p omni-integration-tests
    participant Harness as TestContext Builder
    participant Docker as Docker Engine (Testcontainers)
    participant Ryuk as Ryuk Container Reaper
    participant Migrator as SQLx Migration Runner
    participant App as AppBuilder (In-Memory Axum)
    participant Client as TestClient (reqwest / oneshot)

    Dev->>Cargo: Thực thi cargo test --all
    Cargo->>Harness: Khởi tạo TestContext::builder()
    Harness->>Docker: Kích hoạt Ryuk Container Reaper
    Harness->>Docker: Kéo & chạy postgres:16-alpine trên port ngẫu nhiên
    Docker-->>Harness: Container Ready (vd: localhost:49152)

    Harness->>Migrator: Kết nối database & chạy omni_database::run_migrations()
    Migrator-->>Harness: Schema tạo lập hoàn tất

    Harness->>App: Lắp ráp AppBuilder với ephemeral db_pool
    App-->>Harness: Axum Router sẵn sàng

    Harness->>Client: Trả về TestClient kèm Auth Token Helper
    Client->>App: Gửi HTTP POST /api/v1/orders (Kèm JWT Token hợp lệ)
    App-->>Client: Trả về ApiResponse 201 Created
    Client->>Client: Assert assertions (status, json schema, db record)

    Note over Harness,Docker: HOÀN TẤT TEST CASE -> DỌN DẸP
    Harness->>Docker: Hủy container tạm thời
    Ryuk-->>Docker: Quét dọn toàn bộ volumes và containers rò rỉ
```

---

### 7.2. Cấu trúc Crate `omni-integration-tests` (`tests/`)

Gói kiểm thử tích hợp dùng chung được cấu hình tại gốc workspace:

```structure
tests/
├── Cargo.toml                      # Cấu hình độc lập hoặc workspace member
├── integration_k8s.rs              # Kiểm thử hợp lệ Kubernetes dry-run (đã có sẵn)
├── common/
│   ├── mod.rs                      # Module test helper dùng chung
│   ├── context.rs                  # TestContext & TestContextBuilder
│   ├── containers.rs               # PostgresContainer & RedisContainer wrappers
│   └── client.rs                   # TestClient (ergonomic HTTP calls & asserts)
├── test_service_bootstrap.rs       # Kiểm thử khởi tạo AppBuilder & 9-layer onion
├── test_resilience_integration.rs  # Kiểm thử tích hợp Rate Limit, Load Shed & Timeout
└── test_graceful_shutdown.rs       # Kiểm thử quy trình K8s shutdown 7 giai đoạn
```

---

### 7.3. Đặc tả `TestContext` & `TestContainers` Wrapper

```rust
// tests/common/context.rs

use std::sync::Arc;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::redis::Redis;
use omni_common::bootstrap::AppBuilder;
use crate::common::client::TestClient;

/// Môi trường kiểm thử tích hợp độc lập, tự động dọn dẹp sau khi chạy xong.
pub struct TestContext {
    pub db_pool: PgPool,
    pub redis_url: String,
    pub client: TestClient,
    // Giữ container reference để tránh bị drop sớm
    _postgres_node: testcontainers::ContainerAsync<Postgres>,
    _redis_node: testcontainers::ContainerAsync<Redis>,
}

impl TestContext {
    /// Khởi tạo một môi trường kiểm thử tích hợp hoàn chỉnh với Postgres và Redis thật.
    pub async fn new() -> Self {
        // 1. Kéo và khởi động PostgreSQL Container
        let postgres_node = Postgres::default()
            .with_user("omni_test")
            .with_password("omni_test_password")
            .with_db_name("test_db")
            .start()
            .await
            .expect("Không thể khởi động Postgres Testcontainer");

        let host = postgres_node.get_host().await.expect("Lỗi lấy host");
        let port = postgres_node.get_host_port_ipv4(5432).await.expect("Lỗi lấy port");
        let db_url = format!("postgres://omni_test:omni_test_password@{host}:{port}/test_db");

        // 2. Tạo kết nối Pool và chạy Migrations
        let db_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .expect("Không thể kết nối vào Postgres Testcontainer");

        // Chạy migration tự động
        sqlx::migrate!("./migrations")
            .run(&db_pool)
            .await
            .expect("Lỗi chạy SQLx migrations trên testcontainer");

        // 3. Kéo và khởi động Redis Container
        let redis_node = Redis::default()
            .start()
            .await
            .expect("Không thể khởi động Redis Testcontainer");

        let redis_port = redis_node.get_host_port_ipv4(6379).await.expect("Lỗi lấy redis port");
        let redis_url = format!("redis://{host}:{redis_port}");

        // 4. Khởi tạo TestClient bọc Axum Router
        let client = TestClient::new();

        Self {
            db_pool,
            redis_url,
            client,
            _postgres_node: postgres_node,
            _redis_node: redis_node,
        }
    }
}
```

---

## 8. Standard Domain Service Scaffold Specification ("The Golden Path")

### 8.1. Cấu trúc Chuẩn của một Microservice (`services/<name>/`)

Một microservice chuẩn mực trong thư mục `services/*` (ví dụ `services/order`) tuân thủ nghiêm ngặt mô hình **Hexagonal / Clean Architecture**, phân tách rạch ròi giữa Domain, Application, Infrastructure và Interfaces:

```structure
services/order/
├── Cargo.toml                      # Khai báo phụ thuộc từ workspace
├── Dockerfile                      # Multi-stage cache build tối ưu với cargo-chef
├── migrations/                     # SQLx Migrations riêng của service
│   └── 20261103000001_create_orders_table.sql
├── src/
│   ├── main.rs                     # Entrypoint tinh gọn (< 35 dòng)
│   ├── config.rs                   # Cấu hình service (kế thừa AppConfig)
│   ├── state.rs                    # AppState chia sẻ (Arc-wrapped, cô lập hoàn toàn)
│   ├── domain/                     # Core Business Entities & Logic (Zero external deps)
│   │   ├── mod.rs
│   │   ├── order.rs                # Aggregate Root, OrderStatus, OrderItem
│   │   └── events.rs               # OrderCreatedEvent, OrderCancelledEvent
│   ├── application/                # Use Cases / Application Services
│   │   ├── mod.rs
│   │   ├── create_order.rs
│   │   └── cancel_order.rs
│   ├── infrastructure/             # Tầng kết nối hạ tầng thực tế
│   │   ├── mod.rs
│   │   ├── order_repository.rs     # SQLx implementation
│   │   └── event_publisher.rs      # Outbox Publisher integration
│   └── interfaces/                 # Tầng giao diện người dùng & API
│       ├── mod.rs
│       ├── dtos.rs                 # Request / Response DTOs có Utoipa derive
│       ├── handlers.rs             # Axum handlers (xử lý HTTP, gọi Use Cases)
│       └── routes.rs               # Router mounting & OpenAPI path registration
└── tests/
    └── order_integration_test.rs   # Test tích hợp với Testcontainers
```

---

### 8.2. Mã nguồn Mẫu Chuẩn mực (`The Golden Path Example`)

#### 1. File `src/main.rs` (Dưới 35 dòng mã nguồn):

```rust
// services/order/src/main.rs
use std::sync::Arc;
use omni_common::bootstrap::AppBuilder;
use omni_common::config::AppConfig;
use omni_common::server::serve_with_graceful_shutdown;
use omni_common::resilience::GracefulShutdownCoordinator;
use tokio_util::sync::CancellationToken;

mod config;
mod domain;
mod infrastructure;
mod interfaces;
mod state;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Nạp cấu hình đa tầng & Khởi tạo Logging non-blocking
    let config = AppConfig::load("order-service")?;
    let _log_guard = omni_logging::init(&config.logging)?;

    // 2. Khởi tạo tài nguyên hạ tầng ĐỘC QUYỀN của Order Service
    let db_pool = omni_database::create_pool(&config.database).await?;
    let cache_manager = omni_cache::create_manager(&config.redis).await?;
    let metrics_handle = omni_metrics::init_prometheus_exporter()?;
    let shutdown_coordinator = Arc::new(GracefulShutdownCoordinator::new());
    let shutdown_token = CancellationToken::new();

    // 3. Khởi tạo Application State (chỉ chứa tài nguyên thuộc sở hữu của order-service)
    let app_state = state::AppState::new(db_pool, cache_manager);

    // 4. Lắp ráp Router bằng AppBuilder theo chuẩn 9-Layer Onion
    let app = AppBuilder::new("order-service")
        .with_state(app_state)
        .with_k8s_probes(shutdown_coordinator.drain_gate())
        .with_metrics_endpoint(metrics_handle)
        .with_openapi_docs(interfaces::ApiDoc::openapi(), config.docs.clone())
        .mount_routes("/api/v1/orders", interfaces::routes::order_router())
        .build();

    // 5. Khởi động Web Server kèm hạ cánh an toàn 7 giai đoạn
    serve_with_graceful_shutdown(app, config.server.port, shutdown_coordinator, shutdown_token).await?;

    Ok(())
}
```

---

### 8.3. Enterprise Multi-Stage Dockerfile (`cargo-chef`)

Một vấn đề nan giải khi build Docker cho các dự án Rust lớn là thời gian compile dependencies (thường mất từ 10 đến 25 phút). Bằng cách sử dụng **`cargo-chef`**, tầng dependencies được cache hoàn toàn nếu file `Cargo.lock` không thay đổi. Khi lập trình viên sửa code nghiệp vụ, thời gian build Docker image giảm xuống **dưới 30 giây**:

```dockerfile
# syntax=docker/dockerfile:1.7
# Stage 1: Chuẩn bị danh mục dependencies (Planner)
FROM lukemathwalker/cargo-chef:latest-rust-1.94-alpine AS chef
WORKDIR /app
RUN apk add --no-cache musl-dev

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Cache và build dependencies (Builder)
FROM chef AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
# Build chỉ dependencies - Tầng này sẽ được Docker cache 100% nếu Cargo.lock không đổi
RUN cargo chef cook --release --recipe-path recipe.json

# Build mã nguồn ứng dụng thực tế
COPY . .
ARG SERVICE_NAME=services-order
RUN cargo build --release --bin ${SERVICE_NAME}

# Stage 3: Tạo Image Production siêu nhẹ (Distroless / Alpine Non-Root)
FROM alpine:3.20 AS runtime
WORKDIR /app

# Tạo nhóm và người dùng không có đặc quyền (Unprivileged User)
RUN addgroup -g 10001 -S omni && \
    adduser -u 10001 -S omni -G omni

# Cài đặt dumb-init để xử lý chính xác tín hiệu PID 1 POSIX
RUN apk add --no-cache ca-certificates tzdata dumb-init

ARG SERVICE_NAME=services-order
COPY --from=builder /app/target/release/${SERVICE_NAME} /app/service-binary

# Gán quyền sở hữu
USER omni:omni

# Expose HTTP port chuẩn
EXPOSE 8080

# Chạy thông qua dumb-init để chuyển tiếp SIGTERM/SIGINT trọn vẹn
ENTRYPOINT ["/usr/bin/dumb-init", "--"]
CMD ["/app/service-binary"]
```

---

## 9. Configuration Specification (`BootstrapConfig` & `ServerConfig`)

Để hỗ trợ kiểm soát linh hoạt hành vi của `AppBuilder` và `ServerRuntime`, bảng cấu hình được bổ sung vào hệ thống quản trị cấu hình 5 tầng:

```toml
# configs/order-service/default.toml

[server]
port = 8080
host = "0.0.0.0"
keep_alive_timeout_secs = 75
client_timeout_secs = 30
# Thời gian chờ K8s Ingress rút IP khỏi EndpointSlice trước khi ngắt tiếp nhận
ingress_drain_sleep_secs = 5
# Thời gian tối đa cho phép các In-flight requests hoàn thành nốt
drain_timeout_secs = 25

[database]
# Kết nối độc quyền với user và database riêng
url = "postgres://order_user:order_secret_pass@localhost:5432/order_db"
max_connections = 25
min_connections = 5

[redis]
# Kết nối độc quyền với ACL user và keyspace prefix chuẩn
url = "redis://order_user:order_redis_secret@localhost:6379/0"
keyspace_prefix = "omni:order"

[bootstrap]
enable_observability = true
enable_metrics = true
enable_resilience = true
enable_security_headers = true

[cors]
allow_credentials = true
allowed_origins = ["https://omnikubernetes.com", "http://localhost:3000"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"]
allowed_headers = ["Authorization", "Content-Type", "X-Request-Id", "X-Locale"]
max_age_secs = 3600
```

---

## 10. Comprehensive Testing Strategy & Quality Gate

### 10.1. Ma trận Kiểm thử Tích hợp Toàn diện

```mermaid
graph TD
    subgraph TestMatrix["MA TRẬN KIỂM THỬ KHUNG SERVICE & DEV HARNESS"]
        direction TB
        T1["1. Data Isolation Tests:<br/>Kiểm tra order_user không thể đọc auth_db; Redis ACL chặn chéo key"]
        T2["2. Unit Tests:<br/>Kiểm tra thứ tự bọc Middleware & Typestate Builder"]
        T3["3. Integration Tests (Testcontainers):<br/>Khởi chạy AppBuilder với Postgres & Redis container thật"]
        T4["4. Graceful Shutdown Tests:<br/>Mô phỏng tín hiệu SIGTERM & Đo lường thời gian Drain Gate"]
        T5["5. Panic Safety Tests:<br/>Tạo panic trong handler -> Assert RFC 9457 500 Problem Details"]
        T6["6. Compose Harness Smoke Tests:<br/>Kiểm tra sức khỏe toàn bộ 7 services trong docker-compose.yml"]
    end
```

### 10.2. Tuân thủ Tiêu chuẩn Nghiệm thu (Quality Gate Protocol)

Theo quy định bắt buộc tại [`AGENTS.md`](../../AGENTS.md) và [`.agents/rules/quality-gate.md`](../../.agents/rules/quality-gate.md), bất kỳ thay đổi nào liên quan đến việc hiện thực hóa Bước 9 đều phải vượt qua 100% các bước:

```bash
# 1. Định dạng mã nguồn chuẩn mực
cargo fmt --all

# 2. Đồng bộ cây phụ thuộc workspace
cargo hakari generate

# 3. Kiểm tra cảnh báo linter nghiêm ngặt (Zero Warnings)
cargo clippy --all-targets --all-features -- -D warnings

# 4. Toàn bộ test suite phải vượt qua
cargo test --all
```

---

## 11. Implementation Roadmap & Crate Layout Plan

Khi tiến hành hiện thực hóa mã nguồn (Implementation Phase), cấu trúc file sẽ được bổ sung như sau:

```structure
libs/common/src/
├── lib.rs                          # Export module bootstrap, server, testing
├── bootstrap/
│   ├── mod.rs                      # Public exports
│   ├── builder.rs                  # AppBuilder implementation
│   ├── panic_handler.rs            # RFC 9457 Panic catcher
│   └── fallback.rs                 # Default 404 handler
├── server/
│   ├── mod.rs                      # serve_with_graceful_shutdown
│   ├── listener.rs                 # Socket2 config (SO_REUSEPORT, TCP_NODELAY)
│   └── signal.rs                   # SIGTERM / SIGINT cross-platform handler
└── testing/
    ├── mod.rs                      # Public test helpers
    ├── context.rs                  # TestContext & TestApp
    └── client.rs                   # TestClient helper
infra/
├── docker/
│   ├── docker-compose.yml          # Local infra compose
│   ├── postgres/01-init-databases.sql # Multi-DB + Multi-User setup
│   ├── redis/users.acl             # Redis ACL keyspace policy
│   ├── prometheus/prometheus.yml
│   ├── loki/loki-config.yaml
│   ├── tempo/tempo.yaml
│   └── grafana/provisioning/
└── process-compose.yaml            # Native Rust developer harness
tests/
└── integration_bootstrap_tests.rs  # Testcontainers end-to-end suite
```

---

## 12. Tổng kết & Kết luận Kiến trúc

Tài liệu đặc tả kiến trúc **Bộ khung Service mẫu, Nguyên tắc Cô lập Tài nguyên Dữ liệu & Môi trường phát triển cục bộ (Scaffold & Dev Harness)** chính là mảnh ghép cuối cùng, hoàn thiện bức tranh **BaseCode** của toàn bộ dự án **OmniKubernetes**:

1. **Chuẩn hóa Triệt để:** Lắp ghép 8 thư viện nền tảng thành một quy trình phát triển dịch vụ duy nhất thông qua `AppBuilder`.
2. **Cô lập Dữ liệu & Tự chủ Tuyệt đối:** Thiết lập nguyên tắc bất biến **Database-per-Service** (tách DB logic/vật lý, cấm Cross-Database JOINs, cấp tài khoản DB riêng biệt) và **Cache-per-Service** (Keyspace namespacing bắt buộc, phân quyền Redis ACL, dedicated clusters).
3. **Loại bỏ Bẫy Thứ tự:** Cố định 9 tầng Middleware Onion, đảm bảo bối cảnh phân tán, metrics, bảo mật và khả năng phục hồi luôn hoạt động chuẩn xác.
4. **Trải nghiệm Lập trình Xuất sắc:** Cung cấp môi trường cục bộ 1 lệnh duy nhất (`docker-compose.yml`, `process-compose.yaml`) và quy trình kiểm thử cô lập hoàn toàn (`Testcontainers`), giúp các kỹ sư tập trung 100% vào nghiệp vụ mà không bị cản trở bởi hạ tầng.
5. **Vận hành Đẳng cấp Doanh nghiệp:** Tích hợp mặc định quy trình hạ cánh an toàn 7 giai đoạn Kubernetes, xóa sổ hoàn toàn lỗi 502 khi rolling update.

Tài liệu này đóng vai trò kim chỉ nam kỹ thuật chuẩn mực, sẵn sàng cho giai đoạn hiện thực hóa mã nguồn chi tiết tiếp theo.
