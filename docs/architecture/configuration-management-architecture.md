# OmniKubernetes Architecture Specification: Enterprise Type-Safe Configuration & Environment Binding Standard

- **Document Version:** `1.0.0`
- **Status:** Approved / Enterprise Architecture Standard
- **Author:** System Architecture Team
- **Target Platform:** Rust (2024 Edition, Rust 1.85+), Tokio Runtime (1.43+), Axum Framework (0.8+), Kubernetes (ConfigMap, Secret, Downward API, External Secrets Operator)

---

## 1. Executive Summary & Core Principles

Trong hệ sinh thái microservices quy mô doanh nghiệp (**Enterprise Microservices on Kubernetes**) của **OmniKubernetes**, mọi thành phần nền tảng từ Web Server, Hệ thống Quan sát (`omni-logging`, `omni-metrics`), Cơ sở dữ liệu (`omni-database`), Bộ nhớ đệm (`omni-cache`), Bảo mật (`omni-auth`) cho tới Hàng đợi sự kiện (`omni-messaging`) đều phụ thuộc trực tiếp vào các tham số cấu hình lúc khởi chạy.

Việc để từng microservice tự ý gọi `std::env::var` rải rác trong mã nguồn nghiệp vụ là một **phản mẫu kiến trúc (Architectural Anti-Pattern)** nghiêm trọng, dẫn đến:

- **Lỗi thời gian chạy muộn (Late Runtime Crashes):** Sai chính tả tên biến môi trường hoặc sai kiểu dữ liệu chỉ bị phát hiện khi có luồng request thực tế đi vào nhánh code đó trên môi trường Production.
- **Rò rỉ thông tin mật (Secret Leakage):** Mật khẩu cơ sở dữ liệu, khóa ký JWT hoặc chuỗi kết nối Redis lưu dưới dạng `String` thông thường dễ dàng bị in phơi bày ra hệ thống log tập trung (Grafana Loki) khi lập trình viên vô tình gọi `tracing::info!("{:?}", config)`.
- **Phân mảnh quy ước (Configuration Drift):** Mỗi service đặt tên biến môi trường theo một kiểu khác nhau (`DB_URL`, `POSTGRES_URI`, `DATABASE_HOST`), gây hỗn loạn cho đội ngũ DevOps/SRE khi vận hành Helm Charts và Kubernetes Manifests.

Tiêu chuẩn kiến trúc này xác lập **6 nguyên lý cốt lõi (Core Architectural Tenets)** cho hệ thống quản lý cấu hình của **OmniKubernetes**:

1. **Phân cấp cấu hình đa tầng (5-Layer Hierarchical Resolution):**
   - Nạp và hợp nhất cấu hình theo đúng thứ tự ưu tiên tăng dần:
     1. **Tầng 1 (Compiled-in Safe Defaults):** Giá trị mặc định an toàn trong mã nguồn Rust (`Default` trait) cho các tham số vận hành phi nhạy cảm.
     2. **Tầng 2 (Base YAML):** Tập tin cấu hình nền tảng dùng chung `config/base.yaml`.
     3. **Tầng 3 (Environment Profile YAML):** Tập tin cấu hình chuyên biệt theo môi trường `config/{env}/default.yaml` (hoặc `config/{env}.yaml` với `{env}` thuộc `dev`, `staging`, `prod`).
     4. **Tầng 4 (System Environment Variables):** Biến môi trường hệ thống mang tiền tố chuẩn `OMNI__` (kèm hỗ trợ nạp `.env` cục bộ qua `dotenvy` duy nhất ở môi trường `local`/`dev`).
     5. **Tầng 5 (Programmatic & Test Overrides):** Tham số ghi đè trực tiếp trong mã nguồn phục vụ kiểm thử tự động (`cargo test`) và công cụ dòng lệnh (CLI).
2. **Thẩm định tức thời khi khởi động (Fail-Fast Startup Validation):**
   - Kết hợp **Deserialization-Time Type Safety** (`serde` cùng hệ thống kiểu mạnh của Rust như `NonZeroU16`, `NonZeroUsize`, `IpAddr`) với **Multi-Rule Semantic Validation** (`validator::Validate` và kiểm tra ràng buộc chéo liên trường).
   - Nếu bất kỳ biến bắt buộc nào bị thiếu, sai định dạng, hoặc vi phạm quy tắc an toàn vận hành, tiến trình phải dừng ngay lập tức (**Fail-Fast**) ở pha Bootstrap trước khi mở cổng TCP nhận traffic, đồng thời xuất báo cáo tổng hợp toàn bộ lỗi cấu hình trong một lần duy nhất.
3. **Bảo mật Bí mật Tuyệt đối (Zero-Leak Secret Management & Memory Zeroization):**
   - Toàn bộ các trường nhạy cảm (Database Password, Redis Password, JWT Signing Key, Webhook Secret, Broker Credentials) bắt buộc phải được bọc trong kiểu `secrecy::SecretString`.
   - Vô hiệu hóa hoàn toàn khả năng in giá trị thật qua `fmt::Debug`, `fmt::Display` hoặc `serde::Serialize` (luôn hiển thị `"[REDACTED]"`).
   - Tự động xóa sạch vùng nhớ heap (`zeroize`) ngay khi đối tượng cấu hình kết thúc vòng đời (`Drop`).
4. **Kiến trúc Hợp thành & Mở rộng Kiểu tĩnh (Composable & Extensible Schema):**
   - Cung cấp cấu trúc gốc `AppConfig<C = EmptyCustomConfig>` chứa sẵn toàn bộ các phân hệ hạ tầng dùng chung (`service`, `server`, `logging`, `database`, `redis`, `auth`, `messaging`, `resilience`, `observability`, `i18n`).
   - Cho phép từng Domain Microservice (`order`, `payment`, `user`) gắn thêm cấu hình nghiệp vụ đặc thù thông qua tham số generic `C` (`#[serde(flatten)] pub custom: C`) mà vẫn hưởng trọn vẹn cơ chế nạp đa tầng và kiểm tra hợp lệ tự động.
5. **Tương thích Bản địa với Kubernetes (Kubernetes-Native Binding):**
   - Tách biệt rõ ràng giữa **Cấu hình phi nhạy cảm** (mount từ Kubernetes `ConfigMap`) và **Thông tin định danh mật** (tiêm từ Kubernetes `Secret` / HashiCorp Vault / External Secrets Operator qua biến môi trường `OMNI__*`).
   - Tự động thu thập siêu dữ liệu định danh Pod từ **Kubernetes Downward API** (`K8S_POD_NAME`, `K8S_NAMESPACE`, `K8S_NODE_NAME`) để làm giàu ngữ cảnh cho `omni-logging` và OpenTelemetry Resource.
6. **Bất biến trong Runtime & Kiểm thử Không Mã `unsafe` (Immutable Runtime & Safe Testability):**
   - Sau khi khởi tạo thành công, `AppConfig` được đóng băng bất biến và chia sẻ qua `Arc<AppConfig<C>>` với chi phí khóa bằng 0 (`Zero-Lock Overhead`).
   - Tuân thủ nghiêm ngặt quy định `unsafe_code = "deny"` của Rust 2024 Edition (trong đó `std::env::set_var` là hàm `unsafe`) bằng cơ chế tiêm nguồn biến môi trường cô lập (`ConfigSource` / In-Memory Env Map) phục vụ chạy kiểm thử song song an toàn tuyệt đối.

---

## 2. Scope of Applicability (Phạm vi áp dụng)

### 2.1. Nằm trong phạm vi (In-Scope)

