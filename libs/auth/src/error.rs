//! Authentication and authorization error taxonomy and RFC 9457 Problem Details integration.

use omni_common::{i18n::I18nManager, response::ResponseMeta};
use omni_errors::problem::{ApiErrorResponse, ErrorCodeExtension};
use std::borrow::Cow;
use thiserror::Error;

/// Business error codes for authentication and authorization.
pub const ERR_AUTH_MISSING_CREDENTIALS: &str = "ERR_AUTH_MISSING_CREDENTIALS";
pub const ERR_AUTH_TOKEN_EXPIRED: &str = "ERR_AUTH_TOKEN_EXPIRED";
pub const ERR_AUTH_TOKEN_REVOKED: &str = "ERR_AUTH_TOKEN_REVOKED";
pub const ERR_AUTH_INVALID_SIGNATURE: &str = "ERR_AUTH_INVALID_SIGNATURE";
pub const ERR_AUTH_MALFORMED_TOKEN: &str = "ERR_AUTH_MALFORMED_TOKEN";
pub const ERR_AUTH_INVALID_CREDENTIALS: &str = "ERR_AUTH_INVALID_CREDENTIALS";
pub const ERR_AUTH_INSUFFICIENT_ROLES: &str = "ERR_AUTH_INSUFFICIENT_ROLES";
pub const ERR_AUTH_INSUFFICIENT_PERMS: &str = "ERR_AUTH_INSUFFICIENT_PERMS";
pub const ERR_AUTH_TENANT_MISMATCH: &str = "ERR_AUTH_TENANT_MISMATCH";
pub const ERR_AUTH_SIGNATURE_EXPIRED: &str = "ERR_AUTH_SIGNATURE_EXPIRED";
pub const ERR_AUTH_TOKEN_THEFT_DETECTED: &str = "ERR_AUTH_TOKEN_THEFT_DETECTED";

/// Standard URI types for RFC 9457 authentication problem details.
pub const TYPE_AUTH_MISSING: &str = "https://api.omni.internal/errors/auth/missing-credentials";
pub const TYPE_AUTH_EXPIRED: &str = "https://api.omni.internal/errors/auth/token-expired";
pub const TYPE_AUTH_REVOKED: &str = "https://api.omni.internal/errors/auth/token-revoked";
pub const TYPE_AUTH_INVALID_SIG: &str = "https://api.omni.internal/errors/auth/invalid-signature";
pub const TYPE_AUTH_MALFORMED: &str = "https://api.omni.internal/errors/auth/malformed-token";
pub const TYPE_AUTH_CREDENTIALS: &str = "https://api.omni.internal/errors/auth/invalid-credentials";
pub const TYPE_AUTH_ROLES: &str = "https://api.omni.internal/errors/auth/insufficient-roles";
pub const TYPE_AUTH_PERMS: &str = "https://api.omni.internal/errors/auth/insufficient-permissions";
pub const TYPE_AUTH_TENANT: &str = "https://api.omni.internal/errors/auth/tenant-mismatch";
pub const TYPE_AUTH_INTERNAL: &str = "https://api.omni.internal/errors/auth/internal-error";

/// Phân loại lỗi bảo mật, định danh và phân quyền kiểu tĩnh trong `omni-auth`.
#[derive(Debug, Clone, Error)]
pub enum AuthError {
    /// Thiếu header Authorization hoặc thông tin xác thực.
    #[error("Missing authorization credentials")]
    MissingCredentials,

    /// Mã thông báo JWT đã quá hạn (`exp`).
    #[error("Authentication token has expired")]
    TokenExpired,

    /// Mã thông báo đã bị thu hồi qua JTI Blacklist.
    #[error("Authentication token has been revoked")]
    TokenRevoked,

    /// Chữ ký số của mã thông báo không hợp lệ.
    #[error("Token cryptographic signature verification failed")]
    InvalidSignature,

    /// Cấu trúc JWT sai chuẩn hoặc không thể phân giải base64/JSON.
    #[error("Malformed token structure: {0}")]
    MalformedToken(String),

