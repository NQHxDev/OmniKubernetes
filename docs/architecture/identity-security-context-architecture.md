# OmniKubernetes Architecture Specification: Identity & Security Context (`omni-auth`)

- **Document Version:** `1.0.0`
- **Status:** Draft for Review / Enterprise Architecture Standard
- **Author:** System Architecture Team
- **Roadmap Step:** Giai đoạn 3 — Bước 5: Nền tảng Nhận dạng & Bảo mật truy cập ([`RoadMap.md`](../../RoadMap.md))
- **Target Platform:** Rust (2024 Edition, **Rust 1.94+**), Tokio Runtime (1.43+), Axum Framework (0.8+), `jsonwebtoken 11.1+`, `argon2 0.6+`, `secrecy 0.10+`, `uuid 1.13+` (v4/v7), Redis OSS 7.2+ / Valkey 8+ (`omni-cache`), PostgreSQL 16+ (`omni-database`), Kubernetes & CNCF Ecosystem (Ingress/Envoy Gateway, NetworkPolicy, SPIFFE/SPIRE-ready).

---

## 1. Executive Summary & Core Principles

Sau khi hoàn thành hai giai đoạn nền tảng:

- **Giai đoạn 1:** Base Response & i18n (`omni-common`, `omni-errors`), Distributed Tracing & Structured Logging (`omni-logging`), Type-Safe Configuration (`omni-common::config`).
- **Giai đoạn 2:** Metrics Telemetry & K8s Probes (`omni-metrics`), Relational Persistence (`omni-database`), Distributed Caching & Concurrency Control (`omni-cache`).

Hệ sinh thái **OmniKubernetes** chính thức bước vào **Giai đoạn 3: Bảo mật & Truyền thông Bất đồng bộ**. Trụ cột đầu tiên của giai đoạn này là **Nền tảng Nhận dạng & Bảo mật Truy cập (Identity & Security Context - `omni-auth`)**.

Trong kiến trúc Enterprise Microservices vận hành trên Kubernetes, sự thất bại phổ biến nhất của các hệ thống bảo mật xuất phát từ hai thái cực sai lầm:

1. **Perimeter-Only Security (Bảo mật đường biên lỏng lẻo):** Phụ thuộc 100% vào API Gateway. Các microservice bên trong mạng pod-to-pod tin cậy tuyệt đối mọi header gửi đến (`X-User-Id`), dẫn tới nguy cơ **Header Spoofing** và **Lateral Movement** khi một pod biên bị chiếm quyền điều khiển.
2. **Heavyweight Over-Verification (Xác thực phân tán cồng kềnh):** Mỗi microservice tự ý truy vấn cơ sở dữ liệu Auth trung tâm để kiểm tra quyền cho từng HTTP request, gây nghẽn cổ chai DB (Database Bottleneck), tăng vọt độ trễ P99 và phá vỡ tính tự chủ của microservices.

Crate `libs/auth` (`omni-auth`) thiết lập một nền tảng **Zero-Trust Defense-in-Depth**, kết hợp tính linh hoạt của **Stateless JWT Claims** với khả năng thu hồi tức thời (**Real-time Distributed Revocation**) trên Redis, đồng thời cung cấp các Axum Extractors & Guards kiểu tĩnh (Type-Safe) với chi phí cấp phát bộ nhớ xấp xỉ bằng không (Near-Zero Allocation).

Hệ thống xác lập **8 nguyên lý kiến trúc cốt lõi (Core Principles)**:

```mermaid
flowchart TD
    subgraph Principles["8 NGUYÊN LÝ BẢO MẬT CỐT LÕI (OMNI-AUTH)"]
        P1["1. Zero-Trust Internal Mesh<br/>(Không tin tưởng mù quáng header nội bộ)"]
        P2["2. Zero-Leak Credential & Constant-Time<br/>(Bảo vệ bí mật, chống Timing Attack)"]
        P3["3. Stateless by Default, Statefully Revocable<br/>(JWT phi trạng thái + JTI Redis Blacklist)"]
        P4["4. Strict Type-Safe Claims Specification<br/>(Claims định danh chặt chẽ với UUIDv7)"]
        P5["5. Declarative Axum Guards & Extractors<br/>(Extractors & Guards cắm-rút tinh gọn)"]
        P6["6. End-to-End Context Enrichment<br/>(Trace, Span, Log & DB Session Auditing)"]
        P7["7. Multi-Algorithm Agility<br/>(HS256/384/512, RS256, EdDSA, ES256/384/512, JWKS)"]
        P8["8. Multi-Tenancy Isolation by Construction<br/>(Ranh giới Tenant cách ly từ tầng Extractor)"]
    end
```

1. **Zero-Trust Internal Mesh (Không tin cậy mù quáng mạng nội bộ):**
   - Không chấp nhận các header định danh thô (`X-User-Id`, `X-Tenant-Id`) từ các kết nối không có chữ ký số xác thực.
   - Khi Gateway chuyển tiếp traffic đã xác thực vào downstream microservices, Gateway phải ký gói ngữ cảnh định danh bằng **HMAC-SHA256 Token tạm thời (Signed Internal Context)** hoặc chuyển tiếp nguyên vẹn Bearer JWT token.
2. **Zero-Leak Credentials & Constant-Time Security:**
   - Mật khẩu thô, private key và JWT secret bắt buộc bọc trong `SecretString` (`secrecy`), tự động zeroize bộ nhớ khi bị hủy.
   - Mọi thao tác so sánh chữ ký HMAC, so khớp mật khẩu hoặc kiểm tra token đều sử dụng thuật toán đẳng thời (**Constant-Time Execution**) để triệt tiêu hoàn toàn tấn công kênh bên theo thời gian (Timing Side-Channel Attacks).
   - Tuyệt đối không ghi token, password hoặc refresh token vào log (tuân thủ Nghị định 13/2023/NĐ-CP và GDPR).
3. **Stateless by Default, Statefully Revocable (Stateless JWT + Real-time Invalidation):**
   - Access Token là JSON Web Token tự thân (Self-Contained), mang đầy đủ claims (`sub`, `roles`, `permissions`, `tenant_id`) để downstream service tự giải mã và cấp quyền trong bộ nhớ cục bộ mà không cần gọi network I/O.
   - Khi cần đăng xuất khẩn cấp, khóa tài khoản hoặc thu hồi phiên, hệ thống sử dụng **JTI Blacklist** lưu trên `omni-cache` với TTL chính xác bằng thời gian sống còn lại của Access Token (chống phình bộ nhớ Redis).
4. **Strict Type-Safe Claims Specification:**
   - Không sử dụng untyped JSON (`serde_json::Value`) để chứa thông tin người dùng.
   - Cấu trúc `AuthUserClaims` định nghĩa chặt chẽ với các định danh UUIDv7 (`sub`, `session_id`, `tenant_id`), phân định rạch ròi giữa Vai trò (`Role`) và Quyền hạn vi mô (`Permission`).
5. **Declarative Axum Guards & Extractors:**
   - Cung cấp các Axum 0.8 Extractors: `AuthenticatedUser`, `Option<AuthenticatedUser>`, `CurrentTenant`.
   - Cung cấp các Guard khai báo: `RequireRole`, `RequireAnyRole`, `RequirePermission`, `RequireAllPermissions` dưới dạng Middleware Layers hoặc Guard combinators.
   - Loại bỏ hoàn toàn boilerplate code kiểm tra quyền thủ công bên trong handler nghiệp vụ.
6. **End-to-End Context Enrichment (Auditing & Traceability):**
   - Danh tính người dùng (`user_id`, `tenant_id`) sau khi xác thực thành công sẽ tự động được tiêm vào:
     - `RequestContext` của `omni-common`.
     - `tracing::Span` của OTel (`user.id`, `tenant.id`).
     - PostgreSQL `SessionContext` (`omni.actor_id`, `omni.tenant_id`) thông qua `build_begin_statement` của `omni-database` để phục vụ Entity Auditing (`AuditColumns: created_by, updated_by`) và Row-Level Security (RLS).
7. **Multi-Algorithm Agility (Symmetric, Asymmetric & JWKS):**
   - Hỗ trợ linh hoạt thuật toán đối xứng HMAC (HS256, HS384, HS512) cho môi trường nội bộ/dev và các thuật toán bất đối xứng hiện đại: **RS256** (RSA-SHA256), **EdDSA** (Ed25519), **ES256/ES384/ES512** (ECDSA với NIST P-256/P-384/P-521) cho môi trường Production quy mô lớn và tương thích HSM/KMS.
   - Sẵn sàng tích hợp cơ chế nạp khóa động qua chuẩn **JWKS (JSON Web Key Set)** endpoint khi làm việc với Identity Providers bên ngoài (Keycloak, Auth0, Okta, Zitadel).
8. **Multi-Tenancy Isolation by Construction:**
   - Tenant context là một công dân hạng nhất (`TenantId`).
   - Mọi truy vấn và kiểm tra quyền hạn đều kiểm tra chéo ngữ cảnh Tenant của Request so với dữ liệu thực thể đích, ngăn chặn triệt để lỗ hổng BOLA / IDOR (Broken Object Level Authorization - OWASP API1:2023).

---

## 2. Scope of Applicability & Workspace Dependency Architecture

### 2.1. Nằm trong phạm vi (In-Scope)

