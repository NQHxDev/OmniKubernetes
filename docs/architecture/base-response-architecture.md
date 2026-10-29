# OmniKubernetes Architecture Specification: Enterprise Base Response & i18n Standard

- **Document Version:** `2.0.0`
- **Status:** Verified
- **Author:** System Architecture Team
- **Target Platform:** Rust (Axum / Actix-web Ecosystem), Tokio Runtime, Envoy Gateway & Kubernetes Microservices

---

## 1. Executive Summary & Core Principles

Trong hệ thống kiến trúc phân tán quy mô doanh nghiệp (**Enterprise Microservices on Kubernetes**) của **OmniKubernetes**, việc thiết kế giao thức dữ liệu trả về (Data Contracts) đóng vai trò sống còn đối với trải nghiệm lập trình viên (DX), tính ổn định của hệ thống, khả năng chịu tải và năng lực giám sát (Observability).

Tiêu chuẩn kiến trúc này xác lập **6 nguyên lý cốt lõi (Core Architectural Tenets)**:

1. **HTTP Status Code là Single Source of Truth:**
    - Tuyệt đối **không** áp dụng mô hình phản mẫu (anti-pattern) trả về HTTP 200 OK nhưng body chứa `"success": false` hoặc payload lỗi.
    - Ingress, Envoy Gateway, Service Mesh (Istio/Linkerd) và APM probes bắt buộc phải dựa trên HTTP Status Code chuẩn để tính toán chính xác các chỉ số vận hành vàng: SLA, SLO, Error Budget và Circuit Breaking.
2. **Tuân thủ triệt để tiêu chuẩn quốc tế (Standards-First):**
    - Áp dụng **RFC 9457** (Problem Details for HTTP APIs) cho toàn bộ mã lỗi REST.
    - Áp dụng **RFC 9110** (HTTP Semantics) cho định danh header và xử lý mã trạng thái đặc thù (`204 No Content`, `201 Created`).
    - Thời gian biểu diễn theo **ISO 8601 / RFC 3339** (UTC).
    - Ngôn ngữ và quốc gia tuân thủ **BCP 47** và **RFC 4647** (Language Priority & Subtag Negotiation).
3. **Phân tách Observability hai lớp: `request_id` song hành `trace_id`:**
    - `request_id`: Định danh duy nhất cho **100%** HTTP requests phục vụ gom log (Log Aggregation trên Loki/ELK) và tra cứu sự cố CSKH/kỹ thuật.
    - `trace_id`: W3C Distributed Trace Context phục vụ APM (Grafana Tempo / Jaeger), có thể chịu cơ chế lấy mẫu theo tỷ lệ (Sampling Rate).
4. **Hỗ trợ phân trang kép (Dual Pagination Architecture):**
    - Hỗ trợ song song **Offset-based** (dành cho Web Admin / UI Table cần nhảy số trang) và **Cursor-based / Keyset** (dành cho tập dữ liệu lớn $O(1)$, infinite scroll trên Mobile, event stream, audit trail không gây áp lực quét $O(N)$ lên database).
5. **Đa ngôn ngữ hai chế độ (Dual-mode i18n/l10n):**
    - **Server-side localization:** Render sẵn thông điệp hoàn chỉnh dựa trên Mozilla Fluent cho Client hiển thị ngay lập tức (Toast/Alert/Snackbars).
    - **Client-side enablement:** Cung cấp kèm `code` và `params` động trong validation errors để Native Mobile App (iOS/Android) hoặc SPA Web có toàn quyền tự bản địa hóa ở tầng Presentation nếu người dùng thay đổi ngôn ngữ offline.
6. **Rust Idiomatic & Zero-Cost Abstraction:**
    - Tận dụng tối đa Type System của Rust, Serde serialization, không dynamic allocation thừa, tối ưu hóa zero-copy và compile-time safety.

---

## 2. Phạm vi áp dụng (Scope of Applicability)

### 2.1. Nằm trong phạm vi (In-Scope)

- Toàn bộ External REST JSON APIs (North-South Traffic) cung cấp cho Client bên ngoài (Web Browser, Mobile Native App, Third-party Partners) đi qua API Gateway / Ingress.

### 2.2. Ngoại lệ ngoài phạm vi (Out-of-Scope / Exclusions)

Quy chuẩn Base Response Envelope này **không** áp dụng cho:

- **Giao tiếp nội bộ giữa các microservices (East-West Traffic):** Toàn bộ giao tiếp đồng bộ (synchronous) nội bộ giữa các Microservices trong cụm Kubernetes **bắt buộc sử dụng gRPC / Protocol Buffers (HTTP/2)** để đảm bảo hiệu năng cao, binary serialization, low-latency và strict contract type-safety. Chuẩn gRPC có cơ chế mã lỗi và metadata riêng (gRPC Status Codes, `google.rpc.Status` kèm rich error details, gRPC Metadata/Trailers) nên không sử dụng Base Response JSON Envelope của đặc tả REST này.
- **Kubernetes Health Probes (`/healthz`, `/livez`, `/readyz`):** Trả về HTTP 200 `OK` dạng plain text hoặc JSON tối giản để kubelet kiểm tra nhanh nhất với overhead bằng 0.
- **Prometheus Metrics Endpoint (`/metrics`):** Trả về plain text theo chuẩn OpenMetrics / Prometheus.
- **Binary & Streaming Responses:** File downloads (`application/octet-stream`), Server-Sent Events (`text/event-stream`), WebSocket handshakes.
- **HTTP 204 No Content Responses:** Bắt buộc **không có response body** theo RFC 9110 Section 15.3.5.
- **External Webhook Receivers:** Các endpoint tiếp nhận webhook từ bên thứ 3 (như Stripe, VNPay, GitHub...) phải tuân theo giao thức của bên gửi.

