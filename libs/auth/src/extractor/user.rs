//! Axum extractors for authenticated user context.

#[cfg(feature = "axum")]
use axum::{extract::FromRequestParts, http::request::Parts};
#[cfg(feature = "axum")]
use std::future::Future;
use uuid::Uuid;

use crate::token::Role;

/// Danh tính người dùng đã được xác thực hoàn toàn (Bắt buộc đăng nhập).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Định danh duy nhất của người dùng (`UUIDv7`).
    pub user_id: Uuid,
    /// Định danh khách hàng doanh nghiệp (Tenant ID).
    pub tenant_id: Option<Uuid>,
    /// Định danh phiên làm việc của người dùng.
    pub session_id: Option<Uuid>,
    /// Danh sách vai trò được gán cho người dùng.
    pub roles: Vec<Role>,
    /// Danh sách quyền vi mô (`domain:action`).
    pub permissions: Vec<String>,
    /// Email liên hệ (nếu có).
    pub email: Option<String>,
    /// JWT ID (`jti`) của token hiện thời.
    pub token_id: Uuid,
}

impl AuthenticatedUser {
    /// Kiểm tra người dùng có phải `SuperAdmin` hay không.
    #[must_use]
    pub fn is_super_admin(&self) -> bool {
        self.roles.contains(&Role::SuperAdmin)
    }

    /// Kiểm tra người dùng có sở hữu vai trò cụ thể hay không.
    #[must_use]
    pub fn has_role(&self, role: &Role) -> bool {
        if self.is_super_admin() {
            return true;
        }
        self.roles.iter().any(|r| r == role)
    }

    /// Kiểm tra người dùng có sở hữu ít nhất một vai trò trong danh sách.
    #[must_use]
    pub fn has_any_role(&self, roles: &[Role]) -> bool {
        if self.is_super_admin() {
            return true;
        }
        roles.iter().any(|r| self.has_role(r))
    }

    /// Kiểm tra người dùng có quyền cụ thể hay không (hỗ trợ wildcard `*` và `domain:*`).
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

    /// Kiểm tra người dùng có toàn bộ các quyền trong danh sách.
    #[must_use]
    pub fn has_all_permissions(&self, permissions: &[&str]) -> bool {
        if self.is_super_admin() {
            return true;
        }
        permissions.iter().all(|p| self.has_permission(p))
    }
}

/// Extractor tùy chọn: Không bắt buộc đăng nhập (cho public endpoint có cá nhân hóa).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionalUser(pub Option<AuthenticatedUser>);

/// Extractor trích xuất chuỗi Bearer token thô từ HTTP header `Authorization`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearerToken(pub String);

#[cfg(feature = "axum")]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = omni_errors::AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let res = parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or(omni_errors::AppError::Unauthorized);
        std::future::ready(res)
    }
}

#[cfg(feature = "axum")]
impl<S> FromRequestParts<S> for OptionalUser
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let user = parts.extensions.get::<AuthenticatedUser>().cloned();
        std::future::ready(Ok(OptionalUser(user)))
    }
}

#[cfg(feature = "axum")]
impl<S> FromRequestParts<S> for BearerToken
where
    S: Send + Sync,
{
    type Rejection = omni_errors::AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let res = (|| {
            let auth_header = parts.headers.get(axum::http::header::AUTHORIZATION)?;
            let auth_str = auth_header.to_str().ok()?;
            let token = auth_str.strip_prefix("Bearer ")?;
            let trimmed = token.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(BearerToken(trimmed.to_string()))
            }
        })()
        .ok_or(omni_errors::AppError::Unauthorized);
        std::future::ready(res)
    }
}
