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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_context_canonical() {
        let ctx_with_tenant =
            GatewayContext::new("usr-123", Some("tnt-456"), "admin,user", 1_700_000_000);
        assert_eq!(
            ctx_with_tenant.canonical(),
            "usr-123:tnt-456:admin,user:1700000000"
        );

        let ctx_no_tenant = GatewayContext::new("usr-123", None, "admin", 1_700_000_000);
        assert_eq!(ctx_no_tenant.canonical(), "usr-123::admin:1700000000");
    }

    #[test]
    fn test_sign_and_verify_gateway_context_happy_path() {
        let secret = SecretString::from("gateway_internal_secret_key_1234567890");
        let ts = 1_700_000_000;
        let ctx = GatewayContext::new("usr-1", Some("tnt-1"), "operator", ts);

        let sig = sign_gateway_context(&secret, &ctx).expect("Sign should succeed");
        assert_ne!(sig, "");

        let is_valid = verify_gateway_signature(&secret, &ctx, ts + 5, 30, &sig)
            .expect("Verification should succeed");
        assert!(is_valid);
    }

    #[test]
    fn test_verify_gateway_signature_expired_timestamp() {
        let secret = SecretString::from("gateway_internal_secret_key_1234567890");
        let ts = 1_700_000_000;
        let ctx = GatewayContext::new("usr-1", None, "admin", ts);
        let sig = sign_gateway_context(&secret, &ctx).unwrap();

        // 31 seconds in future (> 30s max age)
        let res_past = verify_gateway_signature(&secret, &ctx, ts + 31, 30, &sig);
        assert!(matches!(res_past, Err(AuthError::SignatureExpired)));

        // 31 seconds in past (< -30s max age)
        let res_future = verify_gateway_signature(&secret, &ctx, ts - 31, 30, &sig);
        assert!(matches!(res_future, Err(AuthError::SignatureExpired)));
    }

    #[test]
    fn test_verify_gateway_signature_tampered_payload() {
        let secret = SecretString::from("gateway_internal_secret_key_1234567890");
        let ts = 1_700_000_000;
        let ctx = GatewayContext::new("usr-1", Some("tnt-1"), "user", ts);
        let sig = sign_gateway_context(&secret, &ctx).unwrap();

        // Tamper roles to "admin"
        let tampered_ctx = GatewayContext::new("usr-1", Some("tnt-1"), "admin", ts);
        let is_valid = verify_gateway_signature(&secret, &tampered_ctx, ts, 30, &sig).unwrap();
        assert!(!is_valid);

        // Tamper user_id
        let tampered_ctx2 = GatewayContext::new("usr-2", Some("tnt-1"), "user", ts);
        let is_valid2 = verify_gateway_signature(&secret, &tampered_ctx2, ts, 30, &sig).unwrap();
        assert!(!is_valid2);
    }

    #[test]
    fn test_verify_gateway_signature_tampered_secret() {
        let secret1 = SecretString::from("gateway_internal_secret_key_1234567890");
        let secret2 = SecretString::from("different_secret_key_0987654321");
        let ts = 1_700_000_000;
        let ctx = GatewayContext::new("usr-1", None, "user", ts);
        let sig = sign_gateway_context(&secret1, &ctx).unwrap();

        let is_valid = verify_gateway_signature(&secret2, &ctx, ts, 30, &sig).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_verify_gateway_signature_invalid_hex() {
        let secret = SecretString::from("gateway_internal_secret_key_1234567890");
        let ts = 1_700_000_000;
        let ctx = GatewayContext::new("usr-1", None, "user", ts);

        let res = verify_gateway_signature(&secret, &ctx, ts, 30, "not_valid_hex!@#$");
        assert!(matches!(res, Err(AuthError::InvalidSignatureFormat)));
    }
}