---

## 3. Standards & Compliance References

| Tiêu chuẩn / Đặc tả kỹ thuật     | Cơ quan ban hành   | Ứng dụng cụ thể trong hệ thống                                                                             |
| :------------------------------- | :----------------- | :--------------------------------------------------------------------------------------------------------- |
| **RFC 9457 (thay thế RFC 7807)** | IETF               | Cấu trúc chi tiết lỗi HTTP APIs (`application/problem+json`).                                              |
| **RFC 9110**                     | IETF               | Ngữ nghĩa HTTP: Content-Language, Content-Type, quy tắc 204 No Content.                                    |
| **BCP 47 / RFC 4647**            | IETF               | Định danh ngôn ngữ và thuật toán Language Priority Matching.                                               |
| **ISO 8601 / RFC 3339**          | ISO / IETF         | Định dạng ngày giờ UTC: `YYYY-MM-DDTHH:mm:ss.sssZ`.                                                        |
| **W3C Trace Context (Level 1)**  | W3C Recommendation | Đồng bộ phân tán OpenTelemetry qua header `traceparent`.                                                   |
| **RFC 6901**                     | IETF               | Định danh đường dẫn dữ liệu JSON lồng nhau (JSON Pointer) ánh xạ qua Dot Notation trong `invalid_params`.  |
| **W3C Server-Timing**            | W3C Recommendation | Đo lường và truyền thời gian thực thi (Latency / Duration) qua HTTP Header `Server-Timing: total;dur=...`. |
| **Project Fluent**               | Mozilla            | Công nghệ bản địa hóa ngữ pháp động, số ít/nhiều và biến nội suy.                                          |

---

## 4. Base Response Data Contracts

### 4.1. Metadata Envelope (`ResponseMeta`)

Mọi response thành công hoặc lỗi (ngoại trừ HTTP 204 No Content) đều chứa đối tượng `meta` chuẩn hóa:

```json
{
    "request_id": "01a185db-a70e-777a-8cee-9ad64ad10253",
    "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
    "timestamp": "2026-10-29T08:30:00.125Z",
    "locale": "vi-VN"
}
```

- **`request_id`** (`String`, bắt buộc): UUIDv7 sinh từ Envoy Gateway hoặc Ingress (`X-Request-Id`). Luôn tồn tại trên 100% request để tra cứu log.
- **`trace_id`** (`String`, bắt buộc): W3C Trace ID (32 ký tự hex) liên kết với Distributed APM Trace.
- **`timestamp`** (`String`, bắt buộc): Thời điểm phát sinh response theo chuẩn ISO 8601 UTC.
- **`locale`** (`String`, bắt buộc): Mã BCP 47 của ngôn ngữ thực tế được sử dụng để tạo response.

#### Quy chuẩn đo lường thời gian thực thi (Latency Measurement & W3C Server-Timing):

- **Tuyệt đối không đưa `duration_ms` vào JSON Body:** Trong Rust Axum, Handler là nơi trực tiếp sinh ra `ApiResponse<T>` và serialize thành JSON payload. Tại thời điểm Handler chạy xong, request chưa đi qua Egress Middleware nên không thể biết tổng thời gian hoàn tất xử lý. Nếu Middleware Ingress-Egress can thiệp vào body, nó sẽ buộc phải parse lại toàn bộ JSON, mutate trường `duration_ms` rồi serialize lại — gây lãng phí bộ nhớ, cấp phát động và phá hủy hoàn toàn nguyên lý **Zero-Cost Abstraction**.
- **Chuẩn hóa qua HTTP Response Headers:**
    - **W3C `Server-Timing`:** `Server-Timing: total;dur=12.4` (Chuẩn W3C Recommendation, được hỗ trợ native bởi Chrome/Firefox DevTools, Grafana Tempo/APM, Datadog).
    - **`X-Response-Time`:** `X-Response-Time: 12.4ms` (Tương thích ngược với các client microservices/APM truyền thống).
    - Egress Middleware tính toán `start.elapsed()` và chèn trực tiếp vào Header với độ phức tạp $O(1)$, zero-copy, không đụng chạm JSON payload.

---

### 4.2. Standard Success Response Contract (`ApiResponse<T>`)

Áp dụng cho các response HTTP 200 OK và HTTP 201 Created:

```json
{
    "success": true,
    "code": "OK",
    "message": "Lấy thông tin người dùng thành công.",
    "data": {
        "user_id": "01a185dd-a81b-72fe-be38-bc8b3c2fb39f",
        "email": "user@example.com",
        "full_name": "Nguyen Van A",
        "created_at": "2026-10-29T08:30:00.000Z"
    },
    "meta": {
        "request_id": "01a185dd-dbd1-74eb-96df-2f4a27de348b",
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "timestamp": "2026-10-29T08:30:00.125Z",
        "locale": "vi-VN"
    }
}
```