| #   | Năng lực / Module                                                  | Mô tả chi tiết                                                                                                                                                                                                   |
| :-- | :----------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Password Cryptography Engine (`crypto::password`)**              | Băm và so khớp mật khẩu chuẩn OWASP bằng `Argon2id` (memory-hard, random salt 128-bit, constant-time verification). Hỗ trợ chạy non-blocking qua `tokio::task::spawn_blocking`.                                  |
| 2   | **Token Engine & Claims Specification (`token::*`)**               | Định nghĩa `AuthUserClaims`, `TokenType` (Access, Refresh, ApiKey), `TokenPair`. Ký và thẩm định JWT qua `jsonwebtoken` với đa thuật toán (HS256..512, RS256, EdDSA, ES256/384/512). Luân chuyển khóa qua `kid`. |
| 3   | **Token Revocation & Session Management (`revocation::*`)**        | Cơ chế JTI Blacklist tích hợp `omni-cache` (`TypedCache`). Cơ chế Refresh Token Rotation (RTR) với **Token Family Detection** (phát hiện tái sử dụng refresh token bị đánh cắp để vô hiệu hóa toàn bộ family).   |
| 4   | **Axum Extractors (`extractor::*`)**                               | Axum 0.8 `FromRequestParts` extractors: `AuthenticatedUser` (bắt buộc), `OptionalUser` (tùy chọn), `CurrentTenant` (bảo vệ đa tenant), `BearerToken` (trích xuất token thô).                                     |
| 5   | **Declarative Authorization Guards (`guard::*`)**                  | Type-safe guards: `RequireRole(Role)`, `RequireAnyRole(&[Role])`, `RequirePermission(&str)`, `RequireAllPermissions(&[&str])`.                                                                                   |
| 6   | **Gateway Signed Context Protocol (`middleware::signed_context`)** | Middleware thẩm định chữ ký HMAC giữa API Gateway và Internal Microservices (`X-Omni-Signature`, `X-Omni-Timestamp`, `X-User-Id`, `X-Tenant-Id`) chống giả mạo header trong cụm K8s.                             |
| 7   | **Context Bridging & Propagation (`bridge::*`)**                   | Cầu nối tự động từ `AuthenticatedUser` sang `omni_common::RequestContext`, `tracing::Span` attributes, và `omni_database::session::SessionContext` (`Actor::User(Uuid)`).                                        |
| 8   | **RFC 9457 Problem Details Integration (`error::*`)**              | Định nghĩa `AuthError` phân loại sâu (`TokenExpired`, `TokenRevoked`, `InsufficientRole`, `TenantMismatch`) và tự động chuyển đổi sang `omni_errors::AppError` (`401 Unauthorized`, `403 Forbidden`).            |
| 9   | **Security Observability & Audit Telemetry (`instrument::*`)**     | Bộ chỉ số Prometheus: `auth_tokens_validated_total`, `auth_failures_total`, `auth_crypto_duration_seconds`. Ghi nhận Audit Spans cho các sự kiện nhạy cảm (đăng nhập, đổi quyền, thu hồi token).                 |
| 10  | **Testing Harness & Test Utilities (`test_utils::*`)**             | Cung cấp builder tạo JWT giả lập, `MockAuthLayer`, test claims fixture phục vụ kiểm thử đơn vị và kiểm thử tích hợp không cần dựng Identity Provider thật.                                                       |

### 2.2. Ngoài phạm vi (Out-of-Scope)

- **Domain Identity Service (`services/auth`, `services/user`):** Các logic nghiệp vụ như bảng cơ sở dữ liệu `users`, API đăng ký tài khoản, xác thực email, SMS OTP, Social Login (Google, GitHub OAuth2 Redirect) thuộc về microservice nghiệp vụ `services/auth`. `omni-auth` chỉ cung cấp **nền tảng thư viện hạ tầng dùng chung**.
- **External OAuth2 / OIDC Server Engine:** Không biến `omni-auth` thành một OAuth2 Authorization Server hoàn chỉnh (như Hydra hay Keycloak); chỉ cung cấp token generation/validation engine chuẩn RFC 7519.
- **Complex Policy Engine (Open Policy Agent / Cedar / Rego):** Dành cho v1.2 nếu hệ thống yêu cầu thuộc tính ABAC phân tán siêu phức tạp. v1.0 tập trung giải quyết xuất sắc mô hình **RBAC + Fine-grained Permissions + Tenant Scoping**.

### 2.3. Sơ đồ Phụ thuộc Crate Không Chu trình (Acyclic Crate Dependency Graph)

```mermaid
flowchart TD
    subgraph Services["Domain Microservices (services/*)"]
        AUTH_SVC["services/auth<br/>(User entity, login, refresh, register)"]
        ORDER_SVC["services/order<br/>(Requires: AuthenticatedUser, RequirePermission)"]
        GATEWAY["services/gateway<br/>(Signs context or forwards Bearer JWT)"]
    end

    subgraph AuthCrate["libs/auth (omni-auth)"]
        CRYPTO["crypto (Argon2id, PasswordHasher)"]
        TOKEN["token (JWT Engine, Claims, TokenPair)"]
        REVOKE["revocation (JTI Blacklist, Token Family)"]
        EXTRACT["extractor (AuthenticatedUser, Tenant)"]
        GUARD["guard (RequireRole, RequirePermission)"]
        BRIDGE["bridge (RequestContext & DB Session Bridge)"]
        ERR["error (AuthError -> AppError)"]
        INST["instrument (Prometheus Metrics & OTel Audit)"]
    end

    subgraph Foundation["Shared Foundation (libs/*)"]
        COMMON["omni-common<br/>(AuthConfig, RequestContext, SecretString)"]
        ERRORS["omni-errors<br/>(AppError, ApiErrorResponse RFC 9457)"]
        METRICS["omni-metrics<br/>(Prometheus metrics facade, HealthIndicator)"]
        CACHE["omni-cache<br/>(TypedCache, RedisConnection, DistributedLock)"]
        DB["omni-database<br/>(SessionContext, Actor::User, AuditColumns)"]
        LOGGING["omni-logging<br/>(PII Sanitizer, OTLP Tracing)"]
    end

    AUTH_SVC --> AuthCrate
    ORDER_SVC --> AuthCrate
    GATEWAY --> AuthCrate

    AuthCrate --> COMMON
    AuthCrate --> ERRORS
    AuthCrate --> METRICS
    AuthCrate --> LOGGING
    AuthCrate -->|"feature: revocation (default)"| CACHE
    AuthCrate -->|"feature: db-session (optional)"| DB
```

> [!IMPORTANT]
> `omni-auth` phụ thuộc vào `omni-common`, `omni-errors`, `omni-logging` và `omni-metrics`. Tính năng thu hồi JTI phụ thuộc vào `omni-cache` thông qua feature `revocation` (bật mặc định). Tính năng cầu nối sang PostgreSQL `SessionContext` nằm sau feature `db-session`. Không có phụ thuộc vòng (circular dependency) nào tồn tại trong cây workspace.

---

## 3. Standards & Compliance References

| Tiêu chuẩn / Tài liệu                | Cơ quan ban hành | Nội dung áp dụng trong `omni-auth`                                                                                                                                     |
| :----------------------------------- | :--------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **RFC 7519**                         | IETF             | Định dạng JSON Web Token (JWT), xử lý các registered claims (`sub`, `iss`, `aud`, `exp`, `nbf`, `iat`, `jti`).                                                         |
| **RFC 7515 & RFC 7518**              | IETF             | JSON Web Signature (JWS) & Cryptographic Algorithms (JWA): HS256, HS384, HS512, RS256, EdDSA (Ed25519), ES256 (ECDSA P-256), ES384 (ECDSA P-384), ES512 (ECDSA P-521). |
| **RFC 7517**                         | IETF             | JSON Web Key (JWK) & JWKS specification cho phân phối public key và key rotation qua trường `kid`.                                                                     |
| **RFC 6749 & RFC 6750**              | IETF             | OAuth 2.0 Authorization Framework & Bearer Token Usage qua HTTP `Authorization: Bearer <token>`.                                                                       |
| **RFC 9106**                         | IETF             | Argon2 Password Hashing Function: áp dụng biến thể **Argon2id** với tham số an toàn chống GPU/FPGA brute-force.                                                        |
| **RFC 9457**                         | IETF             | Problem Details for HTTP APIs: trả về lỗi xác thực/phân quyền chuẩn `application/problem+json` qua `omni-errors`.                                                      |
| **RFC 9562**                         | IETF             | UUIDv7 time-ordered cho `user_id`, `tenant_id`, `session_id`, `jti` giúp tối ưu hóa index B-Tree và phân tích audit log.                                               |
| **OWASP ASVS v5.0**                  | OWASP            | Application Security Verification Standard: Mục V2 (Authentication), V3 (Session Management), V4 (Access Control).                                                     |
| **OWASP API Security Top 10 (2023)** | OWASP            | Ngăn chặn API1:2023 (BOLA), API2:2023 (Broken Authentication), API5:2023 (Broken Function Level Authorization).                                                        |
| **NIST SP 800-63B**                  | NIST             | Digital Identity Guidelines: Quản lý vòng đời Secret, chống Replay attacks, và quy định độ phức tạp khóa.                                                              |
| **Nghị định 13/2023/NĐ-CP & GDPR**   | VN Gov / EU      | Bảo vệ dữ liệu cá nhân (PII): bọc mọi credentials trong `SecretString`, mask authorization headers trong logs, zero-trace passwords.                                   |

---

## 4. Current State & Gap Analysis

### 4.1. Những gì đã có sẵn trong Workspace

