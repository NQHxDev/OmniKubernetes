//! Gateway Signed Context protocol (HMAC-SHA256) for zero-trust internal service mesh.

use hmac::{Hmac, KeyInit, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

use crate::error::AuthError;

type HmacSha256 = Hmac<Sha256>;

pub const HEADER_USER_ID: &str = "x-user-id";
pub const HEADER_TENANT_ID: &str = "x-tenant-id";
pub const HEADER_USER_ROLES: &str = "x-user-roles";
pub const HEADER_OMNI_TIMESTAMP: &str = "x-omni-timestamp";
pub const HEADER_OMNI_SIGNATURE: &str = "x-omni-signature";

/// Ngữ cảnh chữ ký số Gateway cho internal service mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayContext<'a> {
    /// Định danh người dùng UUID string.
    pub user_id: &'a str,
    /// Định danh Tenant UUID string (nếu có).
    pub tenant_id: Option<&'a str>,
    /// Danh sách vai trò phân cách bằng dấu phẩy.
    pub roles: &'a str,
    /// Timestamp Unix (giây) khi ký context.
    pub timestamp_secs: i64,
}

impl<'a> GatewayContext<'a> {
    /// Khởi tạo [`GatewayContext`] mới.
    #[must_use]
    pub fn new(
        user_id: &'a str,
        tenant_id: Option<&'a str>,
        roles: &'a str,
        timestamp_secs: i64,
    ) -> Self {
        Self {
            user_id,
            tenant_id,
            roles,
            timestamp_secs,
        }
    }

    /// Định dạng canonical payload phục vụ tính toán chữ ký HMAC: `{user_id}:{tenant_id}:{roles}:{timestamp_secs}`.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.user_id,
            self.tenant_id.unwrap_or(""),
            self.roles,
            self.timestamp_secs
        )
    }
}

/// Ký gói ngữ cảnh định danh tại API Gateway trước khi chuyển tiếp vào downstream pod.
pub fn sign_gateway_context(
    secret: &SecretString,
    ctx: &GatewayContext<'_>,
) -> Result<String, AuthError> {
    let canonical = ctx.canonical();

    let mut mac = HmacSha256::new_from_slice(secret.expose_secret().as_bytes())
        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
    mac.update(canonical.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(hex::encode(result))
}

/// Thẩm định chữ ký số HMAC của Gateway tại Downstream Microservice.
///
/// Áp dụng Freshness Check chống Replay Attack và phép so sánh thời gian đẳng thời (Constant-Time).
pub fn verify_gateway_signature(
    secret: &SecretString,
    ctx: &GatewayContext<'_>,
    current_time_secs: i64,
    max_age_secs: i64,
    provided_signature_hex: &str,
) -> Result<bool, AuthError> {
    // 1. Freshness check chống Replay Attack
    if (current_time_secs - ctx.timestamp_secs).abs() > max_age_secs {
        return Err(AuthError::SignatureExpired);
    }

    // 2. Tái tạo canonical payload
    let canonical = ctx.canonical();

    // 3. Tính toán và so sánh Constant-Time
    let mut mac = HmacSha256::new_from_slice(secret.expose_secret().as_bytes())
        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
    mac.update(canonical.as_bytes());

    let provided_bytes =
        hex::decode(provided_signature_hex).map_err(|_| AuthError::InvalidSignatureFormat)?;

    match mac.verify_slice(&provided_bytes) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}