#### Quy tắc HTTP 201 Created:

- HTTP Status Code: `201 Created`.
- Bắt buộc bổ sung Header: `Location: /api/v1/users/usr_01HZX87Q1K7`.
- Body tuân thủ đúng `ApiResponse<T>`.

#### Quy tắc HTTP 204 No Content:

- HTTP Status Code: `204 No Content`.
- **Tuyệt đối không có response body** (tuân thủ RFC 9110 Section 15.3.5).
- Các Header vẫn được gửi: `Content-Language`, `X-Request-Id`.

---

### 4.3. Dual Pagination Response Contracts

#### A. Offset-based Pagination (`PaginatedOffsetResponse<T>`)

Phù hợp cho các màn hình quản trị (Admin Portal), nơi người dùng cần chuyển trực tiếp đến một trang bất kỳ (Page 1, 2, 3...) hoặc biết tổng số trang.

```json
{
    "success": true,
    "code": "OK",
    "message": "Truy vấn danh sách đơn hàng thành công.",
    "data": [
        {
            "id": "ord_01HZX89AB",
            "total_amount": 250000.0,
            "currency": "VND"
        }
    ],
    "meta": {
        "request_id": "01a185e0-c938-73d1-ba0a-38a152c436de",
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "timestamp": "2026-10-29T08:30:00.125Z",
        "locale": "vi-VN",
        "pagination": {
            "mode": "offset",
            "page": 1,
            "page_size": 20,
            "total_items": 142,
            "total_pages": 8,
            "has_next": true,
            "has_prev": false
        }
    }
}
```

#### B. Cursor-based / Keyset Pagination (`PaginatedCursorResponse<T>`)

Bắt buộc sử dụng cho các tập dữ liệu lớn, bảng log, audit trail, feed dữ liệu hoặc ứng dụng Mobile (Infinite Scroll) nhằm loại trừ hoàn toàn chi phí quét $O(N)$ và tránh trùng lặp dữ liệu khi có ghi mới liên tục.

```json
{
    "success": true,
    "code": "OK",
    "message": "Truy vấn dòng sự kiện kiểm toán thành công.",
    "data": [
        {
            "id": "evt_01JAZZ8001",
            "action": "USER_LOGIN",
            "actor": "usr_01HZX87Q1K7",
            "created_at": "2026-10-29T08:35:10.000Z"
        }
    ],
    "meta": {
        "request_id": "01a185e1-01a1-7570-a6e1-d43de9dd7df6",
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "timestamp": "2026-10-29T08:30:00.125Z",
        "locale": "vi-VN",
        "pagination": {
            "mode": "cursor",
            "next_cursor": "ZXhwb3J0X2lkOjAxSkFaWjgwMDE=",
            "prev_cursor": null,
            "has_more": true,
            "limit": 50
        }
    }
}
```

---

### 4.4. RFC 9457 Compliant Error Contract (`ApiErrorResponse`)

Mọi lỗi trả về (4xx, 5xx) phải tuân thủ chuẩn **RFC 9457 (Problem Details for HTTP APIs)**, kèm theo Content-Type: `application/problem+json`.

```json
{
    "type": "https://errors.omnikubernetes.com/validation-error",
    "title": "Dữ liệu yêu cầu không hợp lệ",
    "status": 422,
    "detail": "Có 2 trường dữ liệu không vượt qua kiểm tra ràng buộc nghiệp vụ.",
    "instance": "/api/v1/orders",
    "code": "ERR_VALIDATION_FAILED",
    "invalid_params": [
        {
            "name": "items.0.quantity",
            "reason": "Số lượng sản phẩm trong đơn hàng phải lớn hơn 0.",
            "code": "NUMBER_MIN_VALUE",
            "params": {
                "min": 1,
                "actual": 0
            }
        },
        {
            "name": "shipping_address.postal_code",
            "reason": "Mã bưu chính không đúng định dạng.",
            "code": "INVALID_POSTAL_CODE",
            "params": {
                "pattern": "^[0-9]{5,6}$"
            }
        }
    ],
    "meta": {
        "request_id": "01a185e2-2fed-7781-8100-1b8e31a0eee5",
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "timestamp": "2026-10-29T08:30:00.125Z",
        "locale": "vi-VN"
    }
}
```

#### Quy tắc các trường trong RFC 9457:

1. **Trường chuẩn theo RFC 9457:**
    - `type` (URI, bắt buộc): URI định danh duy nhất cho loại lỗi.
    - `title` (String, bắt buộc): Tóm tắt ngắn gọn loại lỗi theo ngôn ngữ yêu cầu.
    - `status` (u16, bắt buộc): Mã trạng thái HTTP (`400`, `401`, `403`, `404`, `409`, `422`, `500`...).
    - `detail` (String, bắt buộc): Chi tiết nguyên nhân lỗi phát sinh lần này.
    - `instance` (URI Reference, bắt buộc): Endpoint URI phát sinh lỗi.
