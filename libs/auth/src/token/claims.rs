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
