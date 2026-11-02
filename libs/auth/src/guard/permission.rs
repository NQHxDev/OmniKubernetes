//! Declarative authorization guards for fine-grained permissions.

use axum::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};

use crate::{error::AuthError, extractor::AuthenticatedUser};

/// Middleware Layer kiểm tra người dùng phải sở hữu một quyền hạn vi mô cụ thể.
#[derive(Clone, Debug)]
pub struct RequirePermissionLayer {
    permission: String,
}

/// Tạo layer yêu cầu quyền hạn vi mô (vd: `require_permission("orders:create")`).
pub fn require_permission(permission: impl Into<String>) -> RequirePermissionLayer {
    RequirePermissionLayer {
        permission: permission.into(),
    }
}

impl<S> Layer<S> for RequirePermissionLayer {
    type Service = RequirePermissionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequirePermissionService {
            inner,
            permission: self.permission.clone(),
        }
    }
}

/// Service thực thi kiểm tra quyền vi mô.
#[derive(Clone, Debug)]
pub struct RequirePermissionService<S> {
    inner: S,
    permission: String,
}

impl<S> Service<Request<Body>> for RequirePermissionService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let expected_perm = self.permission.clone();

        Box::pin(async move {
            let Some(user) = req.extensions().get::<AuthenticatedUser>() else {
                return Ok(AuthError::MissingCredentials.into_response());
            };

            if !user.has_permission(&expected_perm) {
                return Ok(AuthError::InsufficientPermission(expected_perm).into_response());
            }

            inner.call(req).await
        })
    }
}

/// Middleware Layer kiểm tra người dùng phải sở hữu toàn bộ các quyền trong danh sách.
#[derive(Clone, Debug)]
pub struct RequireAllPermissionsLayer {
    permissions: Vec<String>,
}

/// Tạo layer yêu cầu toàn bộ quyền chỉ định (vd: `require_all_permissions(&["orders:read", "orders:write"])`).
#[must_use]
pub fn require_all_permissions(permissions: &[&str]) -> RequireAllPermissionsLayer {
    RequireAllPermissionsLayer {
        permissions: permissions.iter().map(|s| (*s).to_string()).collect(),
    }
}

impl<S> Layer<S> for RequireAllPermissionsLayer {
    type Service = RequireAllPermissionsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireAllPermissionsService {
            inner,
            permissions: self.permissions.clone(),
        }
    }
}

/// Service thực thi kiểm tra toàn bộ quyền.
#[derive(Clone, Debug)]
pub struct RequireAllPermissionsService<S> {
    inner: S,
    permissions: Vec<String>,
}

impl<S> Service<Request<Body>> for RequireAllPermissionsService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let permissions = self.permissions.clone();

        Box::pin(async move {
            let Some(user) = req.extensions().get::<AuthenticatedUser>() else {
                return Ok(AuthError::MissingCredentials.into_response());
            };

            let perms_ref: Vec<&str> = permissions.iter().map(String::as_str).collect();
            if !user.has_all_permissions(&perms_ref) {
                let missing = permissions
                    .iter()
                    .find(|p| !user.has_permission(p))
                    .cloned()
                    .unwrap_or_default();
                return Ok(AuthError::InsufficientPermission(missing).into_response());
            }

            inner.call(req).await
        })
    }
}
