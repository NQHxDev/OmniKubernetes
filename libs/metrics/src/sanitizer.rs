//! Low-cardinality URI path sanitization engine.
//!
//! Provides zero-regex, single-pass byte scanning to normalize raw request URI paths and Axum route templates,
//! protecting Prometheus from Cardinality Explosion.

use std::borrow::Cow;

/// Nhãn cố định dùng cho mọi request 404 không khớp với bất kỳ route nào đã đăng ký.
pub const UNMATCHED_ROUTE_LABEL: &str = "__unmatched__";

/// Giới hạn số lượng phân đoạn đường dẫn tối đa để ngăn chặn tấn công URI sâu bất thường.
const MAX_URI_SEGMENTS: usize = 8;

/// Chuẩn hóa mẫu route từ `axum::extract::MatchedPath` (ví dụ chuyển `/api/v1/orders/{id}` thành `/api/v1/orders/:id`).
#[must_use]
pub fn normalize_matched_path(matched_path: &str) -> Cow<'static, str> {
    if !matched_path.contains('{') {
        return Cow::Owned(matched_path.to_string());
    }

    let mut out = String::with_capacity(matched_path.len());
    for (idx, segment) in matched_path.split('/').enumerate() {
        if idx > 0 {
            out.push('/');
        }
        if let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let clean_name = inner.strip_prefix('*').unwrap_or(inner);
            out.push(':');
            out.push_str(if clean_name.is_empty() {
                "id"
            } else {
                clean_name
            });
        } else {
            out.push_str(segment);
        }
    }
    Cow::Owned(out)
}

/// Chuẩn hóa đường dẫn URI thô bằng thuật toán quét byte một lượt (Zero-Regex) để bảo vệ Prometheus khỏi Cardinality Explosion.
///
/// Chuyển đổi các tham số động như UUIDv4/v7, ULID, số nguyên, chuỗi Hex dài, hoặc ID có tiền tố (`usr_...`, `ord_...`)
/// về dạng chuẩn `:id`.
///
/// # Examples
/// ```
/// use omni_metrics::sanitizer::sanitize_uri_path;
///
/// assert_eq!(
///     sanitize_uri_path("/api/v1/orders/018f4a2b-8a5e-7a1b"),
///     "/api/v1/orders/:id"
/// );
/// assert_eq!(
///     sanitize_uri_path("/api/v2/users/usr_01HZX87Q1K7/invoices/987654"),
///     "/api/v2/users/:id/invoices/:id"
/// );
/// ```
#[must_use]
pub fn sanitize_uri_path(raw_path: &str) -> String {
    let path_only = raw_path.split('?').next().unwrap_or("/");
    let trimmed = path_only.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }

    let mut normalized = String::with_capacity(trimmed.len().min(128));

    for (segment_count, segment) in trimmed.split('/').filter(|s| !s.is_empty()).enumerate() {
        if segment_count >= MAX_URI_SEGMENTS {
            normalized.push_str("/:rest");
            break;
        }
        normalized.push('/');

        if let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let clean_name = inner.strip_prefix('*').unwrap_or(inner);
            normalized.push(':');
            normalized.push_str(if clean_name.is_empty() {
                "id"
            } else {
                clean_name
            });
        } else if segment.starts_with(':') && segment.len() > 1 {
            normalized.push_str(segment);
        } else if is_dynamic_identifier_segment(segment) {
            normalized.push_str(":id");
        } else {
            normalized.push_str(segment);
        }
    }

    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

/// Kiểm tra một phân đoạn đường dẫn có phải là định danh động (ID, UUID, ULID, Hex, Số nguyên, Token) hay không.
#[inline]
fn is_dynamic_identifier_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let len = bytes.len();

    if len == 0 {
        return false;
    }

    // 1. Giữ nguyên các tiền tố phiên bản API chuẩn (v1, v2, ..., v99)
    if (len == 2 || len == 3)
        && (bytes[0] == b'v' || bytes[0] == b'V')
        && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    // 2. Phân đoạn quá dài (> 32 ký tự, ví dụ JWT, SHA256, Base64 token)
    if len > 32 {
        return true;
    }

    // 3. Số nguyên thuần túy (ví dụ: /orders/123456)
    if bytes.iter().all(u8::is_ascii_digit) {
        return true;
    }

    // 4. UUID hoặc chuỗi Hex phân tách bằng dấu gạch ngang (ví dụ: 018f4a2b-8a5e-7a1b)
    let has_hyphen = bytes.contains(&b'-');
    if has_hyphen && len >= 8 && bytes.iter().all(|&b| b.is_ascii_hexdigit() || b == b'-') {
        return true;
    }

    // 5. Chuỗi Hex liên tục dài >= 16 ký tự (ví dụ: Trace ID, MongoDB ObjectId 24-hex)
    if len >= 16 && bytes.iter().all(u8::is_ascii_hexdigit) {
        return true;
    }

    // 6. Định danh có tiền tố nghiệp vụ (ví dụ: usr_01HZX87Q1K7, ord_98765, tx_a1b2c3d4)
    if let Some(underscore_pos) = segment.rfind('_') {
        let suffix = &bytes[underscore_pos + 1..];
        let suffix_has_digit = suffix.iter().any(u8::is_ascii_digit);
        if !suffix.is_empty() && (suffix_has_digit || suffix.len() >= 8) {
            return true;
        }
    }

    // 7. ULID (26 ký tự) hoặc chuỗi ngẫu nhiên kết hợp chữ và số dài >= 10 ký tự với ít nhất 2 chữ số
    let digit_count = bytes.iter().filter(|&&b| b.is_ascii_digit()).count();
    if len >= 10 && digit_count >= 2 && bytes.iter().all(u8::is_ascii_alphanumeric) {
        return true;
    }

    false
}