1. **`omni-common::config::auth::AuthConfig`:** Đã định nghĩa sẵn các trường cấu hình cơ bản (`enabled`, `jwt_algorithm`, `jwt_secret`, `access_token_ttl_secs`, `refresh_token_ttl_secs`, `issuer`, `audience`, `argon2_*`) kèm fail-fast guardrail `validate_auth_guardrails`.
2. **`omni-common::middleware::RequestContext`:** Đã dự phòng sẵn hai trường `pub user_id: Option<Cow<'static, str>>` và `pub tenant_id: Option<Cow<'static, str>>`.
3. **`omni-database::session::SessionContext` & `Actor`:** Đã triển khai sẵn `Actor::User(Uuid)`, `Actor::Service(Uuid)` và hàm `build_begin_statement` tự động tiêm `SET LOCAL omni.actor_id = '...'` và `SET LOCAL omni.tenant_id = '...'`.
4. **`omni-database::audit::AuditColumns`:** Đã hỗ trợ sẵn các trường `created_by: Option<Uuid>` và `updated_by: Option<Uuid>` khớp chuẩn với định danh UUID của `Actor`.
5. **`omni-cache::typed::TypedCache` & `KeySpace`:** Đã có nền tảng cache phân tán với serialization nhị phân/JSON, hash tag `{...}` chuẩn Redis Cluster, hỗ trợ TTL tuyệt đối để xây dựng JTI Blacklist.
6. **`omni-logging::sanitizer`:** Đã có bộ lọc PII tự động che giấu `Bearer eyJ...[REDACTED]` và các trường `token`, `password`, `secret`.
7. **`omni-errors::AppError` & `ApiErrorResponse`:** Đã có hai biến thể `AppError::Unauthorized` (401) và `AppError::Forbidden` (403) hỗ trợ i18n localization qua Mozilla Fluent.
8. **Workspace Dependencies:** `Cargo.toml` gốc đã khai báo sẵn `jsonwebtoken = "11.1.0"`, `argon2 = "0.6"`, `secrecy = "0.10"`, `sha2 = "0.11"`, `hmac = "0.13"`, `hex = "0.4"`, `rand = "0.9"`.

### 4.2. Các khoảng trống kỹ thuật (Gaps) cần hoàn thiện trong `omni-auth`

### 4.2.1. So sánh các thuật toán Asymmetric JWT Signing