    /// Tên đăng nhập hoặc mật khẩu không khớp.
    #[error("Invalid username or password")]
    InvalidCredentials,

    /// Người dùng không có vai trò cần thiết.
    #[error("User does not possess the required role")]
    InsufficientRole,

    /// Người dùng thiếu quyền vi mô cụ thể.
    #[error("User lacks required permission: {0}")]
    InsufficientPermission(String),

    /// Không gian Tenant của request không khớp với ngữ cảnh người dùng.
    #[error("Access to tenant resource denied")]
    TenantMismatch,

    /// Chữ ký HMAC giữa Gateway và downstream pod đã quá hạn.
    #[error("Gateway signed context timestamp expired or clock skew exceeded")]
    SignatureExpired,

    /// Định dạng chữ ký Gateway không phải hex hợp lệ.
    #[error("Gateway signature format is not valid hex")]
    InvalidSignatureFormat,

    /// Token chưa tới thời điểm hiệu lực (`nbf`).
    #[error("Token is not valid yet (nbf claim)")]
    TokenNotYetValid,

    /// Nhà phát hành (`iss`) không khớp với cấu hình.
    #[error("Token issuer mismatch")]
    InvalidIssuer,

    /// Đối tượng (`aud`) không khớp với cấu hình.
    #[error("Token audience mismatch")]
    InvalidAudience,

    /// Phát hiện Refresh Token bị tái sử dụng trái phép (Token Theft).
    #[error("Refresh token reuse detected; whole token family invalidated")]
    TokenTheftDetected,

    /// Cấu hình mật mã không hợp lệ (thuật toán, PEM key, kích thước khóa).
    #[error("Cryptographic configuration error: {0}")]
    CryptoConfiguration(String),

    /// Thất bại khi băm mật khẩu qua Argon2id.
    #[error("Password hashing failed: {0}")]
    PasswordHashingFailed(String),

    /// Thất bại khi kiểm tra hash mật khẩu.
    #[error("Password verification failed: {0}")]
    PasswordVerificationFailed(String),

    /// Chuỗi hash mật khẩu không đúng chuẩn PHC string.
    #[error("Invalid PHC hash format: {0}")]
    InvalidHashFormat(String),

    /// Lỗi khi sinh JWT compact serialization.
    #[error("Token encoding failed: {0}")]
    TokenEncodingFailed(String),

    /// Tokio blocking task bị panic hoặc hủy.
    #[error("Internal worker task error: {0}")]
    InternalTaskPanic(String),

    /// Lỗi kho lưu trữ thu hồi (Redis / Cache).
    #[error("Revocation store error: {0}")]
    RevocationStoreError(String),
}