- **Module nền tảng `libs/common/src/config/` (`omni_common::config`):** Chứa bộ nạp cấu hình đa tầng `ConfigLoader`, cấu trúc `AppConfig<C>`, định nghĩa các khối cấu hình thành phần, bộ kiểm tra hợp lệ (`Validate`), và cơ chế bảo vệ bí mật (`SecretString`).
- **Hệ thống tập tin cấu hình chuẩn tại thư mục gốc `config/`:** Bao gồm `config/base.yaml`, `config/dev/`, `config/staging/`, `config/prod/` và tập tin mẫu `.env.example`.
- **Giao thức ánh xạ biến môi trường toàn cục (`OMNI__*`):** Quy ước đặt tên biến môi trường thống nhất cho toàn bộ 7 microservices và các thư viện dùng chung trong workspace.
- **Cầu nối cấu hình sang các Crate nền tảng:** Chuyển đổi trực tiếp không chi phí (Zero-Friction Adapter) sang `omni_logging::LoggingConfig`, và cung cấp cấu trúc chuẩn cho `omni-metrics`, `omni-database`, `omni-cache`, `omni-auth`, `omni-messaging`.

### 2.2. Phân định ranh giới (Single Source of Truth & Exclusions)

- **Nghiêm cấm truy cập trực tiếp `std::env::var` ở tầng nghiệp vụ:** Mọi crate trong `services/*` và các tầng xử lý request (`handlers`, `services`, `repositories`, `middleware`) không được phép gọi `std::env::var`. Toàn bộ tham số bắt buộc phải đi qua `AppConfig`.
- **Phân biệt Cấu hình Khởi động Tĩnh (Static Startup Config) và Tài nguyên Nạp nóng (Hot-Reloadable Runtime Assets):**
  - `AppConfig` quản lý các thông số hạ tầng mang tính bất biến theo vòng đời Pod (như cổng lắng nghe, kích thước connection pool, địa chỉ cơ sở dữ liệu). Khi thay đổi các thông số này trên Kubernetes, quy trình chuẩn là cập nhật `ConfigMap`/`Secret` và thực hiện Rolling Restart Pod.
  - Các tài nguyên hỗ trợ nạp nóng không gián đoạn (như gói ngôn ngữ Mozilla Fluent `.ftl` được reload qua cơ chế RCU `ArcSwap` và tín hiệu `SIGHUP`) vẫn lấy đường dẫn khởi tạo ban đầu từ `AppConfig.i18n`, nhưng vòng đời cập nhật nội dung do `I18nManager` đảm nhiệm.

---

## 3. Standards & Compliance References

| Tiêu chuẩn / Đặc tả kỹ thuật                     | Cơ quan ban hành      | Ứng dụng cụ thể trong hệ thống OmniKubernetes                                                                                |
| :----------------------------------------------- | :-------------------- | :--------------------------------------------------------------------------------------------------------------------------- |
| **The Twelve-Factor App (Factor III)**           | Heroku / Cloud Native | Tách biệt triệt để cấu hình môi trường và thông tin bí mật khỏi mã nguồn biên dịch.                                          |
| **OWASP Secrets Management Cheat Sheet**         | OWASP Foundation      | Ngăn chặn rò rỉ thông tin xác thực qua log, stack trace, debug dump; xóa sạch vùng nhớ nhạy cảm sau khi sử dụng (`zeroize`). |
| **CNCF Kubernetes Configuration Best Practices** | CNCF / Kubernetes     | Phân tách `ConfigMap` và `Secret`, tích hợp Downward API và hỗ trợ container bất biến (`gcr.io/distroless`).                 |
| **RFC 3986 (URI Generic Syntax)**                | IETF                  | Kiểm tra tính hợp lệ của các chuỗi định danh kết nối hạ tầng (`postgres://`, `redis://`, `http://`, `https://`).             |
| **Rust 2024 Edition Safety Standard**            | Rust Project          | Đảm bảo Thread-Safety tuyệt đối với môi trường đa luồng; không sử dụng `unsafe { std::env::set_var }` trong kiểm thử.        |
| **Nghị định 13/2023/NĐ-CP & ISO/IEC 27001**      | Chính phủ VN / ISO    | Bảo vệ khóa mã hóa dữ liệu cá nhân, mật khẩu hệ thống và khóa ký xác thực dịch vụ.                                           |

---

## 4. Hierarchical Configuration Resolution Architecture

### 4.1. Sơ đồ Luồng Nạp và Hợp nhất Cấu hình (5-Stage Resolution Pipeline)

```mermaid
flowchart TD
    subgraph Sources["5 TẦNG NGUỒN CẤU HÌNH (THỨ TỰ ƯU TIÊN TĂNG DẦN)"]
        direction TB
        L1["Tầng 1: Compiled-in Safe Defaults<br/>(impl Default cho các thông số vận hành an toàn)"]
        L2["Tầng 2: Base Configuration File<br/>(config/base.yaml)"]
        L3["Tầng 3: Environment Profile File<br/>(config/{env}/default.yaml hoặc config/{env}.yaml)"]
        L4["Tầng 4: Environment Variables & Local .env<br/>(Tiền tố OMNI__* + K8s Downward API)"]
        L5["Tầng 5: Programmatic / Test / CLI Overrides<br/>(ConfigLoader::with_override / In-Memory Map)"]

        L1 -->|"Merge"| L2
        L2 -->|"Override"| L3
        L3 -->|"Override"| L4
        L4 -->|"Override"| L5
    end

    L5 --> Deser["Serde Type-Safe Deserialization<br/>(Ánh xạ vào AppConfig&lt;C&gt; &amp; SecretString)"]

    Deser -->|"Lỗi kiểu dữ liệu / Thiếu trường bắt buộc"| FailFast["FAIL-FAST STARTUP ABORT<br/>(In báo cáo lỗi chi tiết &amp; Exit Code 1)"]
    Deser -->|"Thành công"| Phase1Val["Phase 1: Field-Level Validation<br/>(validator::Validate - Range, URL, Length, NonEmpty)"]

    Phase1Val -->|"Vi phạm ràng buộc trường"| FailFast
    Phase1Val -->|"Hợp lệ"| Phase2Val["Phase 2: Cross-Field &amp; Production Guardrails<br/>(min_conn &lt;= max_conn, Timeout hierarchy, Prod TLS/Swagger check)"]

    Phase2Val -->|"Vi phạm bất biến chéo"| FailFast
    Phase2Val -->|"Hợp lệ 100%"| Freeze["Đóng băng vào Arc&lt;AppConfig&lt;C&gt;&gt;<br/>&amp; Ghi Log Tóm tắt Đã Che giấu Secret ([REDACTED])"]
```

### 4.2. Chi tiết Thứ tự Ưu tiên 5 Tầng (Precedence Rules)

Hệ thống nạp cấu hình theo cơ chế **Deep Merge (Hợp nhất sâu theo từng khóa lá)**. Tầng đứng sau chỉ ghi đè đúng trường được chỉ định mà không làm mất các trường lân cận trong cùng một khối struct đã được định nghĩa ở tầng trước:

| Thứ tự | Tầng cấu hình                    | Nguồn dữ liệu                                          | Mục đích sử dụng chính                                                                                                          | Bắt buộc tồn tại |
| :----: | :------------------------------- | :----------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------ | :--------------: |
| **1**  | **Compiled-in Safe Defaults**    | `#[serde(default)]` & `impl Default`                   | Cung cấp giá trị chuẩn cho các tham số kỹ thuật phổ thông (`port = 8080`, `buffered_lines_limit = 10000`, `lossy = true`).      |      Có sẵn      |
| **2**  | **Base YAML**                    | `config/base.yaml`                                     | Định nghĩa khung cấu hình chuẩn dùng chung cho mọi môi trường, giúp giảm thiểu lặp lại cấu hình giữa `dev`, `staging`, `prod`.  |   Khuyến nghị    |
| **3**  | **Environment Profile YAML**     | `config/{env}/default.yaml` hoặc `config/{env}.yaml`   | Tùy chỉnh tham số theo từng môi trường (ví dụ: `dev` dùng `log_format: pretty`, `prod` dùng `log_format: json` và pool lớn).    |     Tùy chọn     |
| **4**  | **System Environment Variables** | Biến môi trường `OMNI__*` (và `.env` ở môi trường Dev) | Tiêm thông tin mật từ Kubernetes `Secret` (`OMNI__DATABASE__PASSWORD`) hoặc ghi đè nhanh qua `ConfigMap` mà không cần sửa file. |   Theo dịch vụ   |
| **5**  | **Programmatic Overrides**       | `ConfigLoader::with_override(key, value)`              | Ghi đè cổng ngẫu nhiên hoặc chuỗi kết nối container động (`testcontainers`) trong quá trình chạy Integration Tests.             |     Tùy chọn     |

### 4.3. Quy ước Ánh xạ Biến Môi trường (The Double-Underscore `OMNI__` Convention)

Một trong những lỗi kinh điển nhất khi sử dụng thư viện `config` trong Rust là dùng dấu gạch dưới đơn `_` làm ký tự phân tách cấp độ struct. Nếu dùng `_` đơn, biến môi trường `OMNI_DATABASE_MAX_CONNECTIONS` sẽ bị bộ phân tích cú pháp tách nhầm thành đường dẫn cây `database.max.connections` thay vì trường `max_connections` nằm trong khối `database`.

Để giải quyết triệt để vấn đề này, **OmniKubernetes** quy định chuẩn bắt buộc:

- **Tiền tố toàn cục (Global Prefix):** `OMNI`
- **Dấu phân tách tiền tố và cấp độ lồng nhau (Hierarchy Separator):** Dấu gạch dưới kép `__` (Double Underscore).
- **Dấu nối từ trong tên trường Rust (Field Name Word Separator):** Dấu gạch dưới đơn `_` (Single Underscore, khớp 1:1 với tên trường `snake_case` trong Rust struct).

#### Bảng ánh xạ minh họa chuẩn `OMNI__`

| Biến môi trường hệ thống (`System Env`) | Đường dẫn cấu hình YAML (`YAML Path`) | Trường trong Rust Struct (`AppConfig`) | Kiểu dữ liệu đích      |
| :-------------------------------------- | :------------------------------------ | :------------------------------------- | :--------------------- |
| `OMNI__SERVICE__NAME`                   | `service.name`                        | `config.service.name`                  | `String`               |
| `OMNI__SERVICE__ENVIRONMENT`            | `service.environment`                 | `config.service.environment`           | `AppEnvironment`       |
| `OMNI__SERVER__HOST`                    | `server.host`                         | `config.server.host`                   | `IpAddr`               |
| `OMNI__SERVER__PORT`                    | `server.port`                         | `config.server.port`                   | `u16`                  |
| `OMNI__SERVER__SHUTDOWN_TIMEOUT_SECS`   | `server.shutdown_timeout_secs`        | `config.server.shutdown_timeout_secs`  | `u64`                  |
| `OMNI__LOGGING__LOG_LEVEL`              | `logging.log_level`                   | `config.logging.log_level`             | `String`               |
| `OMNI__LOGGING__LOG_FORMAT`             | `logging.log_format`                  | `config.logging.log_format`            | `LogFormat`            |
| `OMNI__LOGGING__OTLP_ENDPOINT`          | `logging.otlp_endpoint`               | `config.logging.otlp_endpoint`         | `Option<String>`       |
| `OMNI__DATABASE__HOST`                  | `database.host`                       | `config.database.host`                 | `String`               |
| `OMNI__DATABASE__MAX_CONNECTIONS`       | `database.max_connections`            | `config.database.max_connections`      | `u32`                  |
| `OMNI__DATABASE__PASSWORD`              | `database.password`                   | `config.database.password`             | `SecretString`         |
| `OMNI__REDIS__PASSWORD`                 | `redis.password`                      | `config.redis.password`                | `Option<SecretString>` |
| `OMNI__AUTH__JWT_SECRET`                | `auth.jwt_secret`                     | `config.auth.jwt_secret`               | `SecretString`         |

#### Hỗ trợ Tương thích Biến Môi trường Chuẩn Cloud-Native (Standard Alias Fallbacks)

Bên cạnh tiền tố `OMNI__`, bộ nạp cấu hình tự động nhận diện các biến môi trường tiêu chuẩn công nghiệp của hệ sinh thái Rust, OpenTelemetry và Kubernetes Downward API nếu biến `OMNI__*` tương ứng chưa được đặt:

- **Xác định môi trường (`service.environment`):** `OMNI__SERVICE__ENVIRONMENT` -> `APP_ENV` -> `RUST_ENV` -> `ENV` -> Mặc định `"dev"`.
- **Cấp độ ghi log (`logging.log_level`):** `OMNI__LOGGING__LOG_LEVEL` -> `RUST_LOG` -> `LOG_LEVEL` -> Mặc định `"info"`.
- **Điểm cuối OpenTelemetry (`logging.otlp_endpoint`):** `OMNI__LOGGING__OTLP_ENDPOINT` -> `OTEL_EXPORTER_OTLP_ENDPOINT` -> `OTLP_ENDPOINT`.
- **Siêu dữ liệu Kubernetes Pod (`k8s.*`):**
  - `K8S_POD_NAME` (hoặc `HOSTNAME` trên Linux container) -> `config.k8s.pod_name`.
  - `K8S_NAMESPACE` -> `config.k8s.namespace`.
  - `K8S_NODE_NAME` -> `config.k8s.node_name`.

---

## 5. Comprehensive Data Contracts & Rust Type System Specification

### 5.1. Tổ chức Cây thư mục trong `libs/common` và Workspace Root

```text
OmniKubernetes/
├── config/                                    <-- Thư mục cấu hình chuẩn cấp Workspace
│   ├── base.yaml                              <-- Cấu hình nền tảng dùng chung cho mọi môi trường
│   ├── dev/
│   │   └── default.yaml                       <-- Cấu hình ghi đè cho môi trường Development / Local
│   ├── staging/
│   │   └── default.yaml                       <-- Cấu hình ghi đè cho môi trường Staging / UAT
│   └── prod/
│       └── default.yaml                       <-- Cấu hình ghi đè cho môi trường Production (Hardened)
├── .env.example                               <-- Mẫu biến môi trường cho lập trình viên Local
└── libs/
    └── common/
        ├── Cargo.toml
        └── src/
            ├── lib.rs
            └── config/                        <-- Module quản lý cấu hình kiểu tĩnh
                ├── mod.rs                     <-- AppConfig<C>, EmptyCustomConfig, re-exports
                ├── loader.rs                  <-- ConfigLoader, 5-layer resolution & env alias mapping
                ├── env.rs                     <-- AppEnvironment enum & K8sRuntimeMeta
                ├── secret.rs                  <-- SecretString helpers, redacted URL formatter
                ├── error.rs                   <-- ConfigError & Fail-Fast diagnostic report
                ├── server.rs                  <-- HttpServerConfig, GrpcServerConfig, CorsConfig
                ├── database.rs                <-- DatabaseConfig, DbSslMode
                ├── redis.rs                   <-- RedisConfig
                ├── auth.rs                    <-- AuthConfig, JwtAlgorithm, Argon2Config
                ├── messaging.rs               <-- MessagingConfig, OutboxConfig
                ├── resilience.rs              <-- ResilienceConfig, RateLimitConfig
                ├── observability.rs           <-- ObservabilityConfig & LoggingConfig bridge
                └── i18n.rs                    <-- I18nConfig
```

