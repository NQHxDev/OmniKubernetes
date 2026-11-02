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