2. **RFC 9457 Extension Members (Mở rộng an toàn theo chuẩn RFC):**
    - `code` (String): Mã lỗi máy đọc (Machine-readable Domain Error Code) giúp code client bắt lỗi không phụ thuộc vào message.
    - `invalid_params` (Array): Danh sách lỗi chi tiết từng tham số (RFC 9457 Appendix A.1).
        - `name` (String, bắt buộc): Đường dẫn định danh trường dữ liệu vi phạm chuẩn hóa theo **Dot Notation**:
            - **Trường cấp gốc (Root level):** Định dạng `snake_case` (ví dụ: `email`, `role`, `password`).
            - **Đối tượng lồng nhau (Nested Objects):** Phân tách bằng dấu chấm `.` (ví dụ: `shipping_address.postal_code`, `user.profile.display_name`).
            - **Danh sách mảng (Arrays / Collections):** Phân tách bằng chỉ mục số 0-based index `array.index.field` (ví dụ: `items.0.quantity`, `recipients.1.phone_number`).
            - **Ánh xạ chuẩn RFC 6901 (JSON Pointer):** Đường dẫn Dot Notation `items.0.quantity` tương ứng 1-1 với JSON Pointer `/items/0/quantity`. Quy chuẩn này tương thích tuyệt đối với các form state binder trên Frontend (React Hook Form `setError('items.0.quantity', ...)`, Formik, VeeValidate) và Mobile (Flutter/SwiftUI), cho phép xây dựng các hàm generic error-binder tự động gán thông báo lỗi vào đúng input component của form.
        - `reason`: Thông điệp bản địa hóa sẵn từ Server.
        - `code`: Mã lỗi cụ thể của trường đó.
        - `rejected_value` (Tùy chọn - **Secure-by-default Policy**):
            - **Không echo dữ liệu PII và Free-form text:** Tuyệt đối không phản hồi lại dữ liệu người dùng nhập cho các trường thông tin định danh cá nhân (PII như CCCD/CMND, số điện thoại, email, địa chỉ, tài khoản ngân hàng, OTP...) và các chuỗi nhập tự do nhằm triệt tiêu nguy cơ **rò rỉ dữ liệu cá nhân (PII Leak)** trong gateway/access logs và nguy cơ tấn công **Reflected XSS / Content Injection** khi Client render lên UI.
            - **Chỉ áp dụng cho Non-PII Enums / Scalar Constants:** Chỉ cho phép xuất hiện đối với các giá trị enum hữu hạn hoặc hằng số hệ thống không hợp lệ (ví dụ: `role: "SUPER_ADMIN"`). Mọi trường nhạy cảm hoặc chuỗi tự do phải được **bỏ qua (`None`)** và ẩn hoàn toàn khỏi response (`skip_serializing_if = "Option::is_none"`).
        - `params`: **Đối tượng JSON động chứa các biến nội suy an toàn** (`min`, `max`, `count`, `allowed`...). Đây là điểm quyết định cho phép Native Mobile Apps hoặc SPA tự thực hiện localization ở Client nếu muốn mà không cần phụ thuộc vào việc echo dữ liệu vi phạm.
    - `meta`: Metadata truy vết đồng bộ (`request_id`, `trace_id`).

---

## 5. Internationalization (i18n) & Localization (l10n) Architecture

### 5.1. Locale Resolution Waterfall (Quy trình xác định ngôn ngữ)

Hệ thống xác định ngôn ngữ theo thứ tự ưu tiên giảm dần một cách chặt chẽ và không gây nghẽn I/O:

```text
Request Đến Gateway / Ingress
   │
   ▼
[1] Query Parameter (?lang=vi-VN hoặc ?locale=vi)
   │ (Ưu tiên cao nhất cho Webhook test, link chia sẻ, Preview)
   ├───> Có ───> Khớp danh sách hỗ trợ? ───> Áp dụng
   ▼ Không
[2] Custom Client Header (X-Locale: vi-VN)
   │ (Dành cho Mobile App / Web Client chủ động cấu hình chuyển ngôn ngữ độc lập với OS)
   ├───> Có ───> Khớp danh sách hỗ trợ? ───> Áp dụng
   ▼ Không
[3] Authenticated Identity Claims (JWT Claim: 'locale')
   │ (Ngôn ngữ người dùng chủ động cấu hình trong User Profile/Settings - Zero Database I/O)
   ├───> Có ───> Khớp danh sách hỗ trợ? ───> Áp dụng
   ▼ Không
[4] Standard HTTP Header (Accept-Language: vi,en-US;q=0.9)
   │ (Chuẩn RFC 9110 & BCP 47 Subtag Matching theo OS/Trình duyệt cho khách vãng lai)
   │ (Ví dụ: 'vi' -> 'vi-VN', 'en-GB' -> 'en-US')
   ├───> Có ───> Tìm match tốt nhất? ───> Áp dụng
   ▼ Không
[5] System Fallback Default
   │
   └───> Fallback: vi-VN (cấu hình qua app config)
```

Mọi response trả về cho client **bắt buộc kèm theo header**:

```http
Content-Language: vi-VN
```

### 5.2. Language Subtag Canonicalization & Matching

Thay vì so khớp chuỗi thô sơ (`find(|l| supported.contains(l))`), hệ thống áp dụng cơ chế **BCP 47 Prefix Matching**:

- Người dùng gửi `Accept-Language: vi`: Tự động map về `vi-VN`.
- Người dùng gửi `Accept-Language: en` hoặc `en-GB`: Tự động map về fallback của họ tiếng Anh là `en-US`.
- Tránh việc người dùng gửi mã ngôn ngữ hợp lệ nhưng server lại trả về fallback mặc định vì thiếu mã vùng.