### 5.2. Định danh Môi trường Thực thi (`AppEnvironment`)

```rust
use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

/// Môi trường vận hành chuẩn hóa của hệ thống OmniKubernetes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppEnvironment {
    /// Môi trường phát triển cục bộ trên máy lập trình viên hoặc Docker Compose.
    #[default]
    #[serde(alias = "local", alias = "development")]
    Dev,
    /// Môi trường kiểm thử tích hợp / nghiệm thu trước khi lên Production.
    #[serde(alias = "uat", alias = "preprod")]
    Staging,
    /// Môi trường vận hành thực tế phục vụ người dùng cuối (Áp dụng các quy tắc bảo mật nghiêm ngặt nhất).
    #[serde(alias = "production")]
    Prod,
    /// Môi trường chạy kiểm thử tự động (Unit / Integration Tests).
    #[serde(alias = "testing")]
    Test,
}

impl AppEnvironment {
    /// Tên thư mục cấu hình tương ứng trong `config/{env}/default.yaml`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
            Self::Test => "test",
        }
    }

    /// Kiểm tra có phải môi trường Production hay không.
    #[must_use]
    pub const fn is_prod(&self) -> bool {
        matches!(self, Self::Prod)
    }

    /// Kiểm tra có cho phép nạp tập tin `.env` cục bộ hay không (Chỉ bật ở Dev).
    #[must_use]
    pub const fn allows_dotenv(&self) -> bool {
        matches!(self, Self::Dev)
    }
}

impl fmt::Display for AppEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AppEnvironment {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dev" | "local" | "development" => Ok(Self::Dev),
            "staging" | "uat" | "preprod" => Ok(Self::Staging),
            "prod" | "production" => Ok(Self::Prod),
            "test" | "testing" => Ok(Self::Test),
            other => Err(format!(
                "Invalid environment '{other}'. Expected one of: dev, local, staging, prod, test"
            )),
        }
    }
}
```

### 5.3. Cấu trúc Gốc `AppConfig<C>` và Các Khối Thành phần Nền tảng

```rust
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Struct rỗng mặc định cho các Microservices không yêu cầu cấu hình nghiệp vụ mở rộng riêng.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyCustomConfig;

impl Validate for EmptyCustomConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

/// Cấu trúc cấu hình tổng thể kiểu tĩnh cho một Microservice trong OmniKubernetes.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AppConfig<C = EmptyCustomConfig>
where
    C: Validate,
{
    /// Thông tin định danh dịch vụ (Tên, phiên bản, môi trường).
    #[validate(nested)]
    #[serde(default)]
    pub service: ServiceMetaConfig,

    /// Thông tin định danh Pod từ Kubernetes Downward API.
    #[serde(default)]
    pub k8s: K8sRuntimeMeta,

    /// Cấu hình máy chủ HTTP (Axum) và gRPC nội bộ (Tonic).
    #[validate(nested)]
    #[serde(default)]
    pub server: HttpServerConfig,

    /// Cấu hình hệ thống Structured Logging, PII Masking & OpenTelemetry OTLP.
    #[validate(nested)]
    #[serde(default)]
    pub logging: LoggingSectionConfig,

    /// Cấu hình quan sát bổ sung (Prometheus Metrics & OpenAPI Specification).
    #[validate(nested)]
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// Cấu hình kết nối PostgreSQL (`omni-database`).
    #[validate(nested)]
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Cấu hình kết nối Redis (`omni-cache`).
    #[validate(nested)]
    #[serde(default)]
    pub redis: RedisConfig,

    /// Cấu hình xác thực JWT & băm mật khẩu Argon2 (`omni-auth`).
    #[validate(nested)]
    #[serde(default)]
    pub auth: AuthConfig,

    /// Cấu hình giao tiếp hướng sự kiện & Transactional Outbox (`omni-messaging`).
    #[validate(nested)]
    #[serde(default)]
    pub messaging: MessagingConfig,

    /// Cấu hình khả năng chịu lỗi mạng, Rate Limiting & Circuit Breaker.
    #[validate(nested)]
    #[serde(default)]
    pub resilience: ResilienceConfig,

    /// Cấu hình đa ngôn ngữ Mozilla Fluent (`omni-common::i18n`).
    #[validate(nested)]
    #[serde(default)]
    pub i18n: I18nConfig,

    /// Cấu hình mở rộng chuyên biệt của từng Domain Microservice.
    #[validate(nested)]
    #[serde(default)]
    pub custom: C,
}
```

### 5.4. Đặc tả Chi tiết Các Khối Cấu hình Hạ tầng

#### A. Định danh Dịch vụ (`ServiceMetaConfig`) & Máy chủ (`HttpServerConfig`)

```rust
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use serde::{Deserialize, Serialize};
use validator::Validate;
use crate::config::env::AppEnvironment;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ServiceMetaConfig {
    /// Tên định danh của microservice (ví dụ: `"order-service"`).
    #[validate(length(min = 2, max = 64, message = "service.name must be between 2 and 64 characters"))]
    pub name: String,

    /// Phiên bản ngữ nghĩa (SemVer) của dịch vụ.
    #[validate(length(min = 1, max = 32))]
    pub version: String,

    /// Môi trường triển khai hiện tại (`dev`, `staging`, `prod`, `test`).
    pub environment: AppEnvironment,

    /// Khu vực đám mây hoặc trung tâm dữ liệu (ví dụ: `"ap-southeast-1"`).
    #[validate(length(min = 1, max = 64))]
    pub region: String,
}

impl Default for ServiceMetaConfig {
    fn default() -> Self {
        Self {
            name: "omni-service".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: AppEnvironment::Dev,
            region: "ap-southeast-1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HttpServerConfig {
    /// Địa chỉ IP lắng nghe (Mặc định `0.0.0.0` trong container Kubernetes).
    pub host: IpAddr,

    /// Cổng phục vụ REST API (Bắt buộc >= 1024 để chạy với non-root user UID 65532).
    #[validate(range(min = 1024, max = 65535, message = "server.port must be in unprivileged range 1024..=65535"))]
    pub port: u16,

    /// Cổng phục vụ gRPC nội bộ East-West traffic (Tùy chọn, mặc định `50051`).
    #[validate(range(min = 1024, max = 65535, message = "server.grpc_port must be in unprivileged range 1024..=65535"))]
    pub grpc_port: u16,

    /// Thời gian tối đa xử lý một HTTP request trước khi cắt kết nối (giây).
    #[validate(range(min = 1, max = 300, message = "server.request_timeout_secs must be between 1 and 300 seconds"))]
    pub request_timeout_secs: u64,

    /// Thời gian cửa sổ chờ (Grace Period) khi nhận tín hiệu `SIGTERM` từ Kubernetes (giây).
    #[validate(range(min = 1, max = 120, message = "server.shutdown_timeout_secs must be between 1 and 120 seconds"))]
    pub shutdown_timeout_secs: u64,

    /// Giới hạn dung lượng tối đa của HTTP Request Body (bytes, mặc định 2 MiB).
    #[validate(range(min = 1024, max = 104_857_600))]
    pub max_body_size_bytes: usize,

    /// Danh sách các dải IP / CIDR của Reverse Proxy tin cậy (Envoy Gateway).
    #[serde(default)]
    pub trusted_proxies: Vec<String>,

    /// Cấu hình bảo mật CORS.
    #[validate(nested)]
    #[serde(default)]
    pub cors: CorsConfig,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 8080,
            grpc_port: 50051,
            request_timeout_secs: 30,
            shutdown_timeout_secs: 15,
            max_body_size_bytes: 2 * 1024 * 1024, // 2 MiB
            trusted_proxies: Vec::new(),
            cors: CorsConfig::default(),
        }
    }
}

impl HttpServerConfig {
    /// Trả về `SocketAddr` kết hợp giữa `host` và `port` phục vụ `tokio::net::TcpListener::bind`.
    #[must_use]
    pub const fn http_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    /// Trả về `SocketAddr` cho máy chủ gRPC nội bộ.
    #[must_use]
    pub const fn grpc_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.grpc_port)
    }
}
```