| Thuật toán | Đường cong / Key size  | Độ dài chữ ký | Hiệu năng           | Use Case                           | Tương thích HSM/KMS               |
| :--------- | :--------------------- | :------------ | :------------------ | :--------------------------------- | :-------------------------------- |
| **RS256**  | RSA 2048-bit           | 256 bytes     | Chậm (verify nhanh) | Legacy systems, wide compatibility | Excellent (PKCS#1, PKCS#8)        |
| **EdDSA**  | Ed25519 (Curve25519)   | 64 bytes      | Rất nhanh           | Modern systems, high performance   | Limited (HSM phải hỗ trợ Ed25519) |
| **ES256**  | NIST P-256 (secp256r1) | 64 bytes      | Nhanh               | Enterprise HSM/KMS, FIPS 140-2     | Excellent (NIST standard)         |
| **ES384**  | NIST P-384 (secp384r1) | 96 bytes      | Trung bình          | High security requirements         | Excellent                         |
| **ES512**  | NIST P-521 (secp521r1) | 132 bytes     | Trung bình-chậm     | Maximum security (Suite B)         | Excellent                         |

**Khuyến nghị lựa chọn:**

- **EdDSA (Ed25519):** Ưu tiên cho hệ thống microservices thuần Kubernetes không cần tích hợp HSM bên ngoài. Hiệu năng tối ưu, key size nhỏ (32 bytes), chống side-channel attacks tốt nhất.
- **ES256 (ECDSA P-256):** Lựa chọn tốt nhất cho **Enterprise Production** cần tích hợp với AWS KMS, Google Cloud KMS, Azure Key Vault, hoặc HSM hardware tuân thủ FIPS 140-2/3. Cân bằng giữa hiệu năng và khả năng tương thích.
- **ES384/ES512:** Dành cho các hệ thống yêu cầu bảo mật cực cao (tài chính, quốc phòng, y tế) hoặc tuân thủ Suite B cryptography (NSA).
- **RS256:** Chỉ dùng khi cần tương thích ngược với hệ thống cũ hoặc external IdP chỉ hỗ trợ RSA.

### 4.2.2. Tạo ECDSA Key Pairs

#### ECDSA P-256 (ES256) - Khuyến nghị cho Production:

```bash
# Generate EC private key (P-256 / prime256v1 / secp256r1)
openssl ecparam -genkey -name prime256v1 -noout -out ec_private_p256.pem

# Extract public key
openssl ec -in ec_private_p256.pem -pubout -out ec_public_p256.pem

# Verify key details
openssl ec -in ec_private_p256.pem -text -noout
```

#### ECDSA P-384 (ES384) - High Security:

```bash
# Generate EC private key (P-384 / secp384r1)
openssl ecparam -genkey -name secp384r1 -noout -out ec_private_p384.pem

# Extract public key
openssl ec -in ec_private_p384.pem -pubout -out ec_public_p384.pem
```

#### ECDSA P-521 (ES512) - Maximum Security:

```bash
# Generate EC private key (P-521 / secp521r1)
openssl ecparam -genkey -name secp521r1 -noout -out ec_private_p521.pem

# Extract public key
openssl ec -in ec_private_p521.pem -pubout -out ec_public_p521.pem
```

### 4.2.3. Cấu hình ECDSA trong Production

```yaml
# config/production.yaml
auth:
  enabled: true
  jwt_algorithm: "ES256" # Khuyến nghị cho production với HSM/KMS
  jwt_key_id: "omni-ec-2026-v1"

  # Load from Kubernetes Secret hoặc HashiCorp Vault
  private_key_pem: |
    -----BEGIN EC PRIVATE KEY-----
    MHcCAQEEIIGlJwzYbJZ8... (từ k8s secret)
    -----END EC PRIVATE KEY-----

  public_key_pem: |
    -----BEGIN PUBLIC KEY-----
    MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE... (public, có thể phân phối)
    -----END PUBLIC KEY-----

  access_token_ttl_secs: 900 # 15 phút
  refresh_token_ttl_secs: 2592000 # 30 ngày
  issuer: "https://api.omni.internal"
  audience: ["omni-services"]
```

### 4.2.4. Tích hợp với Cloud KMS/HSM

#### AWS KMS (ECDSA_SHA_256):

```rust
// Tích hợp với AWS KMS để ký JWT không lưu private key trên disk
use aws_sdk_kms::{Client as KmsClient, types::SigningAlgorithmSpec};

pub struct AwsKmsTokenSigner {
    kms_client: KmsClient,
    key_id: String,
}

impl AwsKmsTokenSigner {
    pub async fn sign_jwt(&self, message: &[u8]) -> Result<Vec<u8>, AuthError> {
        let resp = self.kms_client
            .sign()
            .key_id(&self.key_id)
            .message(aws_sdk_kms::primitives::Blob::new(message))
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .send()
            .await
            .map_err(|e| AuthError::KmsError(e.to_string()))?;

        Ok(resp.signature.unwrap().into_inner())
    }
}
```

#### Google Cloud KMS (EC_SIGN_P256_SHA256):

```rust
// Tích hợp với GCP Cloud KMS
use google_cloudkms1::api::{AsymmetricSignRequest, Digest};

pub struct GcpKmsTokenSigner {
    hub: google_cloudkms1::CloudKMS,
    key_name: String,
}

impl GcpKmsTokenSigner {
    pub async fn sign_jwt(&self, digest_sha256: &[u8]) -> Result<Vec<u8>, AuthError> {
        let req = AsymmetricSignRequest {
            digest: Some(Digest {
                sha256: Some(base64::encode(digest_sha256)),
                ..Default::default()
            }),
            ..Default::default()
        };

        let (_response, sign_response) = self.hub
            .projects()
            .locations_key_rings_crypto_keys_crypto_key_versions_asymmetric_sign(
                req,
                &self.key_name
            )
            .doit()
            .await
            .map_err(|e| AuthError::KmsError(e.to_string()))?;

        let signature = base64::decode(sign_response.signature.unwrap())
            .map_err(|e| AuthError::KmsError(e.to_string()))?;

        Ok(signature)
    }
}
```

| #      | Khoảng trống phát hiện                                                                                                                                                                   | Mức độ           | Giải pháp trong thiết kế `omni-auth`                                                                                                           |
| :----- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------- | :--------------------------------------------------------------------------------------------------------------------------------------------- |
| **G1** | `AuthConfig` hiện tại chỉ hỗ trợ một chuỗi `jwt_secret` HMAC, thiếu cấu hình Asymmetric Key Pair (Private PEM / Public PEM), cấu hình Key ID (`kid`), và cấu hình URL cho JWKS endpoint. | **Cao**          | Mở rộng `AuthConfig` theo mô hình đa chế độ (`Symmetric`, `AsymmetricPem`, `JwksRemote`) đảm bảo tương thích ngược 100% (§5.1).                |
| **G2** | Chưa có struct mô hình hóa danh tính chuẩn xác thực: `AuthUserClaims`, `Role`, `Permission`, `AuthenticatedUser`.                                                                        | **Nghiêm trọng** | Xây dựng bộ kiểu dữ liệu bất biến, type-safe, sử dụng `uuid::Uuid` (UUIDv7) và `SmallVec` / `Cow` tối ưu bộ nhớ (§6).                          |
| **G3** | Crate `libs/auth` hiện là thư viện rỗng (`//! OmniKubernetes auth library.`). Chưa có code băm mật khẩu, mã hóa token, extractors và guards.                                             | **Nghiêm trọng** | Triển khai toàn diện crate `omni-auth` theo kiến trúc module hóa tại §14.                                                                      |
| **G4** | Nguy cơ giả mạo Header nội bộ: Chưa có cơ chế bảo vệ khi Gateway chuyển tiếp header định danh (`X-User-Id`, `X-Tenant-Id`) sang các downstream pod.                                      | **Cao**          | Triển khai giao thức **Signed Gateway Context** với HMAC-SHA256 signature và timestamp freshness check (§8.2).                                 |
| **G5** | Chưa có cơ chế thu hồi Token thời gian thực (Token Invalidation): Khi người dùng đổi mật khẩu hoặc đăng xuất, JWT cũ vẫn có hiệu lực cho đến khi hết `exp`.                              | **Cao**          | Thiết lập mô hình **Tiered Revocation**: JTI Blacklist trên `omni-cache` kết hợp **Token Family Rotation** cho Refresh Token (§9).             |
| **G6** | `AppError::Unauthorized` và `Forbidden` hiện tại chưa cho phép đính kèm mã lỗi chi tiết (`code`) như `ERR_TOKEN_EXPIRED`, `ERR_INSUFFICIENT_PERMISSIONS`, `ERR_TENANT_MISMATCH`.         | **Trung bình**   | Bổ sung `AuthError` trong `omni-auth` với adapter chuyển đổi sang RFC 9457 `ApiErrorResponse` mang đầy đủ `code` và localization params (§11). |
| **G7** | Chưa có bộ chỉ số Prometheus chuyên biệt cho Auth Telemetry (tần suất validate token, số lần từ chối quyền, thời gian băm Argon2).                                                       | **Trung bình**   | Đăng ký các metrics chuẩn: `auth_tokens_validated_total`, `auth_failures_total`, `auth_crypto_duration_seconds` (§12).                         |

---

## 5. Configuration Model (`AuthConfig` Extended)

### 5.1. Mô hình Cấu hình Đa chế độ (Backward-Compatible)

Cấu hình `auth` được nạp qua hệ thống phân cấp của `omni-common` (`config/base.yaml` -> `config/{env}/default.yaml` -> Biến môi trường `OMNI__AUTH__*`).

```yaml
# config/base.yaml — section auth
auth:
  enabled: true
  mode: "symmetric" # symmetric | asymmetric_pem | jwks_remote

  # --- Cấu hình Thuật toán & Bí mật ký Token ---
  jwt_algorithm: "HS256" # HS256, HS384, HS512, RS256, EdDSA, ES256, ES384, ES512
  jwt_secret: "dev_only_jwt_secret_key_must_be_at_least_32_bytes_long" # Dùng cho mode symmetric
  jwt_key_id: "omni-key-2026-v1" # kid header trong JWT header

  # Dùng khi mode = asymmetric_pem
  asymmetric:
    private_key_pem: null # SecretString (PKCS#8 PEM) dành cho Auth Issuer Service
    public_key_pem: null # String (SPKI PEM) dành cho Resource Services thẩm định

  # Dùng khi mode = jwks_remote (Tích hợp Keycloak/Auth0/Zitadel)
  jwks:
    url: null # "https://auth.omni.internal/.well-known/jwks.json"
    refresh_interval_secs: 3600
    cache_ttl_secs: 86400

  # --- Thời hạn sống của Tokens ---
  access_token_ttl_secs: 900 # 15 phút (khuyến nghị cho Stateless Access Token)
  refresh_token_ttl_secs: 604800 # 7 ngày (Refresh Token có rotation)
  clock_skew_leeway_secs: 30 # Khoảng dung sai lệch đồng hồ giữa các node K8s

  # --- Định danh Issuer & Audience (RFC 7519) ---
  issuer: "urn:omni:auth"
  audience: "urn:omni:api"

  # --- Bảo mật Giao tiếp Nội bộ (Gateway-to-Service Signed Context) ---
  internal_gateway_secret: "dev_internal_gateway_hmac_signing_key_32_bytes_minimum"
  signature_max_age_secs: 30 # Chống Replay Attack trên mạng nội bộ

  # --- Thuật toán băm mật khẩu Argon2id (OWASP Profile) ---
  argon2:
    memory_kib: 19456 # 19 MiB RAM
    iterations: 2 # 2 passes
    parallelism: 1 # 1 lane (phù hợp với K8s container CPU limits)
    salt_length_bytes: 16 # 128 bits cryptographic salt
```

### 5.2. Rust Struct Representation

```rust
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Chế độ vận hành khóa ký & thẩm định JWT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthKeyMode {
    /// Sử dụng khóa bí mật đối xứng HMAC (phù hợp dev, staging và microservices nội bộ).
    #[default]
    Symmetric,
    /// Sử dụng cặp khóa bất đối xứng RSA/Ed25519 qua file hoặc chuỗi PEM.
    AsymmetricPem,
    /// Thẩm định token qua endpoint JWKS từ xa của Identity Provider.
    JwksRemote,
}

/// Thuật toán ký JWT hỗ trợ chuẩn hóa trong hệ thống.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SupportedAlgorithm {
    #[default]
    HS256,
    HS384,
    HS512,
    RS256,
    EdDSA,
    ES256,
    ES384,
    ES512,
}

/// Cấu hình chi tiết phân hệ Auth mở rộng.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct AuthConfig {
    pub enabled: bool,
    pub mode: AuthKeyMode,
    pub jwt_algorithm: SupportedAlgorithm,

    #[serde(skip_serializing)]
    pub jwt_secret: SecretString,
    pub jwt_key_id: String,

    pub asymmetric: AsymmetricConfig,
    pub jwks: JwksConfig,

    #[validate(range(min = 60, max = 86400))]
    pub access_token_ttl_secs: u64,

    #[validate(range(min = 3600, max = 2_592_000))]
    pub refresh_token_ttl_secs: u64,

    #[validate(range(min = 0, max = 120))]
    pub clock_skew_leeway_secs: u64,

    #[validate(length(min = 1))]
    pub issuer: String,

    #[validate(length(min = 1))]
    pub audience: String,

    #[serde(skip_serializing)]
    pub internal_gateway_secret: SecretString,

    #[validate(range(min = 5, max = 300))]
    pub signature_max_age_secs: u64,

    pub argon2: Argon2Config,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AsymmetricConfig {
    #[serde(skip_serializing)]
    pub private_key_pem: Option<SecretString>,
    pub public_key_pem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JwksConfig {
    pub url: Option<String>,
    pub refresh_interval_secs: u64,
    pub cache_ttl_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Argon2Config {
    #[validate(range(min = 1024, max = 1_048_576))]
    pub memory_kib: u32,
    #[validate(range(min = 1, max = 10))]
    pub iterations: u32,
    #[validate(range(min = 1, max = 16))]
    pub parallelism: u32,
    #[validate(range(min = 16, max = 64))]
    pub salt_length_bytes: usize,
}
```

### 5.3. Fail-Fast Startup Guardrails (`validate_auth_guardrails`)

Hệ thống kích hoạt chốt chặn an toàn khi khởi chạy (áp dụng nguyên lý Fail-Fast):

| Mã chốt chặn                       | Điều kiện vi phạm                                                                                               | Hành vi ngăn chặn                                                                    |
| :--------------------------------- | :-------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------- |
| `CFG_AUTH_SECRET_TOO_SHORT`        | `mode == Symmetric` và độ dài `jwt_secret` < 32 bytes (256 bits).                                               | Dừng tiến trình ngay lập tức với mã lỗi cấu hình nghiêm trọng.                       |
| `CFG_AUTH_DEV_SECRET_IN_PROD`      | Môi trường `production` nhưng `jwt_secret` hoặc `internal_gateway_secret` chứa chuỗi `"dev_"` hoặc `"default"`. | Từ chối khởi chạy container trên Kubernetes để bảo vệ dữ liệu sản xuất.              |
| `CFG_AUTH_MISSING_ASYMMETRIC_KEYS` | `mode == AsymmetricPem` nhưng thiếu cả `private_key_pem` lẫn `public_key_pem`.                                  | Dừng tiến trình và báo lỗi rõ ràng.                                                  |
| `CFG_AUTH_INVALID_TTL_RATIO`       | `refresh_token_ttl_secs` <= `access_token_ttl_secs`.                                                            | Dừng tiến trình: Refresh Token bắt buộc phải có thời gian sống dài hơn Access Token. |
| `CFG_AUTH_ARGON2_TOO_WEAK`         | `argon2.memory_kib` < 19456 (19 MiB) hoặc `iterations` < 2 trong môi trường Staging/Production.                 | Cảnh báo hoặc từ chối khởi chạy để tuân thủ khuyến nghị OWASP Password Storage.      |

---

## 6. Cryptographic Core: Password Hashing Standard (Argon2id)

### 6.1. Tại sao chọn Argon2id?

Theo khuyến nghị chính thức của **OWASP** và tiêu chuẩn **RFC 9106**, `Argon2id` là biến thể kết hợp ưu điểm của `Argon2d` (chống tấn công phá khóa bằng GPU/ASIC nhờ tính năng memory-hard) và `Argon2i` (chống tấn công kênh bên Side-channel/Cache-timing nhờ truy cập bộ nhớ độc lập với dữ liệu mật khẩu).

Các thuật toán cũ như `MD5`, `SHA-1`, `SHA-256 thuần` tuyệt đối bị cấm sử dụng cho lưu trữ mật khẩu trong toàn bộ hệ sinh thái OmniKubernetes. `bcrypt` được chấp nhận cho mục đích di trú tương thích ngược, nhưng mọi mật khẩu mới bắt buộc băm bằng `Argon2id`.

### 6.2. Triển khai Non-blocking với Tokio (`spawn_blocking`)

Argon2id cố tình tiêu tốn CPU và RAM để làm chậm kẻ tấn công. Nếu thực thi trực tiếp trên worker thread của Tokio runtime, nó sẽ làm nghẽn Event Loop (**Worker Thread Starvation**), khiến các kết nối mạng khác bị trễ P99 tăng đột biến.

`omni-auth` giải quyết triệt để vấn đề này bằng cách bọc thao tác tính toán mật khẩu vào `tokio::task::spawn_blocking`:

```rust
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Params, Version,
};
use secrecy::{ExposeSecret, SecretString};
use crate::error::AuthError;

/// Adapter băm và so khớp mật khẩu chuẩn hóa.
#[derive(Debug, Clone)]
pub struct PasswordSecurity {
    params: Params,
}

impl PasswordSecurity {
    pub fn new(memory_kib: u32, iterations: u32, parallelism: u32) -> Result<Self, AuthError> {
        let params = Params::new(memory_kib, iterations, parallelism, None)
            .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
        Ok(Self { params })
    }

    /// Băm mật khẩu bất đồng bộ, ủy quyền tính toán sang blocking thread pool của Tokio.
    pub async fn hash_password(&self, password: SecretString) -> Result<String, AuthError> {
        let params = self.params.clone();
        tokio::task::spawn_blocking(move || {
            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);
            let hash = argon2
                .hash_password(password.expose_secret().as_bytes(), &salt)
                .map_err(|e| AuthError::PasswordHashingFailed(e.to_string()))?;
            Ok(hash.to_string())
        })
        .await
        .map_err(|join_err| AuthError::InternalTaskPanic(join_err.to_string()))?
    }

    /// So khớp mật khẩu với hash lưu trong database (Constant-time comparison).
    pub async fn verify_password(
        &self,
        password: SecretString,
        password_hash: String,
    ) -> Result<bool, AuthError> {
        tokio::task::spawn_blocking(move || {
            let parsed_hash = PasswordHash::new(&password_hash)
                .map_err(|e| AuthError::InvalidHashFormat(e.to_string()))?;
            let argon2 = Argon2::default();
            match argon2.verify_password(password.expose_secret().as_bytes(), &parsed_hash) {
                Ok(()) => Ok(true),
                Err(argon2::password_hash::Error::Password) => Ok(false),
                Err(e) => Err(AuthError::PasswordVerificationFailed(e.to_string())),
            }
        })
        .await
        .map_err(|join_err| AuthError::InternalTaskPanic(join_err.to_string()))?
    }
}
```

---

## 7. Token Engine: JWT Specification & Claims Architecture

### 7.1. Cấu trúc Claims Chuẩn (`AuthUserClaims`)

`AuthUserClaims` tuân thủ nghiêm ngặt **RFC 7519** kết hợp các trường định danh doanh nghiệp đa tenant của OmniKubernetes. Mọi định danh khóa chính (`sub`, `session_id`, `tenant_id`, `jti`) đều sử dụng **UUIDv7** để đảm bảo tính sắp xếp theo thời gian (Time-Ordered):

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Định danh loại token trong vòng đời xác thực.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// Token truy cập ngắn hạn (15 phút), mang đầy đủ quyền hạn.
    Access,
    /// Token làm mới dài hạn (7 ngày), chỉ dùng để xin cấp cặp token mới.
    Refresh,
    /// Khóa API định danh dịch vụ máy bay (Machine-to-Machine / Service Account).
    ApiKey,
}