### 5.3. Mozilla Fluent Engine (`fluent-bundle`)

Fluent được thiết kế bởi Mozilla cho các hệ thống phức tạp, cung cấp khả năng bất đối xứng giữa các ngôn ngữ (tiếng Việt không biến đổi số nhiều, nhưng tiếng Anh chia số ít/số nhiều, tiếng Ả Rập chia 6 dạng số).

#### Cấu trúc tài nguyên Fluent:

```text
libs/common/locales/
├── en-US/
│   ├── common.ftl
│   ├── errors.ftl
│   └── messages.ftl
└── vi-VN/
    ├── common.ftl
    ├── errors.ftl
    └── messages.ftl
```

#### Quy chuẩn Hot-Reloading & Concurrency Pattern (`arc-swap`):

Do `Arc<T>` trong Rust chỉ cung cấp tham chiếu chia sẻ bất biến (immutable shared reference), không thể tự động thay thế hay mutate dữ liệu bên trong khi có yêu cầu tải lại tài nguyên dịch mà không dùng cơ chế đồng bộ hóa. Đồng thời, việc sử dụng `RwLock` thông thường sẽ gây ra hiện tượng _lock contention_ và block các worker thread khi xảy ra thao tác ghi (reload).

Hệ thống đặc tả cơ chế quản lý đa ngôn ngữ lock-free bằng mô hình **RCU (Read-Copy-Update)** dựa trên `Arc<ArcSwap<HashMap<String, FluentBundle<FluentResource>>>>` (từ crate `arc-swap`):

```rust
use arc_swap::ArcSwap;
use fluent_bundle::{FluentBundle, FluentResource};
use std::collections::HashMap;
use std::sync::Arc;

pub type LocaleBundles = HashMap<String, FluentBundle<FluentResource>>;

/// Quản lý đa ngôn ngữ hỗ trợ Hot-Reload lock-free
#[derive(Clone)]
pub struct I18nManager {
    bundles: Arc<ArcSwap<LocaleBundles>>,
}
```

- **Đọc bản dịch (Hot Path - Lock-free Read):**
    - Worker threads gọi `bundles.load()` hoặc `bundles.load_full()`. Đây là atomic load zero-cost (không mutex, không rwlock overhead, không context switch), đảm bảo thời gian tra cứu $O(1)$ ngay cả khi hệ thống chịu tải hàng trăm nghìn requests/giây.
- **Quy trình Hot-Reload (Reload Path - Zero Downtime):**
    1. Khi nhận tín hiệu cập nhật, luồng background tiến hành đọc các file `.ftl` mới và parse sang một `HashMap` tạm (`next_bundles`) trên bộ nhớ độc lập.
    2. Thực hiện validate tính toàn vẹn cú pháp của toàn bộ file dịch mới. Nếu có lỗi parse hoặc thiếu bundle quan trọng, hủy bỏ bản cập nhật, ghi log cảnh báo và giữ nguyên trạng thái cũ (không làm sập Pod hay trả thông điệp hỏng cho người dùng).
    3. Nếu parse thành công, thực hiện `bundles.store(Arc::new(next_bundles))` (hoặc `swap()`) trong $O(1)$ thông qua con trỏ atomic.
    4. Các worker thread đang xử lý dở dang request trước thời điểm swap vẫn giữ tham chiếu an toàn đến `Arc<LocaleBundles>` cũ cho đến khi hoàn thành request, sau đó bộ nhớ cũ tự động được giải phóng an toàn (RCU semantics).
- **Cơ chế kích hoạt Reload:**
    - **HTTP Endpoint:** `/internal/v1/reload-locales` (bảo vệ bởi RBAC / NetworkPolicy nội bộ).
    - **Signal Handling:** Lắng nghe tín hiệu `SIGHUP` (qua `tokio::signal::unix::signal(SignalKind::hangup())`) được phát ra tự động khi Kubernetes ConfigMap cập nhật file `.ftl` mount vào Pod.

---

## 6. Rust Implementation Design Specification

### 6.1. Core Data Structures (`libs/common/src/response.rs`)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Metadata đính kèm cho mọi HTTP Response (Không chứa duration_ms để đảm bảo Zero-Cost Abstraction)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Định danh request duy nhất (100% requests) phục vụ log correlation
    pub request_id: Cow<'static, str>,
    /// W3C Distributed Trace ID (32 hex) kết nối với OpenTelemetry / APM
    pub trace_id: Cow<'static, str>,
    /// Thời điểm sinh response theo chuẩn ISO 8601 (RFC 3339) UTC
    pub timestamp: DateTime<Utc>,
    /// BCP-47 Locale thực tế được áp dụng cho response
    pub locale: Cow<'static, str>,
}

/// Phân trang dạng Offset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetPaginationMeta {
    pub page: u64,
    pub page_size: u64,
    pub total_items: u64,
    pub total_pages: u64,
    pub has_next: bool,
    pub has_prev: bool,
}

/// Phân trang dạng Cursor / Keyset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPaginationMeta {
    pub next_cursor: Option<Cow<'static, str>>,
    pub prev_cursor: Option<Cow<'static, str>>,
    pub has_more: bool,
    pub limit: u32,
}