#### B. Cấu hình Cơ sở dữ liệu Quan hệ (`DatabaseConfig`) & Bảo vệ Chuỗi Kết nối

```rust
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DbSslMode {
    Disable,
    #[default]
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl DbSslMode {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DatabaseConfig {
    /// Cờ kích hoạt kết nối cơ sở dữ liệu (cho phép tắt trên các stateless gateway services).
    pub enabled: bool,

    /// Máy chủ PostgreSQL.
    #[validate(length(min = 1, message = "database.host must not be empty"))]
    pub host: String,

    /// Cổng PostgreSQL (mặc định `5432`).
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Tên người dùng cơ sở dữ liệu.
    #[validate(length(min = 1, message = "database.username must not be empty"))]
    pub username: String,

    /// Mật khẩu cơ sở dữ liệu được bọc bảo mật bằng `SecretString` (Zero-Leak).
    pub password: SecretString,

    /// Tên cơ sở dữ liệu.
    #[validate(length(min = 1, message = "database.database_name must not be empty"))]
    pub database_name: String,

    /// Chế độ mã hóa TLS/SSL cho kết nối PostgreSQL.
    #[serde(default)]
    pub ssl_mode: DbSslMode,

    /// Số lượng kết nối tối đa trong `PgPool`.
    #[validate(range(min = 1, max = 500))]
    pub max_connections: u32,

    /// Số lượng kết nối duy trì tối thiểu trong `PgPool`.
    #[validate(range(min = 0, max = 500))]
    pub min_connections: u32,

    /// Thời gian chờ tối đa khi mượn kết nối từ Pool (giây).
    #[validate(range(min = 1, max = 120))]
    pub acquire_timeout_secs: u64,

    /// Thời gian đóng kết nối nhàn rỗi (giây).
    #[validate(range(min = 10, max = 3600))]
    pub idle_timeout_secs: u64,

    /// Thời gian sống tối đa của một kết nối vật lý trước khi tái tạo (giây).
    #[validate(range(min = 60, max = 86400))]
    pub max_lifetime_secs: u64,

    /// Ngưỡng thời gian cảnh báo truy vấn chậm (Slow Query Threshold, mili-giây).
    #[validate(range(min = 10, max = 60000))]
    pub slow_query_threshold_ms: u64,

    /// Tự động thực thi SQLx migrations khi khởi động Pod.
    pub auto_migrate: bool,
}

impl DatabaseConfig {
    /// Tạo chuỗi kết nối đầy đủ bọc trong `SecretString` để truyền vào `PgPoolOptions`.
    #[must_use]
    pub fn connection_url(&self) -> SecretString {
        SecretString::from(format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode.as_str()
        ))
    }

    /// Tạo chuỗi kết nối đã che mật khẩu phục vụ ghi log chẩn đoán an toàn.
    #[must_use]
    pub fn redacted_url(&self) -> String {
        format!(
            "postgres://{}:[REDACTED]@{}:{}/{}?sslmode={}",
            self.username,
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode.as_str()
        )
    }
}
```

#### C. Cầu nối Tương thích sang `omni-logging` (`LoggingSectionConfig`)

Để đảm bảo mẫu khởi tạo **Golden Path** trong `RoadMap.md` hoạt động liền mạch mà không buộc lập trình viên phải khai báo lặp lại `service.name` và `service.environment` ở cả mục `service` lẫn mục `logging`:

```rust
use omni_logging::{LogFormat, LoggingConfig};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LoggingSectionConfig {
    /// Cấp độ lọc log mặc định (ví dụ: `"info"`, `"debug,hyper=warn"`).
    #[validate(length(min = 1, message = "logging.log_level must not be empty"))]
    pub log_level: String,

    /// Định dạng xuất log (`json` cho Staging/Prod, `pretty` cho Local Dev).
    pub log_format: LogFormat,

    /// Kích thước bộ đệm hàng đợi non-blocking (số dòng log tối đa).
    #[validate(range(min = 128, max = 1_000_000))]
    pub buffered_lines_limit: usize,

    /// Cho phép drop log mới khi bộ đệm đầy để bảo vệ Tokio reactor không bị treo.
    pub lossy: bool,

    /// Địa chỉ gRPC của OpenTelemetry Collector (ví dụ: `"http://otel-collector:4317"`).
    #[validate(url(message = "logging.otlp_endpoint must be a valid URL"))]
    pub otlp_endpoint: Option<String>,
}

impl AppConfig {
    /// Chuyển đổi cấu hình sang `omni_logging::LoggingConfig`, tự động kế thừa
    /// `service.name` và `service.environment` từ `ServiceMetaConfig`.
    #[must_use]
    pub fn to_logging_config(&self) -> LoggingConfig {
        LoggingConfig {
            service_name: self.service.name.clone(),
            environment: self.service.environment.as_str().to_string(),
            log_level: self.logging.log_level.clone(),
            log_format: self.logging.log_format,
            buffered_lines_limit: self.logging.buffered_lines_limit,
            lossy: self.logging.lossy,
            otlp_endpoint: self.logging.otlp_endpoint.clone(),
        }
    }
}
```

---

## 6. Zero-Leak Secret Management & Cryptographic Hygiene

### 6.1. Cơ chế Bảo vệ Bí mật Đa lớp với `secrecy` và `zeroize`

Trong kiến trúc **OmniKubernetes**, mọi dữ liệu xác thực nhạy cảm đều tuân thủ quy tắc **"Opaque by Default, Explicit at Use"** (Mặc định che mờ, Tường minh khi sử dụng):

1. **Chống lộ qua `Debug` và `Display`:**
   - Kiểu `secrecy::SecretString` không triển khai `fmt::Display` (ngăn chặn việc vô tình đưa biến vào chuỗi nội suy `format!("{secret}")` ở mức biên dịch).
   - Triển khai `fmt::Debug` của `SecretString` luôn xuất ra chuỗi cố định `Secret([REDACTED])`.
2. **Chống lộ qua Serialization (`serde::Serialize`):**
   - `SecretString` mặc định không triển khai `Serialize`, ngăn chặn triệt để nguy cơ vô tình trả cấu trúc cấu hình về qua REST API hoặc xuất ra JSON log.
   - Khi cần xuất bản tóm tắt cấu hình lúc khởi động (`Startup Diagnostic Summary`), hệ thống sử dụng hàm serializer tùy chỉnh `serialize_redacted_secret` luôn xuất giá trị `"[REDACTED]"`.
3. **Tự động Xóa sạch Bộ nhớ RAM (`Memory Zeroization`):**
   - `secrecy::SecretString` tích hợp sẵn trait `zeroize::Zeroize`. Ngay khi biến cấu hình hoặc chuỗi kết nối tạm thời ra khỏi phạm vi (`Drop`), vùng nhớ heap chứa mật khẩu sẽ được ghi đè bằng các byte `0x00` bằng lệnh trình biên dịch không bị tối ưu hóa bỏ qua (compiler fence), bảo vệ chống lại tấn công đọc Core Dump hoặc Memory Scraping.