/// Vai trò người dùng trong hệ thống (Role-Based Access Control).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    SuperAdmin,
    Admin,
    Operator,
    User,
    ServiceAccount,
    Custom(String),
}

/// Bộ Claims hoàn chỉnh được mã hóa trong payload của JWT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUserClaims {
    // --- Registered Claims (RFC 7519) ---
    /// Subject: User ID (UUIDv7 duy nhất của người dùng).
    pub sub: Uuid,
    /// Issuer: Định danh hệ thống phát hành token (vd: "urn:omni:auth").
    pub iss: String,
    /// Audience: Định danh hệ sinh thái tiêu thụ token (vd: "urn:omni:api").
    pub aud: String,
    /// Expiration Time: Dấu thời gian hết hạn (Unix timestamp giây).
    pub exp: i64,
    /// Not Before: Token chỉ hợp lệ sau thời điểm này.
    pub nbf: i64,
    /// Issued At: Dấu thời gian phát hành token.
    pub iat: i64,
    /// JWT ID: Định danh duy nhất của token (UUIDv7), dùng cho JTI Revocation Blacklist.
    pub jti: Uuid,

    // --- Custom Enterprise Claims ---
    /// Loại token: Access | Refresh | ApiKey.
    pub token_type: TokenType,
    /// Tenant ID: Định danh tổ chức/khách hàng cho kiến trúc Đa Tenant (Multi-Tenant).
    pub tenant_id: Option<Uuid>,
    /// Session ID: Định danh phiên đăng nhập người dùng (UUIDv7).
    pub session_id: Option<Uuid>,
    /// Danh sách các vai trò được gán cho người dùng.
    pub roles: Vec<Role>,
    /// Danh sách các quyền hạn vi mô dạng danh từ:động từ (vd: "order:create", "payment:refund").
    pub permissions: Vec<String>,
    /// Email người dùng (tùy chọn, phục vụ hiển thị nhanh).
    pub email: Option<String>,
}

impl AuthUserClaims {
    /// Kiểm tra xem token đã hết hạn tại thời điểm tham chiếu hay chưa (có tính clock skew).
    pub fn is_expired_at(&self, now_timestamp: i64, clock_skew_leeway: i64) -> bool {
        now_timestamp >= (self.exp + clock_skew_leeway)
    }

    /// Kiểm tra người dùng có sở hữu vai trò cụ thể hay không.
    pub fn has_role(&self, role: &Role) -> bool {
        self.roles.iter().any(|r| r == role || *r == Role::SuperAdmin)
    }

    /// Kiểm tra người dùng có quyền cụ thể hay không (hỗ trợ wildcard matching "order:*").
    pub fn has_permission(&self, permission: &str) -> bool {
        if self.roles.iter().any(|r| *r == Role::SuperAdmin) {
            return true;
        }
        self.permissions.iter().any(|p| {
            p == permission || p == "*" || (p.ends_with(":*") && permission.starts_with(&p[..p.len() - 1]))
        })
    }
}
```

### 7.2. Cặp Token Hoàn chỉnh (`TokenPair`)

```rust
/// Cặp Token trả về cho Client sau khi đăng nhập hoặc làm mới thành công.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// Access token dạng JWT chuỗi nén (Compact Serialization: Header.Payload.Signature).
    pub access_token: String,
    /// Refresh token dạng JWT (hoặc mã opaque token liên kết family).
    pub refresh_token: String,
    /// Loại mã thông báo (luôn là "Bearer").
    pub token_type: String,
    /// Thời gian sống của access token tính bằng giây.
    pub expires_in: u64,
}
```

### 7.3. Token Engine & Luân chuyển khóa (Key Rotation via `kid`)

Khi ký token, `TokenEngine` tự động đính kèm `kid` (Key ID) vào Header của JWT:

```rust
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use secrecy::ExposeSecret;

pub struct TokenEngine {
    config: AuthConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
}

impl TokenEngine {
    pub fn new(config: AuthConfig) -> Result<Self, AuthError> {
        let (encoding_key, decoding_key, algorithm) = match config.jwt_algorithm {
            SupportedAlgorithm::HS256 => (
                EncodingKey::from_secret(config.jwt_secret.expose_secret().as_bytes()),
                DecodingKey::from_secret(config.jwt_secret.expose_secret().as_bytes()),
                Algorithm::HS256,
            ),
            SupportedAlgorithm::HS384 => (
                EncodingKey::from_secret(config.jwt_secret.expose_secret().as_bytes()),
                DecodingKey::from_secret(config.jwt_secret.expose_secret().as_bytes()),
                Algorithm::HS384,
            ),
            SupportedAlgorithm::HS512 => (
                EncodingKey::from_secret(config.jwt_secret.expose_secret().as_bytes()),
                DecodingKey::from_secret(config.jwt_secret.expose_secret().as_bytes()),
                Algorithm::HS512,
            ),
            SupportedAlgorithm::EdDSA => {
                let priv_pem = config.asymmetric.private_key_pem.as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("EdDSA requires private_key_pem".into()))?;
                let pub_pem = config.asymmetric.public_key_pem.as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("EdDSA requires public_key_pem".into()))?;
                (
                    EncodingKey::from_ed_pem(priv_pem.expose_secret().as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                    DecodingKey::from_ed_pem(pub_pem.as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                    Algorithm::EdDSA,
                )
            }
            SupportedAlgorithm::RS256 => {
                let priv_pem = config.asymmetric.private_key_pem.as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("RS256 requires private_key_pem".into()))?;
                let pub_pem = config.asymmetric.public_key_pem.as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("RS256 requires public_key_pem".into()))?;
                (
                    EncodingKey::from_rsa_pem(priv_pem.expose_secret().as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                    DecodingKey::from_rsa_pem(pub_pem.as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                    Algorithm::RS256,
                )
            }

            SupportedAlgorithm::ES256 => {
                let priv_pem = config.private_key_pem
                    .as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("ES256 requires private_key_pem".into()))?;
                let pub_pem = config.public_key_pem
                    .as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("ES256 requires public_key_pem".into()))?;

                (
                    EncodingKey::from_ec_pem(priv_pem.expose_secret().as_bytes())?,
                    DecodingKey::from_ec_pem(pub_pem.expose_secret().as_bytes())?,
                    Algorithm::ES256,
                )
            }
            SupportedAlgorithm::ES384 => {
                let priv_pem = config.private_key_pem
                    .as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("ES384 requires private_key_pem".into()))?;
                let pub_pem = config.public_key_pem
                    .as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("ES384 requires public_key_pem".into()))?;

                (
                    EncodingKey::from_ec_pem(priv_pem.expose_secret().as_bytes())?,
                    DecodingKey::from_ec_pem(pub_pem.expose_secret().as_bytes())?,
                    Algorithm::ES384,
                )
            }
            SupportedAlgorithm::ES512 => {
                let priv_pem = config.private_key_pem
                    .as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("ES512 requires private_key_pem".into()))?;
                let pub_pem = config.public_key_pem
                    .as_ref()
                    .ok_or_else(|| AuthError::CryptoConfiguration("ES512 requires public_key_pem".into()))?;

                (
                    EncodingKey::from_ec_pem(priv_pem.expose_secret().as_bytes())?,
                    DecodingKey::from_ec_pem(pub_pem.expose_secret().as_bytes())?,
                    Algorithm::ES512,
                )
            }
};

        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[&config.issuer]);
        validation.set_audience(&[&config.audience]);
        validation.leeway = config.clock_skew_leeway_secs;
        validation.validate_exp = true;
        validation.validate_nbf = true;

        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            validation,
        })
    }

    /// Ký một tập claims thành chuỗi JWT.
    pub fn encode_claims(&self, claims: &AuthUserClaims) -> Result<String, AuthError> {
        let mut header = Header::new(self.validation.algorithms[0]);
        header.kid = Some(self.config.jwt_key_id.clone());
        encode(&header, claims, &self.encoding_key).map_err(|e| AuthError::TokenEncodingFailed(e.to_string()))
    }

    /// Giải mã và thẩm định chữ ký, thời hạn, issuer, audience của JWT.
    pub fn decode_token(&self, token: &str) -> Result<AuthUserClaims, AuthError> {
        decode::<AuthUserClaims>(token, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                jsonwebtoken::errors::ErrorKind::InvalidSignature => AuthError::InvalidSignature,
                jsonwebtoken::errors::ErrorKind::ImmatureSignature => AuthError::TokenNotYetValid,
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => AuthError::InvalidIssuer,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => AuthError::InvalidAudience,
                _ => AuthError::MalformedToken(e.to_string()),
            })
    }
}
```

---

## 8. Identity Propagation & Zero-Trust Inter-Service Architecture

### 8.1. Hai hình thái Định tuyến Lưu lượng (Traffic Topologies)

Trong cụm Kubernetes của OmniKubernetes, hệ thống hỗ trợ cả hai mô hình định tuyến bảo mật:

```mermaid
flowchart TD
    subgraph ScenarioA["KỊCH BẢN A: GATEWAY SIGNED CONTEXT PROPAGATION"]
        ClientA["Client"] -->|"1. Authorization: Bearer <jwt>"| GW["API Gateway / Envoy Ingress"]
        GW -->|"Thẩm định JWT tập trung"| GW
        GW -->|"2. Header nội bộ được ký HMAC:<br/>X-User-Id: ...<br/>X-Tenant-Id: ...<br/>X-Roles: ...<br/>X-Omni-Timestamp: ...<br/>X-Omni-Signature: HMAC-SHA256(...)"| SvcOrderA["services/order"]
        SvcOrderA -->|"Thẩm định chữ ký Gateway trong < 5 microseconds"| SvcOrderA
    end

    subgraph ScenarioB["KỊCH BẢN B: DIRECT END-TO-END JWT PROPAGATION"]
        ClientB["Client"] -->|"1. Authorization: Bearer <jwt>"| GW_B["Ingress Controller (L4/L7 Passthrough)"]
        GW_B -->|"2. Forward nguyên vẹn Bearer <jwt>"| SvcOrderB["services/order"]
        SvcOrderB -->|"Tự decode và xác thực JWT bằng Public Key"| SvcOrderB
    end