/// Metadata phân trang hỗ trợ cả hai mô hình (Internally Tagged Enum)
/// Sử dụng `#[serde(tag = "mode", rename_all = "snake_case")]` để Serde tự động quản lý trường
/// discriminator `"mode": "offset" | "cursor"`, đảm bảo parsing O(1), type-safety tại compile-time
/// và loại bỏ hoàn toàn anti-pattern `untagged` trial-deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PaginationMeta {
    Offset(OffsetPaginationMeta),
    Cursor(CursorPaginationMeta),
}

/// Base Success Response Envelope sử dụng Cow<'static, str> triệt tiêu heap allocation cho các chuỗi tĩnh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub code: Cow<'static, str>,
    pub message: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub meta: ResponseMeta,
}

/// Base Success Response Envelope kèm phân trang
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub success: bool,
    pub code: Cow<'static, str>,
    pub message: Cow<'static, str>,
    pub data: Vec<T>,
    pub meta: PaginatedResponseMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponseMeta {
    #[serde(flatten)]
    pub base: ResponseMeta,
    pub pagination: PaginationMeta,
}
```

---

### 6.2. RFC 9457 Problem Details Struct (`libs/errors/src/problem.rs`)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

use crate::response::ResponseMeta;

/// Chi tiết vi phạm từng tham số theo RFC 9457 Appendix A.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidParamError {
    /// Đường dẫn trường vi phạm chuẩn Dot Notation (ví dụ: "email", "items.0.quantity", "shipping_address.city")
    pub name: Cow<'static, str>,
    /// Mô tả lỗi bằng ngôn ngữ yêu cầu (Server-rendered message)
    pub reason: Cow<'static, str>,
    /// Mã lỗi nghiệp vụ của tham số
    pub code: Cow<'static, str>,
    /// Giá trị vi phạm (Chỉ dùng cho non-PII enum/scalar; mặc định None để chống PII leak & Reflected XSS)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_value: Option<serde_json::Value>,
    /// Tham số nội suy động an toàn phục vụ localization (min, max, count, allowed...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// Response cấu trúc lỗi theo tiêu chuẩn RFC 9457 với Zero-Cost Static String Borrowing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    /// URI nhận diện loại lỗi (RFC 9457)
    #[serde(rename = "type")]
    pub error_type: Cow<'static, str>,
    /// Tóm tắt ngắn gọn loại lỗi
    pub title: Cow<'static, str>,
    /// Mã HTTP status code (Single Source of Truth)
    pub status: u16,
    /// Mô tả chi tiết nguyên nhân lỗi phát sinh
    pub detail: Cow<'static, str>,
    /// Endpoint URI phát sinh lỗi
    pub instance: Cow<'static, str>,
    /// Mã lỗi máy đọc định danh nghiệp vụ
    pub code: Cow<'static, str>,
    /// Danh sách lỗi chi tiết từng tham số (nếu có)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_params: Option<Vec<InvalidParamError>>,
    /// Metadata truy vết đồng bộ
    pub meta: ResponseMeta,
}
```

---

### 6.3. Axum `IntoResponse` Implementation & Protocol Compliance

```rust
use axum::{
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

static PROBLEM_JSON_MIME: HeaderValue = HeaderValue::from_static("application/problem+json");

impl<T: serde::Serialize> ApiResponse<T> {
    pub fn ok(
        data: T,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<ApiResponse<T>>) {
        let mut headers = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&meta.locale) {
            headers.insert(header::CONTENT_LANGUAGE, val);
        }
        if let Ok(val) = HeaderValue::from_str(&meta.request_id) {
            headers.insert(HeaderName::from_static("x-request-id"), val);
        }

        let body = ApiResponse {
            success: true,
            code: Cow::Borrowed("OK"),
            message: message.into(),
            data: Some(data),
            meta,
        };

        (StatusCode::OK, headers, Json(body))
    }

    pub fn created(
        data: T,
        location_uri: &str,
        message: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
    ) -> (StatusCode, HeaderMap, Json<ApiResponse<T>>) {
        let (status, mut headers, json_body) = Self::ok(data, message, meta);
        if let Ok(val) = HeaderValue::from_str(location_uri) {
            headers.insert(header::LOCATION, val);
        }
        (StatusCode::CREATED, headers, json_body)
    }
}

/// Helper cho HTTP 204 No Content (Tuyệt đối không có response body)
pub fn no_content(request_id: &str, locale: &str) -> (StatusCode, HeaderMap) {
    let mut headers = HeaderMap::new();
    if let Ok(val) = HeaderValue::from_str(locale) {
        headers.insert(header::CONTENT_LANGUAGE, val);
    }
    if let Ok(val) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static("x-request-id"), val);
    }
    (StatusCode::NO_CONTENT, headers)
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, PROBLEM_JSON_MIME.clone());

        if let Ok(val) = HeaderValue::from_str(&self.meta.locale) {
            headers.insert(header::CONTENT_LANGUAGE, val);
        }
        if let Ok(val) = HeaderValue::from_str(&self.meta.request_id) {
            headers.insert(HeaderName::from_static("x-request-id"), val);
        }

        (status, headers, Json(self)).into_response()
    }
}
```

---

### 6.4. Canonical Locale Resolution Extractor

