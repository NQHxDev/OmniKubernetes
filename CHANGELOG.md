# Changelog

Tất cả những thay đổi đáng chú ý của dự án **OmniKubernetes** sẽ được ghi chép tại file này.

Định dạng tài liệu này tuân thủ theo chuẩn [Keep a Changelog](https://keepachangelog.com/vi/1.0.0/)
và phiên bản tuân theo [Semantic Versioning (SemVer)](https://semver.org/lang/vi/).

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
- Triển khai thư viện lõi `omni-common` (`libs/common`):
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
- Bổ sung bộ kiểm thử đơn vị và tích hợp (`response_and_i18n_tests.rs`, `problem_details_tests.rs`) cho `omni-common` và `omni-errors`.

### Changed

- Cập nhật `Cargo.toml` cấp workspace, `libs/common/Cargo.toml` và `libs/errors/Cargo.toml` để tích hợp các thư viện đa ngôn ngữ (`fluent-bundle`, `intl-memoizer`, `unic-langid`) và liên kết phụ thuộc nội bộ.
- Bổ sung các khóa thông điệp kiểm tra ràng buộc dữ liệu (`NUMBER_MIN_VALUE`, `INVALID_POSTAL_CODE`) vào bộ tài nguyên Fluent `locales/{vi-VN,en-US,ja-JP}/errors.ftl`.

### Security

- **Bảo vệ dữ liệu cá nhân (PII) & Phòng chống Reflected XSS (`InvalidParamError`):** Thực thi chính sách *Secure-by-Default* ẩn trường `rejected_value` (`None`) khỏi JSON response đối với toàn bộ chuỗi nhập tự do và dữ liệu PII; chỉ cho phép phản hồi các giá trị hằng số enum hệ thống không chứa PII thông qua `with_rejected_enum_value`.
- **Ngăn chặn rò rỉ thông tin hạ tầng (`ApiErrorResponse::internal_server_error` & `AppError::Internal`):** Tự động lọc bỏ chi tiết lỗi nhạy cảm (truy vấn SQL, thông tin kết nối cơ sở dữ liệu, stack trace) khỏi payload HTTP `500 Internal Server Error` trả về cho Client, chỉ ghi log nội bộ qua `tracing::error!` kèm `request_id` và `trace_id`.

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
