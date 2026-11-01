//! Zero-leak secret management and memory zeroization helpers.

pub use secrecy::{ExposeSecret, SecretString};
use serde::Serializer;
use validator::ValidationError;

/// Kiểm tra một [`SecretString`] không được để trống hoặc chỉ chứa khoảng trắng.
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
            format!(
                "Secret length ({actual_len} bytes) is shorter than required minimum of {min_bytes} bytes"
            )
            .into(),
        );
        return Err(err);
    }
    Ok(())
}

/// Bộ tuần tự hóa (serializer) che giấu giá trị thật của [`SecretString`] thành `"[REDACTED]"`.
pub fn serialize_redacted_secret<S>(_: &SecretString, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str("[REDACTED]")
}

/// Bộ tuần tự hóa (serializer) che giấu giá trị thật của `Option<SecretString>` thành `"[REDACTED]"`.
pub fn serialize_opt_redacted_secret<S>(
    val: &Option<SecretString>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match val {
        Some(_) => serializer.serialize_some("[REDACTED]"),
        None => serializer.serialize_none(),
    }
}

/// Thay thế mật khẩu trong chuỗi URL kết nối hạ tầng thành `[REDACTED]`.
#[must_use]
pub fn redact_connection_url(raw_url: &str) -> String {
    if let Some(scheme_idx) = raw_url.find("://") {
        let scheme = &raw_url[..scheme_idx + 3];
        let rest = &raw_url[scheme_idx + 3..];
        if let Some(at_idx) = rest.find('@') {
            let auth = &rest[..at_idx];
            let host_part = &rest[at_idx..];
            if let Some(colon_idx) = auth.find(':') {
                let user = &auth[..colon_idx];
                return format!("{scheme}{user}:[REDACTED]{host_part}");
            }
            return format!("{scheme}:[REDACTED]{host_part}");
        }
    }
    raw_url.to_string()
}