impl AuthError {
    /// Trả về mã lỗi nghiệp vụ chuẩn hóa.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingCredentials => ERR_AUTH_MISSING_CREDENTIALS,
            Self::TokenExpired => ERR_AUTH_TOKEN_EXPIRED,
            Self::TokenRevoked => ERR_AUTH_TOKEN_REVOKED,
            Self::InvalidSignature | Self::InvalidSignatureFormat => ERR_AUTH_INVALID_SIGNATURE,
            Self::MalformedToken(_)
            | Self::TokenNotYetValid
            | Self::InvalidIssuer
            | Self::InvalidAudience => ERR_AUTH_MALFORMED_TOKEN,
            Self::InvalidCredentials => ERR_AUTH_INVALID_CREDENTIALS,
            Self::InsufficientRole => ERR_AUTH_INSUFFICIENT_ROLES,
            Self::InsufficientPermission(_) => ERR_AUTH_INSUFFICIENT_PERMS,
            Self::TenantMismatch => ERR_AUTH_TENANT_MISMATCH,
            Self::SignatureExpired => ERR_AUTH_SIGNATURE_EXPIRED,
            Self::TokenTheftDetected => ERR_AUTH_TOKEN_THEFT_DETECTED,
            Self::CryptoConfiguration(_)
            | Self::PasswordHashingFailed(_)
            | Self::PasswordVerificationFailed(_)
            | Self::InvalidHashFormat(_)
            | Self::TokenEncodingFailed(_)
            | Self::InternalTaskPanic(_)
            | Self::RevocationStoreError(_) => "ERR_AUTH_INTERNAL",
        }
    }

    /// Trả về mã HTTP status tương ứng theo chuẩn RFC 9110 / RFC 9457.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::MissingCredentials
            | Self::TokenExpired
            | Self::TokenRevoked
            | Self::InvalidSignature
            | Self::MalformedToken(_)
            | Self::InvalidCredentials
            | Self::SignatureExpired
            | Self::InvalidSignatureFormat
            | Self::TokenNotYetValid
            | Self::InvalidIssuer
            | Self::InvalidAudience
            | Self::TokenTheftDetected => 401,

            Self::InsufficientRole | Self::InsufficientPermission(_) | Self::TenantMismatch => 403,

            Self::CryptoConfiguration(_)
            | Self::PasswordHashingFailed(_)
            | Self::PasswordVerificationFailed(_)
            | Self::InvalidHashFormat(_)
            | Self::TokenEncodingFailed(_)
            | Self::InternalTaskPanic(_)
            | Self::RevocationStoreError(_) => 500,
        }
    }

    /// Trả về URI type chuẩn RFC 9457.
    #[must_use]
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::MissingCredentials => TYPE_AUTH_MISSING,
            Self::TokenExpired => TYPE_AUTH_EXPIRED,
            Self::TokenRevoked | Self::SignatureExpired | Self::TokenTheftDetected => {
                TYPE_AUTH_REVOKED
            }
            Self::InvalidSignature | Self::InvalidSignatureFormat => TYPE_AUTH_INVALID_SIG,
            Self::MalformedToken(_)
            | Self::TokenNotYetValid
            | Self::InvalidIssuer
            | Self::InvalidAudience => TYPE_AUTH_MALFORMED,
            Self::InvalidCredentials => TYPE_AUTH_CREDENTIALS,
            Self::InsufficientRole => TYPE_AUTH_ROLES,
            Self::InsufficientPermission(_) => TYPE_AUTH_PERMS,
            Self::TenantMismatch => TYPE_AUTH_TENANT,
            Self::CryptoConfiguration(_)
            | Self::PasswordHashingFailed(_)
            | Self::PasswordVerificationFailed(_)
            | Self::InvalidHashFormat(_)
            | Self::TokenEncodingFailed(_)
            | Self::InternalTaskPanic(_)
            | Self::RevocationStoreError(_) => TYPE_AUTH_INTERNAL,
        }
    }

    /// Chuyển đổi sang [`ApiErrorResponse`] chuẩn RFC 9457 có hỗ trợ đa ngôn ngữ.
    #[must_use]
    pub fn to_problem(
        &self,
        instance: impl Into<Cow<'static, str>>,
        meta: ResponseMeta,
        i18n: &I18nManager,
    ) -> ApiErrorResponse {
        let code = self.code();
        let status = self.status_code();

        let title_key = match status {
            401 => "err-unauthorized-title",
            403 => "err-forbidden-title",
            _ => "err-internal-server-title",
        };
        let detail_key = format!("err-{}-detail", code.to_lowercase().replace('_', "-"));

        let title = i18n.translate_simple(&meta.locale, title_key);
        let detail = {
            let translated = i18n.translate_simple(&meta.locale, &detail_key);
            if translated == detail_key {
                // Fallback nếu chưa có bản dịch chi tiết trong Fluent
                self.to_string()
            } else {
                translated
            }
        };

        ApiErrorResponse::builder()
            .error_type(self.error_type())
            .title(title)
            .status(status)
            .detail(detail)
            .instance(instance.into())
            .code(code)
            .meta(meta)
            .build()
    }
}

impl From<AuthError> for omni_errors::AppError {
    fn from(err: AuthError) -> Self {
        let i18n = I18nManager::default();
        let meta = ResponseMeta::default();
        let problem = err.to_problem("/", meta, &i18n);
        Self::Problem(Box::new(problem))
    }
}

