//! Strictly type-safe JWT Claims specification and RBAC/Permission abstractions.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Định danh loại token trong vòng đời xác thực.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// Token truy cập ngắn hạn (vd: 15 phút), mang đầy đủ quyền hạn.
    Access,
    /// Token làm mới dài hạn (vd: 7 ngày), chỉ dùng để xin cấp cặp token mới.
    Refresh,
    /// Khóa API định danh dịch vụ máy bay (Machine-to-Machine / Service Account).
    ApiKey,
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access => write!(f, "access"),
            Self::Refresh => write!(f, "refresh"),
            Self::ApiKey => write!(f, "api_key"),
        }
    }
}

/// Vai trò người dùng trong hệ thống (Role-Based Access Control).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Quản trị viên tối cao, vượt qua mọi kiểm tra quyền (Bypass authorization).
    SuperAdmin,
    /// Quản trị viên hệ thống / tenant.
    Admin,
    /// Nhân sự vận hành kỹ thuật.
    Operator,
    /// Người dùng chuẩn.
    User,
    /// Tài khoản dịch vụ nội bộ (Machine-to-Machine).
    ServiceAccount,
    /// Vai trò tùy biến theo nhu cầu doanh nghiệp.
    Custom(String),
}

impl Role {
    /// Chuỗi định danh tĩnh của role.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::SuperAdmin => "super_admin",
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::User => "user",
            Self::ServiceAccount => "service_account",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let role = match s.trim().to_ascii_lowercase().as_str() {
            "super_admin" | "superadmin" => Self::SuperAdmin,
            "admin" => Self::Admin,
            "operator" => Self::Operator,
            "user" => Self::User,
            "service_account" | "serviceaccount" => Self::ServiceAccount,
            other => Self::Custom(other.to_string()),
        };
        Ok(role)
    }
}

/// Bộ Claims hoàn chỉnh được mã hóa trong payload của JWT tuân thủ RFC 7519 & RFC 9562.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthUserClaims {
    // --- Registered Claims (RFC 7519) ---
    /// Subject: User ID (`UUIDv7` duy nhất của người dùng).
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
    /// JWT ID: Định danh duy nhất của token `UUIDv7`, dùng cho JTI Revocation Blacklist.
    pub jti: Uuid,

    // --- Custom Enterprise Claims ---
    /// Loại token: Access | Refresh | `ApiKey`.
    pub token_type: TokenType,
    /// Tenant ID: Định danh tổ chức/khách hàng cho kiến trúc Đa Tenant (Multi-Tenant).
    pub tenant_id: Option<Uuid>,
    /// Session ID: Định danh phiên đăng nhập người dùng `UUIDv7`.
    pub session_id: Option<Uuid>,
    /// Danh sách các vai trò được gán cho người dùng.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Danh sách các quyền hạn vi mô dạng `domain:action` (vd: "orders:create", "payment:refund").
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Email người dùng (tùy chọn, phục vụ hiển thị nhanh).
    pub email: Option<String>,
}

impl AuthUserClaims {
    /// Kiểm tra xem token đã hết hạn tại thời điểm tham chiếu hay chưa (có tính clock skew).
    #[must_use]
    pub fn is_expired_at(&self, now_timestamp: i64, clock_skew_leeway: i64) -> bool {
        now_timestamp >= (self.exp + clock_skew_leeway)
    }

    /// Kiểm tra người dùng có phải `SuperAdmin` hay không.
    #[must_use]
    pub fn is_super_admin(&self) -> bool {
        self.roles.contains(&Role::SuperAdmin)
    }

    /// Kiểm tra người dùng có sở hữu vai trò cụ thể hay không (`SuperAdmin` luôn thỏa mãn).
    #[must_use]
    pub fn has_role(&self, role: &Role) -> bool {
        if self.is_super_admin() {
            return true;
        }
        self.roles.iter().any(|r| r == role)
    }

    /// Kiểm tra người dùng có sở hữu ít nhất một trong các vai trò được chỉ định.
    #[must_use]
    pub fn has_any_role(&self, roles: &[Role]) -> bool {
        if self.is_super_admin() {
            return true;
        }
        roles.iter().any(|r| self.has_role(r))
    }

    /// Kiểm tra người dùng có quyền cụ thể hay không (hỗ trợ wildcard matching `*` và `order:*`).
    #[must_use]
    pub fn has_permission(&self, permission: &str) -> bool {
        if self.is_super_admin() {
            return true;
        }
        self.permissions.iter().any(|p| {
            p == permission
                || p == "*"
                || (p.ends_with(":*") && permission.starts_with(&p[..p.len() - 1]))
        })
    }