4. **Kiểm tra Độ rỗng mà Không Làm Lộ Giá trị:**
   - Cung cấp hàm tiện ích `is_secret_empty(secret: &SecretString) -> bool` và validator tùy chỉnh `validate_non_empty_secret` để kiểm tra mật khẩu không rỗng hoặc đạt độ dài entropy tối thiểu mà không bao giờ in giá trị mật khẩu ra thông báo lỗi.

```rust
use secrecy::{ExposeSecret, SecretString};
use validator::ValidationError;

/// Kiểm tra một `SecretString` không được để trống hoặc chỉ chứa khoảng trắng.
pub fn validate_non_empty_secret(secret: &SecretString) -> Result<(), ValidationError> {
    if secret.expose_secret().trim().is_empty() {
        let mut err = ValidationError::new("secret_empty");
        err.message = Some("Secret value must not be empty or whitespace-only".into());
        return Err(err);
    }
    Ok(())
}

/// Kiểm tra khóa ký đối xứng (như JWT HMAC Secret) đạt độ dài tối thiểu (ví dụ >= 32 bytes / 256-bit).
pub fn validate_min_secret_entropy(
    secret: &SecretString,
    min_bytes: usize,
) -> Result<(), ValidationError> {
    let actual_len = secret.expose_secret().len();
    if actual_len < min_bytes {
        let mut err = ValidationError::new("secret_insufficient_entropy");
        err.message = Some(
            format!("Secret length ({actual_len} bytes) is shorter than required minimum of {min_bytes} bytes").into(),
        );
        return Err(err);
    }
    Ok(())
}
```

### 6.2. Ghi Log Tóm tắt Cấu hình An toàn Khi Khởi động (Safe Startup Diagnostic Summary)

Khi một Pod khởi động trên cụm Kubernetes, kỹ sư SRE cần xác nhận nhanh Pod đã nạp đúng cấu hình mong muốn (port, môi trường, kích thước pool, trạng thái TLS) mà không bao giờ để lộ bất kỳ secret nào:

```json
{
  "timestamp": "2026-11-01T16:00:00.012450Z",
  "level": "INFO",
  "message": "Application configuration loaded and validated successfully",
  "target": "omni_common::config",
  "service": {
    "name": "order-service",
    "version": "0.1.0",
    "environment": "prod"
  },
  "config_summary": {
    "http_addr": "0.0.0.0:8080",
    "grpc_addr": "0.0.0.0:50051",
    "database_url": "postgres://omni_order:[REDACTED]@pg-cluster.database.svc:5432/omni_orders?sslmode=require",
    "database_pool": "min=5, max=50, acquire_timeout=5s",
    "redis_url": "redis://:[REDACTED]@redis-master.cache.svc:6379/0",
    "jwt_secret": "[REDACTED]",
    "openapi_swagger_ui_enabled": false
  }
}
```

---

## 7. Fail-Fast Startup Validation & Cross-Field Invariants

### 7.1. Quy trình Kiểm tra Hợp lệ Hai Giai đoạn (Two-Phase Validation Pipeline)

Việc chỉ kiểm tra từng trường độc lập (`#[validate(range(...))]`) là chưa đủ cho một hệ thống phân tán cấp doanh nghiệp. Nhiều sự cố nghiêm trọng trên thực tế xuất phát từ **mâu thuẫn logic giữa các trường cấu hình với nhau** hoặc **cấu hình phát triển bị đẩy nhầm lên môi trường Production**.

Do đó, `AppConfig::validate_all()` thực thi tuần tự 2 pha kiểm tra:

#### Pha 1: Kiểm tra Ràng buộc Cấp độ Trường (`validator::Validate`)

- Kiểm tra dải cổng mạng hợp lệ (`1024..=65535`).
- Kiểm tra định dạng URL hợp lệ cho `otlp_endpoint`, `jwks_url`.
- Kiểm tra tỷ lệ lấy mẫu `sampling_ratio` nằm trong khoảng `0.0..=1.0`.
- Kiểm tra các chuỗi định danh (`service.name`, `database.host`, `i18n.default_locale`) không rỗng.

#### Pha 2: Kiểm tra Bất biến Liên trường & Chốt chặn Bảo mật Production (`Cross-Field & Production Guardrails`)

| Mã quy tắc                      | Điều kiện kiểm tra liên trường (Cross-Field Invariant)                                                                                                    | Lý do kỹ thuật & Hậu quả nếu vi phạm                                                                                                     |
| :------------------------------ | :-------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------- |
| `CFG_DB_POOL_BOUNDS`            | `database.min_connections <= database.max_connections`                                                                                                    | Ngăn chặn lỗi panic của `sqlx::PgPoolOptions` khi số kết nối tối thiểu lớn hơn giới hạn tối đa.                                          |
| `CFG_TIMEOUT_HIERARCHY`         | `database.acquire_timeout_secs <= server.request_timeout_secs`                                                                                            | Nếu thời gian chờ mượn kết nối DB dài hơn tổng timeout của HTTP request, kết nối lấy được sẽ bị vứt bỏ vô ích và gây nghẽn dây chuyền.   |
| `CFG_PROD_SECRET_REQUIRED`      | Khi `database.enabled == true`: `database.password` không được rỗng; Khi `environment == Prod`: không dùng mật khẩu mặc định `"postgres"` / `"password"`. | Ngăn chặn việc triển khai Pod lên Production với mật khẩu trống hoặc mật khẩu mặc định của môi trường Local.                             |
| `CFG_PROD_JWT_ENTROPY`          | Khi `auth.enabled == true`: `auth.jwt_secret` phải có độ dài tối thiểu `32 bytes` (256-bit).                                                              | Đảm bảo độ mạnh mật mã học chống tấn công vét cạn (Brute-force HMAC-SHA256) theo khuyến nghị RFC 7518.                                   |
| `CFG_PROD_SWAGGER_UI_DISABLED`  | Khi `service.environment == Prod`: `observability.swagger_ui_enabled == false`.                                                                           | Tuân thủ nghiêm ngặt đặc tả `api-documentation-architecture.md` (Tuyệt đối không bật Swagger UI HTML trên các Pod backend ở Production). |
| `CFG_PROD_CORS_STRICT`          | Khi `service.environment == Prod`: Không được cấu hình `cors.allow_origins = ["*"]` đồng thời với `allow_credentials = true`.                             | Ngăn chặn lỗ hổng bảo mật Cross-Origin Resource Sharing trên môi trường Production.                                                      |
| `CFG_I18N_DEFAULT_IN_SUPPORTED` | `i18n.supported_locales.contains(&i18n.default_locale)`                                                                                                   | Đảm bảo ngôn ngữ mặc định (`default_locale`) luôn nằm trong danh sách các ngôn ngữ được hệ thống nạp bundle.                             |

### 7.2. Cấu trúc Lỗi `ConfigError` và Báo cáo Chẩn đoán Tường minh

Khi phát hiện vi phạm, `ConfigLoader` thu thập **toàn bộ** lỗi từ cả Pha 1 và Pha 2 vào một danh sách `Vec<ConfigValidationIssue>` thay vì dừng lại ở lỗi đầu tiên:

```rust
use thiserror::Error;

/// Chi tiết một vi phạm ràng buộc cấu hình kèm gợi ý biến môi trường để khắc phục.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationIssue {
    /// Đường dẫn trường bị lỗi (ví dụ: `"database.max_connections"`).
    pub field_path: String,
    /// Mã lỗi chuẩn hóa (ví dụ: `"CFG_DB_POOL_BOUNDS"`).
    pub code: String,
    /// Thông điệp giải thích rõ nguyên nhân lỗi.
    pub message: String,
    /// Tên biến môi trường `OMNI__*` tương ứng để DevOps sửa ngay trên Kubernetes Deployment/ConfigMap.
    pub env_var_hint: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Invalid application environment specification: {0}")]
    InvalidEnvironment(String),

    #[error("Failed to build or deserialize configuration layers: {0}")]
    Deserialization(String),

    #[error("Configuration validation failed with {} error(s):\n{}", .0.len(), format_issues(.0))]
    ValidationFailed(Vec<ConfigValidationIssue>),
}

fn format_issues(issues: &[ConfigValidationIssue]) -> String {
    issues
        .iter()
        .enumerate()
        .map(|(idx, issue)| {
            format!(
                "  {}. [{}] Field '{}' (Env: {}): {}",
                idx + 1,
                issue.code,
                issue.field_path,
                issue.env_var_hint,
                issue.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

Ví dụ thông báo lỗi xuất ra `stderr` khi Pod bị Fail-Fast:

```text
Configuration validation failed with 3 error(s):
  1. [range] Field 'server.port' (Env: OMNI__SERVER__PORT): server.port must be in unprivileged range 1024..=65535
  2. [CFG_DB_POOL_BOUNDS] Field 'database.min_connections' (Env: OMNI__DATABASE__MIN_CONNECTIONS): min_connections (50) cannot be greater than max_connections (20)
  3. [CFG_PROD_SWAGGER_UI_DISABLED] Field 'observability.swagger_ui_enabled' (Env: OMNI__OBSERVABILITY__SWAGGER_UI_ENABLED): Swagger UI HTML must be disabled in 'prod' environment
```

---

## 8. Standard YAML Templates & Kubernetes Deployment Integration

### 8.1. Tập tin Cấu hình Nền tảng `config/base.yaml`

```yaml
# ==============================================================================
# OmniKubernetes Base Configuration (Shared Across All Environments)
# Override per environment via config/{env}/default.yaml or OMNI__* env vars.
# ==============================================================================

service:
  name: "omni-service"
  version: "0.1.0"
  environment: "dev"
  region: "ap-southeast-1"

server:
  host: "0.0.0.0"
  port: 8080
  grpc_port: 50051
  request_timeout_secs: 30
  shutdown_timeout_secs: 15
  max_body_size_bytes: 2097152 # 2 MiB
  trusted_proxies:
    - "10.0.0.0/8"
    - "172.16.0.0/12"
    - "192.168.0.0/16"
  cors:
    allow_origins:
      - "http://localhost:3000"
    allow_credentials: true
    max_age_secs: 3600

logging:
  log_level: "info"
  log_format: "json"
  buffered_lines_limit: 10000
  lossy: true
  otlp_endpoint: null

observability:
  metrics_enabled: true
  metrics_path: "/metrics"
  openapi_enabled: true
  openapi_json_path: "/v3/api-docs/openapi.json"
  swagger_ui_enabled: false
  sampling_ratio: 1.0

database:
  enabled: false
  host: "localhost"
  port: 5432
  username: "omni_user"
  password: "dev_only_password_change_me"
  database_name: "omni_db"
  ssl_mode: "prefer"
  max_connections: 20
  min_connections: 2
  acquire_timeout_secs: 5
  idle_timeout_secs: 300
  max_lifetime_secs: 1800
  slow_query_threshold_ms: 200
  auto_migrate: false

redis:
  enabled: false
  host: "localhost"
  port: 6379
  password: null
  database_index: 0
  pool_size: 16
  connection_timeout_ms: 2000
  command_timeout_ms: 1000
  key_prefix: "omni"
  compression_threshold_bytes: 8192

auth:
  enabled: false
  jwt_algorithm: "HS256"
  jwt_secret: "dev_only_jwt_secret_key_must_be_at_least_32_bytes_long"
  access_token_ttl_secs: 900
  refresh_token_ttl_secs: 604800
  issuer: "urn:omni:auth"
  audience: "urn:omni:api"
  argon2_memory_kib: 19456
  argon2_iterations: 2
  argon2_parallelism: 1

messaging:
  enabled: false
  broker_url: "amqp://guest:guest@localhost:5672/%2f"
  outbox_batch_size: 100
  outbox_poll_interval_ms: 500
  max_retries: 5
  dlq_topic_suffix: ".dlq"

resilience:
  rate_limit_enabled: true
  rate_limit_rps: 100
  rate_limit_burst: 200
  max_concurrent_requests: 1024
  circuit_breaker_error_threshold_pct: 50
  circuit_breaker_reset_timeout_secs: 30

i18n:
  locales_dir: "libs/common/locales"
  default_locale: "vi-VN"
  supported_locales:
    - "vi-VN"
    - "en-US"
    - "ja-JP"
```

### 8.2. Tập tin Cấu hình Môi trường `config/dev/default.yaml` & `config/prod/default.yaml`

#### A. `config/dev/default.yaml` (Tối ưu hóa trải nghiệm lập trình viên Local)

```yaml
service:
  environment: "dev"

logging:
  log_level: "debug"
  log_format: "pretty"

observability:
  swagger_ui_enabled: true
  sampling_ratio: 1.0
```

#### B. `config/prod/default.yaml` (Cấu hình Chịu tải cao & Bảo mật Production)

```yaml
service:
  environment: "prod"

logging:
  log_level: "info"
  log_format: "json"
  buffered_lines_limit: 50000
  lossy: true

observability:
  swagger_ui_enabled: false
  sampling_ratio: 0.1

database:
  ssl_mode: "require"
  max_connections: 50
  min_connections: 10
  acquire_timeout_secs: 3
  auto_migrate: false

i18n:
  locales_dir: "/app/locales"
```

### 8.3. Tích hợp với Kubernetes `ConfigMap`, `Secret` và `Downward API`

Trong môi trường Kubernetes thực tế, cấu hình của một Pod được lắp ghép tự động từ `ConfigMap`, `Secret` (đồng bộ từ Vault qua External Secrets Operator) và `Downward API`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: order-service
  namespace: omni-ecommerce
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: order-service
          image: ghcr.io/nqhxdev/omnikubernetes/order-service:0.1.0
          ports:
            - containerPort: 8080
              name: http
            - containerPort: 50051
              name: grpc
          env:
            # 1. Xác định môi trường và tên dịch vụ
            - name: OMNI__SERVICE__NAME
              value: "order-service"
            - name: OMNI__SERVICE__ENVIRONMENT
              value: "prod"

            # 2. Kubernetes Downward API (Gắn tự động định danh Pod vào Log & OTel Resource)
            - name: K8S_POD_NAME
              valueFrom:
                fieldRef:
                  fieldPath: metadata.name
            - name: K8S_NAMESPACE
              valueFrom:
                fieldRef:
                  fieldPath: metadata.namespace
            - name: K8S_NODE_NAME
              valueFrom:
                fieldRef:
                  fieldPath: spec.nodeName

            # 3. Tham số vận hành từ ConfigMap
            - name: OMNI__DATABASE__ENABLED
              value: "true"
            - name: OMNI__DATABASE__HOST
              valueFrom:
                configMapKeyRef:
                  name: order-service-config
                  key: db_host
            - name: OMNI__LOGGING__OTLP_ENDPOINT
              valueFrom:
                configMapKeyRef:
                  name: omni-observability-config
                  key: otlp_grpc_endpoint

            # 4. Thông tin mật từ Kubernetes Secret (Được bảo vệ bởi SecretString trong Rust)
            - name: OMNI__DATABASE__PASSWORD
              valueFrom:
                secretKeyRef:
                  name: order-service-secrets
                  key: db_password
            - name: OMNI__REDIS__PASSWORD
              valueFrom:
                secretKeyRef:
                  name: order-service-secrets
                  key: redis_password
            - name: OMNI__AUTH__JWT_SECRET
              valueFrom:
                secretKeyRef:
                  name: omni-auth-secrets
                  key: jwt_signing_key
```