#[cfg(feature = "axum")]
impl axum::response::IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let code = self.code();
        let problem = self.to_problem("/", ResponseMeta::default(), &I18nManager::default());
        let mut resp = problem.into_response();
        resp.extensions_mut()
            .insert(ErrorCodeExtension(Cow::Borrowed(code)));
        resp
    }
}

/// Kết quả alias chuẩn cho các tác vụ Auth.
pub type AuthResult<T> = Result<T, AuthError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(
            AuthError::MissingCredentials.code(),
            ERR_AUTH_MISSING_CREDENTIALS
        );
        assert_eq!(AuthError::TokenExpired.code(), ERR_AUTH_TOKEN_EXPIRED);
        assert_eq!(AuthError::TokenRevoked.code(), ERR_AUTH_TOKEN_REVOKED);
        assert_eq!(
            AuthError::InvalidSignature.code(),
            ERR_AUTH_INVALID_SIGNATURE
        );
        assert_eq!(
            AuthError::InvalidSignatureFormat.code(),
            ERR_AUTH_INVALID_SIGNATURE
        );
        assert_eq!(
            AuthError::MalformedToken("bad".into()).code(),
            ERR_AUTH_MALFORMED_TOKEN
        );
        assert_eq!(AuthError::TokenNotYetValid.code(), ERR_AUTH_MALFORMED_TOKEN);
        assert_eq!(AuthError::InvalidIssuer.code(), ERR_AUTH_MALFORMED_TOKEN);
        assert_eq!(AuthError::InvalidAudience.code(), ERR_AUTH_MALFORMED_TOKEN);
        assert_eq!(
            AuthError::InvalidCredentials.code(),
            ERR_AUTH_INVALID_CREDENTIALS
        );
        assert_eq!(
            AuthError::InsufficientRole.code(),
            ERR_AUTH_INSUFFICIENT_ROLES
        );
        assert_eq!(
            AuthError::InsufficientPermission("perm".into()).code(),
            ERR_AUTH_INSUFFICIENT_PERMS
        );
        assert_eq!(AuthError::TenantMismatch.code(), ERR_AUTH_TENANT_MISMATCH);
        assert_eq!(
            AuthError::SignatureExpired.code(),
            ERR_AUTH_SIGNATURE_EXPIRED
        );
        assert_eq!(
            AuthError::TokenTheftDetected.code(),
            ERR_AUTH_TOKEN_THEFT_DETECTED
        );
        assert_eq!(
            AuthError::CryptoConfiguration("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
        assert_eq!(
            AuthError::PasswordHashingFailed("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
        assert_eq!(
            AuthError::PasswordVerificationFailed("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
        assert_eq!(
            AuthError::InvalidHashFormat("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
        assert_eq!(
            AuthError::TokenEncodingFailed("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
        assert_eq!(
            AuthError::InternalTaskPanic("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
        assert_eq!(
            AuthError::RevocationStoreError("err".into()).code(),
            "ERR_AUTH_INTERNAL"
        );
    }

    #[test]
    fn test_status_codes() {
        assert_eq!(AuthError::MissingCredentials.status_code(), 401);
        assert_eq!(AuthError::TokenExpired.status_code(), 401);
        assert_eq!(AuthError::TokenRevoked.status_code(), 401);
        assert_eq!(AuthError::InvalidSignature.status_code(), 401);
        assert_eq!(AuthError::MalformedToken("test".into()).status_code(), 401);
        assert_eq!(AuthError::InvalidCredentials.status_code(), 401);
        assert_eq!(AuthError::SignatureExpired.status_code(), 401);
        assert_eq!(AuthError::InvalidSignatureFormat.status_code(), 401);
        assert_eq!(AuthError::TokenNotYetValid.status_code(), 401);
        assert_eq!(AuthError::InvalidIssuer.status_code(), 401);
        assert_eq!(AuthError::InvalidAudience.status_code(), 401);
        assert_eq!(AuthError::TokenTheftDetected.status_code(), 401);

        assert_eq!(AuthError::InsufficientRole.status_code(), 403);
        assert_eq!(
            AuthError::InsufficientPermission("act".into()).status_code(),
            403
        );
        assert_eq!(AuthError::TenantMismatch.status_code(), 403);

        assert_eq!(
            AuthError::CryptoConfiguration("c".into()).status_code(),
            500
        );
        assert_eq!(
            AuthError::PasswordHashingFailed("h".into()).status_code(),
            500
        );
        assert_eq!(
            AuthError::PasswordVerificationFailed("v".into()).status_code(),
            500
        );
        assert_eq!(AuthError::InvalidHashFormat("f".into()).status_code(), 500);
        assert_eq!(
            AuthError::TokenEncodingFailed("e".into()).status_code(),
            500
        );
        assert_eq!(AuthError::InternalTaskPanic("p".into()).status_code(), 500);
        assert_eq!(
            AuthError::RevocationStoreError("s".into()).status_code(),
            500
        );
    }

    #[test]
    fn test_error_types() {
        assert_eq!(
            AuthError::MissingCredentials.error_type(),
            TYPE_AUTH_MISSING
        );
        assert_eq!(AuthError::TokenExpired.error_type(), TYPE_AUTH_EXPIRED);
        assert_eq!(AuthError::TokenRevoked.error_type(), TYPE_AUTH_REVOKED);
        assert_eq!(AuthError::SignatureExpired.error_type(), TYPE_AUTH_REVOKED);
        assert_eq!(
            AuthError::TokenTheftDetected.error_type(),
            TYPE_AUTH_REVOKED
        );
        assert_eq!(
            AuthError::InvalidSignature.error_type(),
            TYPE_AUTH_INVALID_SIG
        );
        assert_eq!(
            AuthError::InvalidSignatureFormat.error_type(),
            TYPE_AUTH_INVALID_SIG
        );
        assert_eq!(
            AuthError::MalformedToken("x".into()).error_type(),
            TYPE_AUTH_MALFORMED
        );
        assert_eq!(
            AuthError::TokenNotYetValid.error_type(),
            TYPE_AUTH_MALFORMED
        );
        assert_eq!(AuthError::InvalidIssuer.error_type(), TYPE_AUTH_MALFORMED);
        assert_eq!(AuthError::InvalidAudience.error_type(), TYPE_AUTH_MALFORMED);
        assert_eq!(
            AuthError::InvalidCredentials.error_type(),
            TYPE_AUTH_CREDENTIALS
        );
        assert_eq!(AuthError::InsufficientRole.error_type(), TYPE_AUTH_ROLES);
        assert_eq!(
            AuthError::InsufficientPermission("p".into()).error_type(),
            TYPE_AUTH_PERMS
        );
        assert_eq!(AuthError::TenantMismatch.error_type(), TYPE_AUTH_TENANT);
        assert_eq!(
            AuthError::CryptoConfiguration("e".into()).error_type(),
            TYPE_AUTH_INTERNAL
        );
    }

    #[test]
    fn test_to_problem_and_from_conversion() {
        let i18n = I18nManager::default();
        let meta = ResponseMeta::default();
        let err = AuthError::MissingCredentials;
        let problem = err.to_problem("/api/v1/resource", meta, &i18n);

        assert_eq!(problem.status, 401);
        assert_eq!(problem.code, ERR_AUTH_MISSING_CREDENTIALS);
        assert_eq!(problem.instance.as_ref(), "/api/v1/resource");
        assert_eq!(problem.error_type, TYPE_AUTH_MISSING);

        let app_err: omni_errors::AppError = err.into();
        match app_err {
            omni_errors::AppError::Problem(p) => {
                assert_eq!(p.status, 401);
                assert_eq!(p.code, ERR_AUTH_MISSING_CREDENTIALS);
            }
            _ => panic!("Expected AppError::Problem"),
        }
    }

    #[cfg(feature = "axum")]
    #[test]
    fn test_into_response() {
        use axum::response::IntoResponse;

        let err = AuthError::InsufficientRole;
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
        let ext = resp.extensions().get::<ErrorCodeExtension>();
        assert!(ext.is_some());
        assert_eq!(ext.unwrap().0.as_ref(), ERR_AUTH_INSUFFICIENT_ROLES);
    }
}