```rust
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};
use std::collections::HashMap;

pub const SUPPORTED_LOCALES: &[&str] = &["vi-VN", "en-US", "ja-JP"];
pub const DEFAULT_LOCALE: &str = "vi-VN";

#[derive(Debug, Clone)]
pub struct RequestLocale(pub Cow<'static, str>);

#[async_trait]
impl<S> FromRequestParts<S> for RequestLocale
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 1. Query Param (?lang= hoặc ?locale=)
        if let Some(query) = parts.uri.query() {
            if let Ok(params) = serde_urlencoded::from_str::<HashMap<String, String>>(query) {
                if let Some(lang) = params.get("lang").or_else(|| params.get("locale")) {
                    if let Some(matched) = match_canonical_locale(lang) {
                        return Ok(RequestLocale(Cow::Borrowed(matched)));
                    }
                }
            }
        }

        // 2. Custom Client Header (X-Locale)
        if let Some(val) = parts.headers.get("x-locale").and_then(|v| v.to_str().ok()) {
            if let Some(matched) = match_canonical_locale(val) {
                return Ok(RequestLocale(Cow::Borrowed(matched)));
            }
        }

        // 3. Token User Claims (ngôn ngữ lưu trong User Profile/Settings từ JWT token - 0 I/O)
        // if let Some(user_claims) = parts.extensions.get::<AuthUserClaims>() {
        //     if let Some(locale) = &user_claims.locale {
        //         if let Some(matched) = match_canonical_locale(locale) {
        //             return Ok(RequestLocale(Cow::Borrowed(matched)));
        //         }
        //     }
        // }

        // 4. HTTP Header (Accept-Language từ trình duyệt/OS cho khách vãng lai chưa đăng nhập)
        if let Some(accept_str) = parts.headers.get(axum::http::header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()) {
            let candidates = accept_language::parse(accept_str);
            for candidate in candidates {
                if let Some(matched) = match_canonical_locale(&candidate) {
                    return Ok(RequestLocale(Cow::Borrowed(matched)));
                }
            }
        }

        // 5. Fallback Default
        Ok(RequestLocale(Cow::Borrowed(DEFAULT_LOCALE)))
    }
}

/// Canonicalization logic hỗ trợ BCP-47 Language Prefix với Zero-Allocation (0 Heap Allocations)
fn match_canonical_locale(input: &str) -> Option<&'static str> {
    let input = input.trim();
    for &supported in SUPPORTED_LOCALES {
        // 1. So khớp chính xác không phân biệt hoa thường (0 heap allocations)
        if supported.eq_ignore_ascii_case(input) {
            return Some(supported);
        }

        // 2. So khớp Language Subtag prefix (ví dụ input 'vi' khớp 'vi-VN', 'en' khớp 'en-US')
        // Sử dụng split_once thay vì format!() để triệt tiêu hoàn toàn dynamic heap allocation
        if let Some((lang_prefix, _)) = supported.split_once('-') {
            if lang_prefix.eq_ignore_ascii_case(input) {
                return Some(supported);
            }
        }
    }
    None
}
```

### 6.5. W3C Server-Timing & Latency Middleware (`libs/common/src/middleware/timing.rs`)

Middleware đo lường chính xác toàn bộ chu kỳ xử lý (từ khi request vào service đến khi chuẩn bị ghi ra socket) và đính kèm trực tiếp vào HTTP Header với chi phí zero-copy:

```rust
use axum::{
    body::Body,
    http::{HeaderValue, Request, Response},
    middleware::Next,
};
use std::time::Instant;

/// Middleware đo lường và gắn header Server-Timing / X-Response-Time
pub async fn timing_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let start = Instant::now();
    let mut response = next.run(req).await;
    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_secs_f64() * 1000.0;

    // 1. Chuẩn W3C Server-Timing: total;dur=12.40
    let server_timing = format!("total;dur={:.2}", duration_ms);
    if let Ok(val) = HeaderValue::from_str(&server_timing) {
        response.headers_mut().insert("server-timing", val);
    }

    // 2. Chuẩn truyền thống X-Response-Time: 12.40ms
    let x_response_time = format!("{:.2}ms", duration_ms);
    if let Ok(val) = HeaderValue::from_str(&x_response_time) {
        response.headers_mut().insert("x-response-time", val);
    }

    response
}
```

---

## 7. Distributed Tracing & Kubernetes Integration

```text
Client (Web / Mobile / Third-Party)
   │
   │  HTTPS Request (Accept-Language, X-Locale, traceparent)
   ▼
[ Envoy Gateway / Ingress Controller ]
   │  1. Kiểm tra header 'X-Request-Id', nếu chưa có thì tự sinh UUIDv7 mới.
   │  2. Kiểm tra W3C header 'traceparent', nếu chưa có thì sinh mới.
   │  3. Gắn access-log với cả 'request_id' và 'trace_id'.
   ▼
[ OmniKubernetes Microservice (Axum Pod) ]
   │  1. Trích xuất RequestContext (request_id, trace_id, RequestLocale).
   │  2. Mở rộng tracing span hiện tại: tracing::Span::current().record("request_id", &req_id).
   │  3. Xử lý nghiệp vụ logic.
   │  4. Đóng gói response:
   │     - Success -> ApiResponse<T>
   │     - Error   -> ApiErrorResponse (RFC 9457)
   │     - Header  -> Content-Language, Content-Type, X-Request-Id, Server-Timing, X-Response-Time
   ▼
[ Logging & Observability Pipeline ]
   ├──> Vector / Fluentbit -> Grafana Loki (Tra cứu log theo 'request_id')
   └──> OpenTelemetry Collector -> Grafana Tempo / Jaeger (Truy vết Span qua 'trace_id')
```

