//! Axum extractors for multi-tenant isolation.

#[cfg(feature = "axum")]
use axum::{extract::FromRequestParts, http::request::Parts};
#[cfg(feature = "axum")]
use std::future::Future;
use uuid::Uuid;

use crate::extractor::user::AuthenticatedUser;

/// Extractor cô lập ngữ cảnh đa Tenant (Multi-Tenancy Isolation).
///
/// Tự động trích xuất `tenant_id` từ danh tính đã xác thực.
/// Nếu yêu cầu không mang `tenant_id` hợp lệ, từ chối với `403 Forbidden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CurrentTenant(pub Uuid);

impl CurrentTenant {
    /// Trả về [`Uuid`] của tenant.
    #[must_use]
    pub fn id(&self) -> Uuid {
        self.0
    }
}

impl std::ops::Deref for CurrentTenant {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for CurrentTenant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(feature = "axum")]
impl<S> FromRequestParts<S> for CurrentTenant
where
    S: Send + Sync,
{
    type Rejection = omni_errors::AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let res = if let Some(user) = parts.extensions.get::<AuthenticatedUser>() {
            if let Some(tenant_id) = user.tenant_id {
                Ok(CurrentTenant(tenant_id))
            } else {
                // Thiếu Tenant ID hoặc người dùng không thuộc Tenant nào
                Err(omni_errors::AppError::Forbidden)
            }
        } else {
            Err(omni_errors::AppError::Forbidden)
        };
        std::future::ready(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Role;

    #[test]
    fn test_current_tenant_basics() {
        let tid = Uuid::now_v7();
        let tenant = CurrentTenant(tid);

        assert_eq!(tenant.id(), tid);
        assert_eq!(*tenant, tid);
        assert_eq!(tenant.to_string(), tid.to_string());
    }

    #[cfg(feature = "axum")]
    #[tokio::test]
    async fn test_current_tenant_from_request_parts() {
        use axum::http::Request;

        let tid = Uuid::now_v7();
        let user_with_tenant = AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: Some(tid),
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
            token_id: Uuid::now_v7(),
        };

        // Case 1: Authenticated user with tenant_id -> Ok
        let mut req = Request::builder().body(()).unwrap();
        req.extensions_mut().insert(user_with_tenant);
        let (mut parts, ()) = req.into_parts();
        let current_tenant = CurrentTenant::from_request_parts(&mut parts, &())
            .await
            .expect("Should extract tenant");
        assert_eq!(current_tenant.id(), tid);

        // Case 2: Authenticated user with None tenant_id -> Forbidden
        let user_without_tenant = AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
            token_id: Uuid::now_v7(),
        };
        let mut req = Request::builder().body(()).unwrap();
        req.extensions_mut().insert(user_without_tenant);
        let (mut parts, ()) = req.into_parts();
        let err = CurrentTenant::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        match err {
            omni_errors::AppError::Forbidden => {}
            other => panic!("Expected Forbidden, got {other:?}"),
        }

        // Case 3: No user in extensions -> Forbidden
        let empty_req = Request::builder().body(()).unwrap();
        let (mut parts, ()) = empty_req.into_parts();
        let err = CurrentTenant::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        match err {
            omni_errors::AppError::Forbidden => {}
            other => panic!("Expected Forbidden, got {other:?}"),
        }
    }
}