```

### 8.2. Giao thức Gateway Signed Context (Chống Giả mạo Header Nội bộ)

Để tránh tình trạng downstream microservice tin tưởng mù quáng header `X-User-Id` (khiến một pod bị thâm nhập có thể giả danh `admin` gọi các service nội bộ khác), **Giao thức Ký Ngữ cảnh Gateway (Signed Gateway Context Protocol)** được áp dụng:

1. **Chuỗi dữ liệu chuẩn hóa (Canonical String):**
   Gateway tạo chữ ký HMAC-SHA256 trên chuỗi chuẩn hóa ghép từ:
   $$\text{canonical\_payload} = \text{user\_id} \parallel ":" \parallel \text{tenant\_id} \parallel ":" \parallel \text{roles} \parallel ":" \parallel \text{timestamp}$$
2. **Ký chữ ký:**
   $$\text{signature} = \text{HMAC-SHA256}(\text{internal\_gateway\_secret}, \text{canonical\_payload})$$
3. **Thẩm định tại Downstream Pod:**
   - Downstream pod nhận các header:
     - `X-User-Id`
     - `X-Tenant-Id` (nếu có)
     - `X-User-Roles` (chuỗi phân cách bằng dấu phẩy)
     - `X-Omni-Timestamp` (Unix epoch giây)
     - `X-Omni-Signature` (Hex string)
   - Kiểm tra **Freshness Check:** $|\text{now} - \text{timestamp}| \le \text{signature\_max\_age\_secs}$ (mặc định 30 giây). Nếu quá hạn $\rightarrow$ từ chối ngay (chống Replay Attack).
   - Tính toán lại HMAC bằng `internal_gateway_secret` qua phép so sánh Constant-Time (`subtle::ConstantTimeEq`).
   - Nếu chữ ký hợp lệ: Khởi tạo struct `AuthenticatedUser` ngay lập tức mà không cần decode JWT!

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use secrecy::ExposeSecret;

type HmacSha256 = Hmac<Sha256>;

pub fn verify_gateway_signature(
    secret: &secrecy::SecretString,
    user_id: &str,
    tenant_id: Option<&str>,
    roles: &str,
    timestamp_secs: i64,
    current_time_secs: i64,
    max_age_secs: i64,
    provided_signature_hex: &str,
) -> Result<bool, AuthError> {
    // 1. Freshness check chống Replay Attack
    if (current_time_secs - timestamp_secs).abs() > max_age_secs {
        return Err(AuthError::SignatureExpired);
    }

    // 2. Tái tạo canonical payload
    let canonical = format!(
        "{}:{}:{}:{}",
        user_id,
        tenant_id.unwrap_or(""),
        roles,
        timestamp_secs
    );

    // 3. Tính toán và so sánh Constant-Time
    let mut mac = HmacSha256::new_from_slice(secret.expose_secret().as_bytes())
        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
    mac.update(canonical.as_bytes());

    let provided_bytes = hex::decode(provided_signature_hex)
        .map_err(|_| AuthError::InvalidSignatureFormat)?;

    match mac.verify_slice(&provided_bytes) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}
```

---

## 9. Token Revocation, Session Management & Refresh Token Family

### 9.1. Nghịch lý Stateless vs. Thu hồi Thời gian thực

Điểm yếu cố hữu của Stateless JWT là **không thể thu hồi trước khi hết hạn `exp`**. Nếu người dùng đăng xuất, hoặc quản trị viên khóa tài khoản khẩn cấp, một access token còn hạn 15 phút vẫn có thể tiếp tục được sử dụng.

`omni-auth` giải quyết triệt để vấn đề này thông qua mô hình **Phân tầng Thu hồi 3 Cấp (Tiered Revocation Architecture)**:

```mermaid
flowchart TD
    Req["Incoming Request (Bearer JWT)"] --> LocalVerify["Tầng 1: Local JWT Cryptographic Verification<br/>(Ký đúng? Chưa hết hạn? Issuer/Audience khớp?)"]
    LocalVerify -- Hợp lệ --> JtiCheck{"Tầng 2: JTI Blacklist Check<br/>(Kiểm tra omni-cache Redis)"}
    LocalVerify -- Không hợp lệ --> Err401["Trả lỗi 401 Unauthorized"]

    JtiCheck -- "Key tồn tại trong Redis (Blacklisted)" --> ErrRevoked["401 Token Revoked<br/>(ERR_TOKEN_REVOKED)"]
    JtiCheck -- "Key không tồn tại (Clean)" --> Grant["Cho phép thực thi Handler"]

    subgraph RTR["Tầng 3: Refresh Token Rotation & Token Family (Phát hiện Đánh cắp)"]
        RefreshReq["Yêu cầu cấp lại: POST /auth/refresh"] --> FamCheck{"Kiểm tra Refresh Token đã từng bị dùng chưa?"}
        FamCheck -- "Token hợp lệ chưa dùng" --> Issue["Hủy token cũ + Cấp cặp Token mới cùng Family ID"]
        FamCheck -- "Phát hiện Token ĐÃ BỊ DÙNG TRƯỚC ĐÓ" --> Alert["CẢNH BÁO: TOKEN THEFT DETECTED!<br/>Vô hiệu hóa ngay TOÀN BỘ Family Tokens của người dùng"]
    end
```

### 9.2. Tầng 2: JTI Blacklist trên `omni-cache` (Redis)

Khi người dùng đăng xuất hoặc bị khóa tài khoản, `jti` (UUIDv7 duy nhất của token) được đẩy vào Redis thông qua `omni-cache`:

- **Redis Key:** `omni:auth:revocation:v1:{jti}`
- **Redis Value:** JSON mang lý do (`"logout"`, `"admin_ban"`, `"password_changed"`) và timestamp.
- **Redis TTL:** Được đặt **chính xác bằng thời gian sống còn lại của Token**:
  $$\text{TTL} = \text{claims.exp} - \text{now()}$$
- **Lợi ích kiến trúc:** Redis tự động thu dọn bộ nhớ sau khi token tự nhiên hết hạn. Không bao giờ xảy ra hiện tượng rò rỉ hoặc phình bộ nhớ Redis theo thời gian!

### 9.3. Tầng 3: Refresh Token Rotation (RTR) & Token Family Pattern

Để ngăn chặn việc kẻ tấn công đánh cắp Refresh Token lưu dài hạn:

1. Mỗi khi Client gọi API `/auth/refresh`, Refresh Token gửi lên **bị vô hiệu hóa ngay lập tức**.
2. Một cặp Access Token & Refresh Token mới được sinh ra và gán cùng một `family_id` (UUID).
3. Nếu một Refresh Token cũ **đã bị sử dụng lại lần thứ hai** (dấu hiệu kẻ tấn công hoặc nạn nhân đang dùng cùng một token bị lộ):
   - Hệ thống kích hoạt cơ chế phòng vệ tự động: **Hủy toàn bộ tất cả token thuộc `family_id` đó ngay lập tức**.
   - Buộc người dùng thật phải đăng nhập lại từ đầu bằng mật khẩu + MFA, ngăn chặn kẻ tấn công duy trì phiên trái phép.

---

## 10. Axum Extractors & Declarative Authorization Guards

### 10.1. Bộ Axum Extractors Kiểu tĩnh

`omni-auth` triển khai trait `axum::extract::FromRequestParts` để trích xuất danh tính trực tiếp trong tham số của controller:

```rust
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};
use uuid::Uuid;
use crate::{error::AuthError, token::Role};

/// Danh tính người dùng đã được xác thực hoàn toàn (Bắt buộc đăng nhập).
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub roles: Vec<Role>,
    pub permissions: Vec<String>,
    pub email: Option<String>,
    pub token_id: Uuid,
}

impl AuthenticatedUser {
    pub fn is_super_admin(&self) -> bool {
        self.roles.contains(&Role::SuperAdmin)
    }

    pub fn has_role(&self, role: &Role) -> bool {
        self.roles.iter().any(|r| r == role || *r == Role::SuperAdmin)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        if self.is_super_admin() {
            return true;
        }
        self.permissions.iter().any(|p| p == permission || p == "*")
    }
}

/// Extractor tùy chọn: Không bắt buộc đăng nhập (Dùng cho public endpoint có cá nhân hóa).
#[derive(Debug, Clone)]
pub struct OptionalUser(pub Option<AuthenticatedUser>);

/// Extractor cô lập ngữ cảnh đa Tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentTenant(pub Uuid);

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = omni_errors::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 1. Kiểm tra xem danh tính đã được Auth Middleware tiêm vào Extensions chưa
        if let Some(user) = parts.extensions.get::<AuthenticatedUser>() {
            return Ok(user.clone());
        }

        // 2. Nếu chưa có trong extensions, trả về lỗi 401 Unauthorized theo chuẩn RFC 9457
        Err(omni_errors::AppError::Unauthorized)
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for CurrentTenant
where
    S: Send + Sync,
{
    type Rejection = omni_errors::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<AuthenticatedUser>() {
            if let Some(tenant_id) = user.tenant_id {
                return Ok(CurrentTenant(tenant_id));
            }
        }
        // Thiếu Tenant ID hoặc người dùng không thuộc Tenant nào
        Err(omni_errors::AppError::Forbidden)
    }
}
```

### 10.2. Declarative Guards & Macro Helpers

Loại bỏ hoàn toàn code kiểm tra quyền thủ công bên trong handler. Lập trình viên sử dụng các guard khai báo:

```rust
// services/order/src/routes.rs
use axum::{routing::{get, post}, Router};
use omni_auth::{
    extractor::{AuthenticatedUser, CurrentTenant},
    guard::{require_permission, require_role, Role},
};

pub fn order_routes() -> Router {
    Router::new()
        // Endpoint công khai: Không yêu cầu Auth
        .route("/health", get(health_handler))

        // Endpoint yêu cầu đăng nhập cơ bản
        .route("/me/profile", get(get_profile_handler))

        // Endpoint yêu cầu quyền tạo đơn: "orders:create"
        .route(
            "/orders",
            post(create_order_handler)
                .route_layer(require_permission("orders:create")),
        )

        // Endpoint yêu cầu vai trò Quản trị viên: Role::Admin hoặc Role::SuperAdmin
        .route(
            "/admin/system-stats",
            get(admin_stats_handler)
                .route_layer(require_role(Role::Admin)),
        )
}

// Handler nghiệp vụ cực kỳ tinh gọn và type-safe
async fn create_order_handler(
    user: AuthenticatedUser,         // Tự động đảm bảo 401 nếu chưa đăng nhập
    tenant: CurrentTenant,           // Tự động đảm bảo 403 nếu thiếu tenant
    // Json(payload): Json<CreateOrderDto>,
) -> Result<ApiResponse<OrderDto>, AppError> {
    tracing::info!(user_id = %user.user_id, tenant_id = %tenant.0, "Tạo đơn hàng mới");
    // Nghiệp vụ thuần túy...
}
```

---

## 11. Error Taxonomy & RFC 9457 Integration

### 11.1. Bảng Phân loại Mã lỗi Chi tiết (`AuthError`)

Mọi lỗi phát sinh trong phân hệ Auth được định nghĩa kiểu tĩnh trong enum `AuthError` và tự động ánh xạ sang `omni_errors::AppError` tuân thủ tiêu chuẩn **RFC 9457 Problem Details** (`application/problem+json`):

| Mã lỗi Nghiệp vụ (`code`)      | HTTP Status | Tiêu đề lỗi (RFC 9457 `title`) | Chi tiết giải thích (`detail`)                                               |
| :----------------------------- | :---------: | :----------------------------- | :--------------------------------------------------------------------------- |
| `ERR_AUTH_MISSING_CREDENTIALS` |    `401`    | Unauthorized                   | Thiếu header `Authorization: Bearer <token>` hoặc thông tin xác thực.        |
| `ERR_AUTH_TOKEN_EXPIRED`       |    `401`    | Token Expired                  | Mã thông báo truy cập đã hết hạn. Hãy sử dụng Refresh Token để cấp mới.      |
| `ERR_AUTH_TOKEN_REVOKED`       |    `401`    | Token Revoked                  | Mã thông báo đã bị thu hồi do người dùng đăng xuất hoặc phiên bị hủy.        |
| `ERR_AUTH_INVALID_SIGNATURE`   |    `401`    | Invalid Signature              | Chữ ký số của mã thông báo không hợp lệ hoặc đã bị can thiệp.                |
| `ERR_AUTH_MALFORMED_TOKEN`     |    `401`    | Malformed Token                | Định dạng mã thông báo không đúng chuẩn JWT (thiếu segments).                |
| `ERR_AUTH_INVALID_CREDENTIALS` |    `401`    | Invalid Credentials            | Tên đăng nhập hoặc mật khẩu không chính xác.                                 |
| `ERR_AUTH_INSUFFICIENT_ROLES`  |    `403`    | Forbidden Role                 | Người dùng không sở hữu vai trò yêu cầu để thực hiện hành động này.          |
| `ERR_AUTH_INSUFFICIENT_PERMS`  |    `403`    | Forbidden Permission           | Người dùng thiếu quyền vi mô (`permission`) cần thiết.                       |
| `ERR_AUTH_TENANT_MISMATCH`     |    `403`    | Tenant Access Denied           | Người dùng không có quyền truy cập vào không gian dữ liệu Tenant này.        |
| `ERR_AUTH_SIGNATURE_EXPIRED`   |    `401`    | Gateway Signature Expired      | Chữ ký nội bộ giữa Gateway và Microservice đã quá hạn (lệch đồng hồ/replay). |

### 11.2. Mẫu Phản hồi Lỗi Chuẩn RFC 9457 (`application/problem+json`)

Khi token bị hết hạn, response trả về cho Mobile App hoặc Frontend tuân thủ chính xác:

```http
HTTP/1.1 401 Unauthorized
Content-Type: application/problem+json; charset=utf-8
X-Request-Id: 018f4a2b-8a5e-7a1b-9c3d-e4f5a6b7c8d9
traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01

{
  "type": "https://api.omni.internal/errors/auth/token-expired",
  "title": "Token Expired",
  "status": 401,
  "detail": "Mã xác thực của bạn đã hết hạn lúc 2026-11-02T09:45:00Z. Vui lòng làm mới phiên làm việc.",
  "instance": "/api/v1/orders",
  "code": "ERR_AUTH_TOKEN_EXPIRED",
  "meta": {
    "request_id": "018f4a2b-8a5e-7a1b-9c3d-e4f5a6b7c8d9",
    "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
    "timestamp": "2026-11-02T09:58:42Z",
    "locale": "vi-VN"
  }
}
```

---

## 12. Observability, Telemetry & Security Auditing

### 12.1. OpenTelemetry Tracing & Context Enrichment

Khi một request được xác thực thành công, Auth Middleware lập tức đính kèm danh tính vào Span hiện tại:

```rust
// Gắn thuộc tính danh tính an toàn (không PII) vào tracing span
let span = tracing::Span::current();
span.record("user.id", claims.sub.to_string());
if let Some(tenant_id) = claims.tenant_id {
    span.record("tenant.id", tenant_id.to_string());
}
span.record("auth.token_id", claims.jti.to_string());
span.record("auth.roles_count", claims.roles.len() as u64);
```

> [!WARNING]
> **Tuyệt đối không bao giờ** gắn các giá trị sau vào Tracing Span, Metrics Labels hoặc Access Logs:
>
> - Raw JWT Token (chuỗi `Bearer eyJ...`).
> - Password thô hoặc Password Hash.
> - Refresh Token hoặc Client Secret.
> - Email cá nhân nếu chưa qua bộ lọc PII Sanitizer của `omni-logging`.

### 12.2. Bộ Chỉ số Prometheus Chuẩn Hóa (Auth Metrics)

Tuân thủ nghiêm ngặt nguyên tắc **Low Cardinality** của `omni-metrics`:

```mermaid
flowchart LR
    A["Auth Operations"] --> M1["Counter: auth_validations_total<br/>Labels: {status, algorithm}"]
    A --> M2["Counter: auth_failures_total<br/>Labels: {reason}"]
    A --> M3["Histogram: auth_crypto_duration_seconds<br/>Labels: {operation}"]
    A --> M4["Gauge: auth_active_sessions<br/>Labels: {tenant_id_hashed}"]
```

1. **`auth_validations_total` (Counter):**
   - Đếm tổng số lần thẩm định token.
   - Nhãn: `status` (`"success"`, `"failure"`), `algorithm` (`"hs256"`, `"eddsa"`, `"gateway_signature"`).
2. **`auth_failures_total` (Counter):**
   - Đếm số lần từ chối xác thực / phân quyền.
   - Nhãn: `reason` (`"expired"`, `"invalid_sig"`, `"revoked"`, `"insufficient_role"`, `"insufficient_perm"`, `"missing_token"`).
3. **`auth_crypto_duration_seconds` (Histogram):**
   - Đo thời lượng tính toán mật mã (Argon2 hash/verify, JWT sign/verify).
   - Buckets tối ưu: `[0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0]`.
   - Nhãn: `operation` (`"jwt_encode"`, `"jwt_decode"`, `"argon2_hash"`, `"argon2_verify"`).

---

## 13. Security Hardening & Zero-Leak Compliance

### 13.1. Chống các Lỗ hổng OWASP API Security Top 10 (2023)

- **API1:2023 - Broken Object Level Authorization (BOLA / IDOR):**
  - Tích hợp `CurrentTenant` và `AuthenticatedUser.user_id` trực tiếp vào câu lệnh SQLx hoặc repository layer.
  - Ngăn chặn triệt để tình trạng User A sửa `id` trong URL để xem trộm đơn hàng của User B.
