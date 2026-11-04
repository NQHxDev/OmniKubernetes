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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Role;
    use axum::{body::Body, http::StatusCode, response::Response};
    use tower::{ServiceBuilder, ServiceExt, service_fn};
    use uuid::Uuid;

    fn make_user(roles: Vec<Role>, permissions: Vec<String>) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: Some(Uuid::now_v7()),
            session_id: None,
            roles,
            permissions,
            email: None,
            token_id: Uuid::now_v7(),
        }
    }

    async fn dummy_handler(_req: Request<Body>) -> Result<Response, std::convert::Infallible> {
        Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap())
    }

    #[tokio::test]
    async fn test_require_permission_layer_allows_exact_and_wildcards() {
        let mut svc = ServiceBuilder::new()
            .layer(require_permission("orders:create"))
            .service(service_fn(dummy_handler));

        // Exact match
        let mut req1 = Request::builder().body(Body::empty()).unwrap();
        req1.extensions_mut().insert(make_user(
            vec![Role::User],
            vec!["orders:create".to_string()],
        ));
        let resp1 = svc.ready().await.unwrap().call(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        // Domain wildcard match
        let mut req2 = Request::builder().body(Body::empty()).unwrap();
        req2.extensions_mut()
            .insert(make_user(vec![Role::User], vec!["orders:*".to_string()]));
        let resp2 = svc.ready().await.unwrap().call(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);

        // Global wildcard match
        let mut req3 = Request::builder().body(Body::empty()).unwrap();
        req3.extensions_mut()
            .insert(make_user(vec![Role::User], vec!["*".to_string()]));
        let resp3 = svc.ready().await.unwrap().call(req3).await.unwrap();
        assert_eq!(resp3.status(), StatusCode::OK);

        // SuperAdmin bypass
        let mut req4 = Request::builder().body(Body::empty()).unwrap();
        req4.extensions_mut()
            .insert(make_user(vec![Role::SuperAdmin], vec![]));
        let resp4 = svc.ready().await.unwrap().call(req4).await.unwrap();
        assert_eq!(resp4.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_permission_layer_rejects_missing_permission() {
        let mut svc = ServiceBuilder::new()
            .layer(require_permission("orders:create"))
            .service(service_fn(dummy_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(make_user(vec![Role::User], vec!["orders:read".to_string()]));
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_require_permission_layer_rejects_unauthenticated() {
        let mut svc = ServiceBuilder::new()
            .layer(require_permission("orders:create"))
            .service(service_fn(dummy_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_require_all_permissions_layer() {
        let mut svc = ServiceBuilder::new()
            .layer(require_all_permissions(&["orders:read", "orders:write"]))
            .service(service_fn(dummy_handler));

        // Has all permissions
        let mut req1 = Request::builder().body(Body::empty()).unwrap();
        req1.extensions_mut().insert(make_user(
            vec![Role::User],
            vec!["orders:read".to_string(), "orders:write".to_string()],
        ));
        let resp1 = svc.ready().await.unwrap().call(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        // Has wildcard covering all
        let mut req2 = Request::builder().body(Body::empty()).unwrap();
        req2.extensions_mut()
            .insert(make_user(vec![Role::User], vec!["orders:*".to_string()]));
        let resp2 = svc.ready().await.unwrap().call(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);

        // Missing one permission
        let mut req3 = Request::builder().body(Body::empty()).unwrap();
        req3.extensions_mut()
            .insert(make_user(vec![Role::User], vec!["orders:read".to_string()]));
        let resp3 = svc.ready().await.unwrap().call(req3).await.unwrap();
        assert_eq!(resp3.status(), StatusCode::FORBIDDEN);

        // SuperAdmin bypass
        let mut req4 = Request::builder().body(Body::empty()).unwrap();
        req4.extensions_mut()
            .insert(make_user(vec![Role::SuperAdmin], vec![]));
        let resp4 = svc.ready().await.unwrap().call(req4).await.unwrap();
        assert_eq!(resp4.status(), StatusCode::OK);

        // Unauthenticated
        let req5 = Request::builder().body(Body::empty()).unwrap();
        let resp5 = svc.ready().await.unwrap().call(req5).await.unwrap();
        assert_eq!(resp5.status(), StatusCode::UNAUTHORIZED);
    }
}