---

## 9. Domain Service Extension Pattern & Safe Testing Architecture

### 9.1. Mở rộng Cấu hình Nghiệp vụ cho Từng Microservice (`AppConfig<Custom>`)

Mỗi microservice trong `services/*` có thể định nghĩa thêm các tham số nghiệp vụ riêng mà không cần sửa đổi `libs/common`. Ví dụ tại `services/order`:

```rust
use omni_common::config::AppConfig;
use serde::Deserialize;
use validator::Validate;

/// Cấu hình nghiệp vụ chuyên biệt của Order Service.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct OrderDomainConfig {
    /// Thời gian giữ chỗ đơn hàng chờ thanh toán trước khi tự động hủy (phút).
    #[validate(range(min = 5, max = 1440))]
    pub reservation_ttl_minutes: u32,

    /// Số lượng mặt hàng tối đa được phép đặt trong một đơn hàng.
    #[validate(range(min = 1, max = 100))]
    pub max_items_per_order: usize,
}

impl Default for OrderDomainConfig {
    fn default() -> Self {
        Self {
            reservation_ttl_minutes: 15,
            max_items_per_order: 50,
        }
    }
}

pub fn load_order_config() -> Result<AppConfig<OrderDomainConfig>, omni_common::config::ConfigError> {
    // Tự động nạp 5 tầng + kiểm tra hợp lệ cả cấu hình nền tảng lẫn OrderDomainConfig
    AppConfig::<OrderDomainConfig>::load("order-service")
}
```

Khi đó, DevOps có thể ghi đè trực tiếp trường mở rộng bằng biến môi trường `OMNI__CUSTOM__RESERVATION_TTL_MINUTES=30` hoặc khai báo mục `custom:` trong file YAML.

### 9.2. Kiến trúc Kiểm thử Cô lập Không Dùng `unsafe` (Rust 2024 Edition Safe Testing)

Trong **Rust 2024 Edition**, hàm `std::env::set_var` và `std::env::remove_var` đã được chuyển thành hàm **`unsafe`** do việc thay đổi biến môi trường toàn cục của tiến trình gây ra lỗi Data Race (Undefined Behavior) khi `cargo test` thực thi nhiều luồng kiểm thử song song. Hơn nữa, toàn bộ workspace **OmniKubernetes** thực thi chính sách `unsafe_code = "deny"`.

Để đảm bảo 100% bài kiểm thử chạy song song nhanh, cô lập hoàn toàn và không cần dùng `unsafe`:

- `ConfigLoader` được thiết kế theo mẫu **Builder & Dependency Injection**:
  - Trong vận hành thực tế (`AppConfig::load("order-service")`), `ConfigLoader` đọc từ thư mục `config/` và `std::env::vars()`.
  - Trong kiểm thử tự động (`ConfigLoader::for_test("order-service")`), lập trình viên truyền trực tiếp bản đồ biến môi trường giả lập `with_env_vars(...)` và `with_override(...)` trong bộ nhớ.

```rust
use omni_common::config::{AppConfig, AppEnvironment, ConfigLoader};

#[test]
fn test_hierarchical_env_override_and_secret_redaction() {
    let config: AppConfig = ConfigLoader::new("payment-service")
        .with_environment(AppEnvironment::Staging)
        .with_dotenv_disabled()
        .with_env_vars([
            ("OMNI__SERVER__PORT", "9090"),
            ("OMNI__DATABASE__ENABLED", "true"),
            ("OMNI__DATABASE__MAX_CONNECTIONS", "64"),
            ("OMNI__DATABASE__PASSWORD", "super_secret_prod_db_pass_2026"),
        ])
        .load()
        .expect("Configuration should load and validate cleanly");

    assert_eq!(config.service.name, "payment-service");
    assert_eq!(config.service.environment, AppEnvironment::Staging);
    assert_eq!(config.server.port, 9090);
    assert_eq!(config.database.max_connections, 64);

    // Đảm bảo mật khẩu không bao giờ lộ ra khi format Debug hoặc Redacted URL
    let debug_output = format!("{config:?}");
    assert!(!debug_output.contains("super_secret_prod_db_pass_2026"));
    assert!(config.database.redacted_url().contains("[REDACTED]"));
}
```

---

## 10. Verification & Quality Acceptance Criteria

Để nghiệm thu hoàn tất **Bước 1: Type-Safe Configuration & Env Binding** theo [`RoadMap.md`](file://RoadMap.md), mã nguồn triển khai tại `libs/common` và `config/` phải vượt qua 100% các tiêu chí kiểm định sau:

1. **Unit Test Thứ tự Ưu tiên 5 Tầng (5-Layer Precedence Verification):**
   - Kiểm chứng giá trị từ `Compiled-in Default` bị ghi đè bởi `config/base.yaml`, tiếp tục bị ghi đè bởi `config/{env}/default.yaml`, bị ghi đè bởi biến môi trường `OMNI__*`, và cuối cùng bị ghi đè bởi `with_override()`.
2. **Unit Test Phân tích Biến Môi trường `OMNI__` (Double-Underscore Parsing):**
   - Các trường chứa dấu gạch dưới đơn dạng `snake_case` (như `max_connections`, `shutdown_timeout_secs`, `buffered_lines_limit`) phải được ánh xạ chính xác từ biến môi trường phân tách bằng `__` (`OMNI__DATABASE__MAX_CONNECTIONS`).
   - Kiểm chứng cơ chế fallback sang các biến môi trường chuẩn hệ sinh thái (`RUST_LOG`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `K8S_POD_NAME`, `K8S_NAMESPACE`).
3. **Unit Test Fail-Fast & Cross-Field Validation:**
   - Từ chối khởi động khi `server.port < 1024`.
   - Từ chối khởi động khi `database.min_connections > database.max_connections` (`CFG_DB_POOL_BOUNDS`).
   - Từ chối khởi động khi `database.acquire_timeout_secs > server.request_timeout_secs` (`CFG_TIMEOUT_HIERARCHY`).
   - Từ chối khởi động ở môi trường `Prod` nếu bật `swagger_ui_enabled = true`, nếu dùng mật khẩu mặc định hoặc `jwt_secret` ngắn hơn 32 bytes.
   - Đảm bảo `ConfigError::ValidationFailed` gom đầy đủ tất cả các lỗi vi phạm trong một lần trả về kèm gợi ý biến môi trường `OMNI__*`.
4. **Security Test Zero-Leak `SecretString`:**
   - Kiểm chứng chuỗi xuất ra từ `format!("{config:?}")`, `config.database.redacted_url()`, `config.redis.redacted_url()` và bản tóm tắt cấu hình (`StartupSummary`) tuyệt đối không chứa giá trị plaintext của `database.password`, `redis.password`, `auth.jwt_secret`, `messaging.broker_url`.
5. **Integration Test Cầu nối `omni-logging` & Mở rộng Generic `AppConfig<C>`:**
   - Kiểm chứng `config.to_logging_config()` chuyển đổi chính xác sang `omni_logging::LoggingConfig` và khởi tạo logger thành công.
   - Kiểm chứng khả năng nạp và validate struct mở rộng tùy chỉnh `AppConfig<CustomServiceConfig>`.
6. **Tuân thủ Tuyệt đối Quality Gate:**
   - Vượt qua `cargo fmt --all -- --check`, `cargo hakari generate --diff`, `cargo clippy --all-targets --all-features -- -D warnings` (0 warnings, `unsafe_code = "deny"`), và `cargo test --all`.