- **API2:2023 - Broken Authentication:**
  - Áp dụng chuẩn Argon2id, giới hạn thời gian sống Access Token (15 phút), Refresh Token Rotation, kiểm tra JTI Blacklist thời gian thực.
- **API5:2023 - Broken Function Level Authorization (BFLA):**
  - Khai báo quyền chặt chẽ ngay tại router bằng `require_role(Role::Admin)` hoặc `require_permission(...)`. Tránh việc quên đặt guard ở các controller quản trị.

### 13.2. Chống Timing Attacks (Tấn công Kênh bên)

Mọi phép so sánh chuỗi nhạy cảm:

- So khớp token.
- So khớp mã bí mật Gateway Signature.
- So khớp chữ ký HMAC.

Bắt buộc sử dụng hàm so sánh thời gian bất biến (**Constant-Time Equality**) từ crate `subtle` hoặc hàm tích hợp `hmac::Mac::verify_slice`. Không bao giờ sử dụng toán tử so sánh chuỗi thông thường `==` cho các giá trị cryptographic (vì `==` sẽ ngắt sớm ngay khi gặp byte khác biệt đầu tiên, làm rò rỉ độ dài chuỗi khớp qua microsecond latency).

### 13.3. Bảo vệ Bộ nhớ Heap (Memory Zeroization)

Tất cả các trường mật khẩu, private key và cryptographic secret được bọc bằng `SecretString` từ crate `secrecy`. Khi struct bị drop, bộ nhớ heap chứa nội dung bí mật được tự động ghi đè bằng các byte `0x00` (Zeroize), ngăn chặn rò rỉ bí mật qua Memory Dumps hoặc Core Dumps khi tiến trình gặp sự cố.

---

## 14. Implementation Blueprint & Crate Layout (`libs/auth`)

### 14.1. Cấu trúc Thư mục Module của `libs/auth`

```text
libs/auth/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs                  # Public facades, re-exports & feature gates
    ├── config.rs               # AuthConfig, KeyMode, Algorithm enums & validations
    ├── error.rs                # AuthError enum, RFC 9457 conversion, Result alias
    ├── crypto/
    │   ├── mod.rs              # Cryptographic facade
    │   └── password.rs         # Argon2id hasher/verifier with spawn_blocking
    ├── token/
    │   ├── mod.rs              # Token engine facade
    │   ├── claims.rs           # AuthUserClaims, Role, TokenType, Permission definitions
    │   ├── engine.rs           # JsonWebToken encode/decode with jsonwebtoken crate
    │   └── pair.rs             # TokenPair representation
    ├── extractor/
    │   ├── mod.rs              # Axum FromRequestParts extractors
    │   ├── user.rs             # AuthenticatedUser & OptionalUser extractors
    │   └── tenant.rs           # CurrentTenant multi-tenancy extractor
    ├── guard/
    │   ├── mod.rs              # Authorization guards & layers
    │   ├── role.rs             # RequireRole, RequireAnyRole layers
    │   └── permission.rs       # RequirePermission, RequireAllPermissions layers
    ├── middleware/
    │   ├── mod.rs              # Axum middlewares
    │   ├── auth_layer.rs       # Global Auth Middleware (JWT / Signed Context decoding)
    │   └── signed_context.rs   # Gateway HMAC signed context verifier
    ├── revocation/
    │   ├── mod.rs              # Revocation facade (behind feature "revocation")
    │   ├── blacklist.rs        # JTI Blacklist operations using omni-cache
    │   └── family.rs           # Refresh token family & reuse detection logic
    ├── bridge/
    │   ├── mod.rs              # Context bridging integrations
    │   ├── request_context.rs  # Sync user_id/tenant_id to omni_common::RequestContext
    │   └── db_session.rs       # Sync AuthenticatedUser to omni_database::Actor::User
    ├── instrument.rs           # Prometheus metrics & OTel audit event helpers
    └── test_utils.rs           # Mock claims, test token generator (feature = "test-util")
```

### 14.2. Khai báo `libs/auth/Cargo.toml` Hoàn chỉnh

```toml
[package]
name = "omni-auth"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[lints]
workspace = true

[features]
default = ["revocation", "axum"]
revocation = ["dep:omni-cache"]
axum = ["dep:axum"]
db-session = ["dep:omni-database"]
test-util = []

[dependencies]
# Omni Workspace dependencies
omni-common = { workspace = true }
omni-errors = { workspace = true }
omni-logging = { workspace = true }
omni-metrics = { workspace = true }
omni-cache = { workspace = true, optional = true }
omni-database = { workspace = true, optional = true }

# Web Framework
axum = { workspace = true, optional = true }

# Cryptography & Security
argon2 = { workspace = true }
jsonwebtoken = { workspace = true }
secrecy = { workspace = true }
sha2 = { workspace = true }
hmac = { workspace = true }
hex = { workspace = true }
rand = { workspace = true }

# Concurrency & Async
tokio = { workspace = true, features = ["sync", "rt"] }
async-trait = { workspace = true }

# Data Modeling & Serde
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
uuid = { workspace = true, features = ["v4", "v7", "serde"] }
chrono = { workspace = true, features = ["serde", "clock"] }
validator = { workspace = true, features = ["derive"] }
thiserror = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
```

---

## 15. Testing Strategy & Verification Matrix

Để đảm bảo tính bất khả xâm phạm của phân hệ bảo mật, `omni-auth` áp dụng ma trận kiểm thử 4 lớp:

```mermaid
flowchart TD
    T1["Lớp 1: Unit Tests (Pure Logic)<br/>- Argon2 hashing & verification<br/>- JWT encode/decode/expiration<br/>- Canonical HMAC signature matching"] --> T2["Lớp 2: Attack Simulation Tests<br/>- Expired token injection<br/>- Tampered payload & signature mutation<br/>- Replay attack with expired timestamp<br/>- Timing attack resistance"]
    T2 --> T3["Lớp 3: Axum Integration Tests<br/>- Extractors 401/403 rejections<br/>- Declarative guards validation<br/>- Multi-tenant boundary isolation"]
    T3 --> T4["Lớp 4: Quality Gate Protocol<br/>- cargo fmt --all<br/>- cargo hakari generate<br/>- cargo clippy -- -D warnings<br/>- cargo test --all"]
```

### 15.1. Kịch bản Kiểm thử Tấn công Giả lập (Attack Simulation)

1. **Token Tampering Test:** Sửa đổi 1 ký tự trong payload của JWT $\rightarrow$ `TokenEngine::decode_token` bắt buộc từ chối với `AuthError::InvalidSignature`.
2. **Expired Token Test:** Tạo token với `exp = now - 60s` $\rightarrow$ Thẩm định bắt buộc trả về `AuthError::TokenExpired`, status HTTP 401 với code `ERR_AUTH_TOKEN_EXPIRED`.
3. **Replay Attack Test:** Gửi lại header Gateway có chữ ký hợp lệ nhưng `timestamp = now - 60s` (vượt ngưỡng `max_age = 30s`) $\rightarrow$ Bắt buộc bị từ chối với `AuthError::SignatureExpired`.
4. **Token Reuse Detection Test:** Sử dụng một Refresh Token đã bị thu hồi $\rightarrow$ Kích hoạt `TokenTheftDetected` và vô hiệu hóa toàn bộ tokens trong cùng family.

---

## 16. The Golden Path Integration (Mẫu Ứng dụng Thực tế)

Sau khi `omni-auth` hoàn thành, một Domain Microservice (như `services/order`) tích hợp bảo mật chỉ trong 10 dòng mã khai báo:

```rust
// services/order/src/main.rs
use omni_auth::{
    extractor::{AuthenticatedUser, CurrentTenant},
    guard::require_permission,
    middleware::AuthLayer,
    TokenEngine,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load("order-service")?;

    // Khởi tạo Token Engine từ cấu hình chuẩn
    let token_engine = TokenEngine::new(config.auth.clone())?;

    let app = AppBuilder::new("order-service")
        .with_observability_middleware()
        // Cắm AuthLayer làm middleware xác thực trung tâm
        .with_middleware(AuthLayer::new(token_engine))
        .mount_api_routes(routes::order_router())
        .build();

    omni_common::server::serve_with_graceful_shutdown(app, config.server.port).await?;
    Ok(())
}
```

---

## 17. Quality Gate Checklist (Definition of Done)

Theo quy định nghiêm ngặt tại [`AGENTS.md`](file://AGENTS.md) và [`.agents/rules/quality-gate.md`](file://.agents/rules/quality-gate.md), quá trình triển khai mã nguồn cho Bước 5 phải vượt qua:

```bash
# 1. Format code chuẩn xác
cargo fmt --all

# 2. Đồng bộ cây phụ thuộc workspace
cargo hakari generate

# 3. Linter kiểm tra không lỗi (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings

# 4. Toàn bộ test suite chạy thành công
cargo test --all
```

- [ ] Cấu hình mở rộng `AuthConfig` và guardrails `validate_auth_guardrails` hoàn tất.
- [ ] Module `crypto::password` với `Argon2id` và `spawn_blocking` hoàn tất.
- [ ] Module `token::engine` và `AuthUserClaims` với UUIDv7 và đa thuật toán hoàn tất.
- [ ] Module `extractor` (`AuthenticatedUser`, `CurrentTenant`) cho Axum 0.8 hoàn tất.
- [ ] Module `guard` (`require_role`, `require_permission`) hoàn tất.
- [ ] Module `revocation` (JTI Blacklist trên `omni-cache` và Refresh Token Family) hoàn tất.
- [ ] Cầu nối sang `RequestContext` và PostgreSQL `SessionContext` (`Actor::User`) hoàn tất.
- [ ] Bộ chỉ số Prometheus và OTel Tracing Attributes hoàn tất.
- [ ] 100% tài liệu i18n các mã lỗi `ERR_AUTH_*` được bổ sung vào `libs/common/locales/{vi-VN,en-US,ja-JP}`.
- [ ] Cập nhật nhật ký vào mục `[Unreleased]` trong [`CHANGELOG.md`](file://CHANGELOG.md).