---

## 8. Security & Production Hardening

1. **Information Leakage Prevention (Ngăn rò rỉ thông tin hạ tầng):**
    - Trong môi trường `production`, mã lỗi `500 Internal Server Error` **tuyệt đối không** được đưa raw SQL error, rust panic trace, hay connection strings vào `detail`.
    - Mọi exception hệ thống chưa xử lý bắt buộc phải map về thông điệp chung:
        ```ftl
        err_internal_server = Hệ thống đang gặp gián đoạn tạm thời. Vui lòng thử lại sau.
        ```
    - Log chi tiết nguyên nhân gốc kèm `trace_id` chỉ được ghi ở tầng log nội bộ (`tracing::error!`). Khách hàng chỉ nhận được `request_id` để cung cấp cho đội ngũ hỗ trợ kỹ thuật.
2. **Bảo mật thông tin cá nhân (PII) & Phòng chống Reflected XSS trong `rejected_value`:**
    - **Ngăn chặn rò rỉ thông tin định danh cá nhân (PII Leak):**
        - Tuyệt đối không echo lại các trường dữ liệu cá nhân (CCCD/CMND, số hộ chiếu, số điện thoại, email, địa chỉ cư trú, số tài khoản ngân hàng, thẻ tín dụng/CVV, mật khẩu, mã OTP, token...) vào `rejected_value`.
        - Việc echo dữ liệu PII trong HTTP response body khiến dữ liệu nhạy cảm bị ghi lại vào Ingress/Envoy Access Logs, APM traces (Tempo/Jaeger), WAF logs và browser cache, vi phạm trực tiếp các quy định pháp lý về bảo vệ dữ liệu cá nhân (Nghị định 13/2023/NĐ-CP, GDPR).
    - **Triệt tiêu nguy cơ Reflected XSS & Content Injection:**
        - Kẻ tấn công có thể chèn các chuỗi mã độc (ví dụ `<script>alert(1)</script>`, payload SVG, javascript pseudo-protocol) vào form input để server phản chiếu (echo) lại trong `rejected_value`. Nếu Frontend Web (SPA) hoặc WebView hiển thị chuỗi này mà không escape kỹ lưỡng (như dùng `v-html` hoặc `dangerouslySetInnerHTML`), mã độc sẽ thực thi trực tiếp trên phiên người dùng.
    - **Quy tắc thực thi (Secure-by-Default Enforcement):**
        - Thuộc tính `rejected_value` được cấu hình là `Option<Value>` với `#[serde(skip_serializing_if = "Option::is_none")]`.
        - **Mặc định là `None` (omitted)** cho toàn bộ các trường chuỗi tự do (free-form text) và toàn bộ dữ liệu người dùng cá nhân.
        - Chỉ kích hoạt `rejected_value` cho các giá trị enum hệ thống hữu hạn (ví dụ `user_role`, `order_status`, `payment_method`).
        - Thay vì echo lại chuỗi vi phạm, cung cấp các metadata định lượng an toàn trong `params` (ví dụ: `{"min": 8, "actual": 4}` hoặc `{"allowed": ["MEMBER", "ADMIN"]}`).
3. **Deterministic UTC Timestamps:**
    - Toàn bộ mốc thời gian đều sinh qua `chrono::Utc::now()` và serialize thành định dạng chuẩn RFC 3339 với độ chính xác mili-giây (`YYYY-MM-DDTHH:mm:ss.sssZ`).
4. **Resilient Memory Lookup & Hot-Reload:**
    - Tránh việc đọc đĩa (Disk I/O) hoặc truy vấn Database để nạp file i18n trong mỗi request. Fluent bundles phải được tải hoàn toàn vào RAM khi khởi động Pod và quản lý qua con trỏ nguyên tử `ArcSwap` để hot-reload lock-free, không gián đoạn luồng xử lý.

---

## 9. Workspace Organization in OmniKubernetes

```text
OmniKubernetes/
├── libs/
│   ├── common/
│   │   ├── Cargo.toml
│   │   ├── locales/               <-- File tài nguyên bản dịch Fluent (.ftl)
│   │   │   ├── en-US/
│   │   │   │   ├── common.ftl
│   │   │   │   └── errors.ftl
│   │   │   └── vi-VN/
│   │   │       ├── common.ftl
│   │   │       └── errors.ftl
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── response.rs        <-- ApiResponse<T>, Pagination, ResponseMeta
│   │       ├── i18n/              <-- Locale matcher, Fluent bundle manager
│   │       └── middleware/        <-- RequestContext, Traceparent extractor
│   └── errors/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── problem.rs         <-- RFC 9457 ApiErrorResponse, InvalidParamError
│           └── codes.rs           <-- Domain error codes constants
└── docs/
    └── architecture/
        └── base-response-architecture.md  <-- Tài liệu đặc tả chuẩn này
```
