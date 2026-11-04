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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_user(roles: Vec<Role>, permissions: Vec<String>) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: Some(Uuid::now_v7()),
            session_id: Some(Uuid::now_v7()),
            roles,
            permissions,
            email: Some("user@omni.internal".to_string()),
            token_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn test_authenticated_user_methods() {
        let user = make_test_user(
            vec![Role::User, Role::Operator],
            vec!["pods:read".to_string(), "services:*".to_string()],
        );

        assert!(!user.is_super_admin());
        assert!(user.has_role(&Role::User));
        assert!(user.has_role(&Role::Operator));
        assert!(!user.has_role(&Role::Admin));
        assert!(user.has_any_role(&[Role::Admin, Role::User]));
        assert!(!user.has_any_role(&[Role::Admin, Role::SuperAdmin]));

        assert!(user.has_permission("pods:read"));
        assert!(!user.has_permission("pods:delete"));
        assert!(user.has_permission("services:get"));
        assert!(user.has_permission("services:delete"));
        assert!(!user.has_permission("deployments:get"));

        assert!(user.has_all_permissions(&["pods:read", "services:get"]));
        assert!(!user.has_all_permissions(&["pods:read", "deployments:get"]));

        // SuperAdmin bypass
        let admin = make_test_user(vec![Role::SuperAdmin], vec![]);
        assert!(admin.is_super_admin());
        assert!(admin.has_role(&Role::Admin));
        assert!(admin.has_role(&Role::Custom("arbitrary".into())));
        assert!(admin.has_any_role(&[Role::Custom("arbitrary".into())]));
        assert!(admin.has_permission("any:action"));
        assert!(admin.has_all_permissions(&["a", "b", "c"]));
    }

    #[cfg(feature = "axum")]
    #[tokio::test]
    async fn test_authenticated_user_extractor() {
        use axum::http::Request;

        let user = make_test_user(vec![Role::User], vec![]);
        let mut req = Request::builder().body(()).unwrap();
        req.extensions_mut().insert(user.clone());

        let (mut parts, ()) = req.into_parts();
        let extracted = AuthenticatedUser::from_request_parts(&mut parts, &())
            .await
            .expect("Extraction should succeed");
        assert_eq!(extracted, user);

        // Without user in extensions
        let empty_req = Request::builder().body(()).unwrap();
        let (mut parts, ()) = empty_req.into_parts();
        let err = AuthenticatedUser::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        match err {
            omni_errors::AppError::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {other:?}"),
        }
    }

    #[cfg(feature = "axum")]
    #[tokio::test]
    async fn test_optional_user_extractor() {
        use axum::http::Request;

        let user = make_test_user(vec![Role::User], vec![]);
        let mut req = Request::builder().body(()).unwrap();
        req.extensions_mut().insert(user.clone());

        let (mut parts, ()) = req.into_parts();
        let opt_user = OptionalUser::from_request_parts(&mut parts, &())
            .await
            .expect("Optional extraction never fails");
        assert_eq!(opt_user.0, Some(user));

        // Without user
        let empty_req = Request::builder().body(()).unwrap();
        let (mut parts, ()) = empty_req.into_parts();
        let opt_user = OptionalUser::from_request_parts(&mut parts, &())
            .await
            .expect("Optional extraction never fails");
        assert_eq!(opt_user.0, None);
    }

    #[cfg(feature = "axum")]
    #[tokio::test]
    async fn test_bearer_token_extractor() {
        use axum::http::Request;

        // Valid Bearer
        let req = Request::builder()
            .header("Authorization", "Bearer valid_jwt_token_123")
            .body(())
            .unwrap();
        let (mut parts, ()) = req.into_parts();
        let token = BearerToken::from_request_parts(&mut parts, &())
            .await
            .expect("Bearer extraction should succeed");
        assert_eq!(token.0, "valid_jwt_token_123");

        // Valid Bearer with extra spaces
        let req = Request::builder()
            .header("Authorization", "Bearer    spaced_token   ")
            .body(())
            .unwrap();
        let (mut parts, ()) = req.into_parts();
        let token = BearerToken::from_request_parts(&mut parts, &())
            .await
            .expect("Bearer extraction should trim");
        assert_eq!(token.0, "spaced_token");

        // Missing Authorization header
        let req = Request::builder().body(()).unwrap();
        let (mut parts, ()) = req.into_parts();
        assert!(
            BearerToken::from_request_parts(&mut parts, &())
                .await
                .is_err()
        );

        // Non-Bearer header (e.g. Basic)
        let req = Request::builder()
            .header("Authorization", "Basic dXNlcjpwYXNz")
            .body(())
            .unwrap();
        let (mut parts, ()) = req.into_parts();
        assert!(
            BearerToken::from_request_parts(&mut parts, &())
                .await
                .is_err()
        );

        // Empty Bearer token
        let req = Request::builder()
            .header("Authorization", "Bearer   ")
            .body(())
            .unwrap();
        let (mut parts, ()) = req.into_parts();
        assert!(
            BearerToken::from_request_parts(&mut parts, &())
                .await
                .is_err()
        );
    }
}