    /// Kiểm tra người dùng có tất cả các quyền được chỉ định hay không.
    #[must_use]
    pub fn has_all_permissions(&self, permissions: &[&str]) -> bool {
        if self.is_super_admin() {
            return true;
        }
        permissions.iter().all(|p| self.has_permission(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_type_display_and_serde() {
        assert_eq!(TokenType::Access.to_string(), "access");
        assert_eq!(TokenType::Refresh.to_string(), "refresh");
        assert_eq!(TokenType::ApiKey.to_string(), "api_key");

        let access_json = serde_json::to_string(&TokenType::Access).unwrap();
        assert_eq!(access_json, "\"access\"");
        let deserialized: TokenType = serde_json::from_str(&access_json).unwrap();
        assert_eq!(deserialized, TokenType::Access);

        let refresh_json = serde_json::to_string(&TokenType::Refresh).unwrap();
        assert_eq!(refresh_json, "\"refresh\"");
        let deserialized_refresh: TokenType = serde_json::from_str(&refresh_json).unwrap();
        assert_eq!(deserialized_refresh, TokenType::Refresh);

        let api_key_json = serde_json::to_string(&TokenType::ApiKey).unwrap();
        assert_eq!(api_key_json, "\"api_key\"");
        let deserialized_api_key: TokenType = serde_json::from_str(&api_key_json).unwrap();
        assert_eq!(deserialized_api_key, TokenType::ApiKey);
    }

    #[test]
    fn test_role_methods_and_from_str() {
        assert_eq!(Role::SuperAdmin.as_str(), "super_admin");
        assert_eq!(Role::Admin.as_str(), "admin");
        assert_eq!(Role::Operator.as_str(), "operator");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::ServiceAccount.as_str(), "service_account");
        assert_eq!(Role::Custom("billing".to_string()).as_str(), "billing");

        assert_eq!(Role::SuperAdmin.to_string(), "super_admin");
        assert_eq!(
            Role::Custom("custom_role".into()).to_string(),
            "custom_role"
        );

        assert_eq!(Role::from_str("super_admin").unwrap(), Role::SuperAdmin);
        assert_eq!(Role::from_str("SUPERADMIN").unwrap(), Role::SuperAdmin);
        assert_eq!(Role::from_str("admin").unwrap(), Role::Admin);
        assert_eq!(Role::from_str("Operator").unwrap(), Role::Operator);
        assert_eq!(Role::from_str("user").unwrap(), Role::User);
        assert_eq!(
            Role::from_str("service_account").unwrap(),
            Role::ServiceAccount
        );
        assert_eq!(
            Role::from_str("ServiceAccount").unwrap(),
            Role::ServiceAccount
        );
        assert_eq!(
            Role::from_str(" auditor ").unwrap(),
            Role::Custom("auditor".to_string())
        );

        let serialized = serde_json::to_string(&Role::Admin).unwrap();
        assert_eq!(serialized, "\"admin\"");
        let deserialized: Role = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Role::Admin);

        let custom_ser = serde_json::to_string(&Role::Custom("support".to_string())).unwrap();
        let custom_de: Role = serde_json::from_str(&custom_ser).unwrap();
        assert_eq!(custom_de, Role::Custom("support".to_string()));
    }

    #[test]
    fn test_claims_is_expired_at() {
        let claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: 1000,
            nbf: 900,
            iat: 900,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
        };

        // exp = 1000, leeway = 10 -> boundary is 1010
        assert!(!claims.is_expired_at(999, 10));
        assert!(!claims.is_expired_at(1005, 10));
        assert!(claims.is_expired_at(1010, 10));
        assert!(claims.is_expired_at(1020, 10));
    }

    #[test]
    fn test_claims_super_admin_bypass() {
        let claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: 2000,
            nbf: 1000,
            iat: 1000,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::SuperAdmin],
            permissions: vec![],
            email: None,
        };

        assert!(claims.is_super_admin());
        assert!(claims.has_role(&Role::Admin));
        assert!(claims.has_role(&Role::User));
        assert!(claims.has_role(&Role::Custom("unknown".to_string())));
        assert!(claims.has_any_role(&[Role::Operator]));
        assert!(claims.has_permission("users:delete"));
        assert!(claims.has_all_permissions(&["any:one", "any:two"]));
    }

    #[test]
    fn test_claims_normal_role_checks() {
        let claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: 2000,
            nbf: 1000,
            iat: 1000,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User, Role::Operator],
            permissions: vec![],
            email: None,
        };

        assert!(!claims.is_super_admin());
        assert!(claims.has_role(&Role::User));
        assert!(claims.has_role(&Role::Operator));
        assert!(!claims.has_role(&Role::Admin));
        assert!(claims.has_any_role(&[Role::Admin, Role::User]));
        assert!(!claims.has_any_role(&[Role::Admin, Role::SuperAdmin]));
    }

    #[test]
    fn test_claims_permission_wildcards() {
        let claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: 2000,
            nbf: 1000,
            iat: 1000,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec!["orders:*".to_string(), "users:read".to_string()],
            email: None,
        };

        assert!(claims.has_permission("orders:read"));
        assert!(claims.has_permission("orders:write"));
        assert!(claims.has_permission("orders:delete"));
        assert!(claims.has_permission("users:read"));
        assert!(!claims.has_permission("users:write"));
        assert!(!claims.has_permission("billing:read"));

        assert!(claims.has_all_permissions(&["orders:read", "users:read"]));
        assert!(!claims.has_all_permissions(&["orders:read", "users:write"]));

        // Global wildcard
        let wildcard_claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: 2000,
            nbf: 1000,
            iat: 1000,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec!["*".to_string()],
            email: None,
        };
        assert!(wildcard_claims.has_permission("anything:at:all"));
        assert!(wildcard_claims.has_all_permissions(&["p1", "p2", "p3"]));
    }

    #[test]
    fn test_claims_serde_roundtrip() {
        let sub = Uuid::now_v7();
        let jti = Uuid::now_v7();
        let tenant_id = Some(Uuid::now_v7());
        let session_id = Some(Uuid::now_v7());

        let claims = AuthUserClaims {
            sub,
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: 1_700_000_000,
            nbf: 1_699_990_000,
            iat: 1_699_990_000,
            jti,
            token_type: TokenType::Access,
            tenant_id,
            session_id,
            roles: vec![Role::Admin, Role::Custom("auditor".to_string())],
            permissions: vec!["orders:read".to_string()],
            email: Some("admin@omni.internal".to_string()),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: AuthUserClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims, deserialized);
    }
}
