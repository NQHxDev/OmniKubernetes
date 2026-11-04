//! Declarative authorization guards for Role-Based Access Control (RBAC).

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

use crate::{error::AuthError, extractor::AuthenticatedUser, token::Role};

/// Middleware Layer kiểm tra người dùng phải sở hữu một vai trò cụ thể.
#[derive(Clone, Debug)]
pub struct RequireRoleLayer {
    role: Role,
}

/// Tạo layer yêu cầu vai trò xác định (vd: `require_role(Role::Admin)`).
#[must_use]
pub fn require_role(role: Role) -> RequireRoleLayer {
    RequireRoleLayer { role }
}

impl<S> Layer<S> for RequireRoleLayer {
    type Service = RequireRoleService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireRoleService {
            inner,
            role: self.role.clone(),
        }
    }
}

/// Service thực thi kiểm tra vai trò.
#[derive(Clone, Debug)]
pub struct RequireRoleService<S> {
    inner: S,
    role: Role,
}

impl<S> Service<Request<Body>> for RequireRoleService<S>
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
        let expected_role = self.role.clone();

        Box::pin(async move {
            let Some(user) = req.extensions().get::<AuthenticatedUser>() else {
                return Ok(AuthError::MissingCredentials.into_response());
            };

            if !user.has_role(&expected_role) {
                return Ok(AuthError::InsufficientRole.into_response());
            }

            inner.call(req).await
        })
    }
}

/// Middleware Layer kiểm tra người dùng phải sở hữu ít nhất một trong các vai trò.
#[derive(Clone, Debug)]
pub struct RequireAnyRoleLayer {
    roles: Vec<Role>,
}

/// Tạo layer yêu cầu ít nhất một vai trò hợp lệ (vd: `require_any_role(&[Role::Admin, Role::Operator])`).
#[must_use]
pub fn require_any_role(roles: &[Role]) -> RequireAnyRoleLayer {
    RequireAnyRoleLayer {
        roles: roles.to_vec(),
    }
}

impl<S> Layer<S> for RequireAnyRoleLayer {
    type Service = RequireAnyRoleService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireAnyRoleService {
            inner,
            roles: self.roles.clone(),
        }
    }
}

/// Service thực thi kiểm tra ít nhất một vai trò.
#[derive(Clone, Debug)]
pub struct RequireAnyRoleService<S> {
    inner: S,
    roles: Vec<Role>,
}

impl<S> Service<Request<Body>> for RequireAnyRoleService<S>
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
        let roles = self.roles.clone();

        Box::pin(async move {
            let Some(user) = req.extensions().get::<AuthenticatedUser>() else {
                return Ok(AuthError::MissingCredentials.into_response());
            };

            if !user.has_any_role(&roles) {
                return Ok(AuthError::InsufficientRole.into_response());
            }

            inner.call(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, response::Response};
    use tower::{ServiceBuilder, ServiceExt, service_fn};
    use uuid::Uuid;

    fn make_user(roles: Vec<Role>) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: Some(Uuid::now_v7()),
            session_id: None,
            roles,
            permissions: vec![],
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
    async fn test_require_role_layer_allows_matching_role() {
        let mut svc = ServiceBuilder::new()
            .layer(require_role(Role::Admin))
            .service(service_fn(dummy_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(make_user(vec![Role::Admin]));

        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_role_layer_allows_super_admin() {
        let mut svc = ServiceBuilder::new()
            .layer(require_role(Role::Admin))
            .service(service_fn(dummy_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(make_user(vec![Role::SuperAdmin]));

        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_require_role_layer_rejects_missing_role() {
        let mut svc = ServiceBuilder::new()
            .layer(require_role(Role::Admin))
            .service(service_fn(dummy_handler));

        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(make_user(vec![Role::User]));

        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_require_role_layer_rejects_unauthenticated() {
        let mut svc = ServiceBuilder::new()
            .layer(require_role(Role::Admin))
            .service(service_fn(dummy_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_require_any_role_layer() {
        let mut svc = ServiceBuilder::new()
            .layer(require_any_role(&[Role::Admin, Role::Operator]))
            .service(service_fn(dummy_handler));

        // Matching one role
        let mut req1 = Request::builder().body(Body::empty()).unwrap();
        req1.extensions_mut()
            .insert(make_user(vec![Role::Operator]));
        let resp1 = svc.ready().await.unwrap().call(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        // Matching super admin
        let mut req2 = Request::builder().body(Body::empty()).unwrap();
        req2.extensions_mut()
            .insert(make_user(vec![Role::SuperAdmin]));
        let resp2 = svc.ready().await.unwrap().call(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);

        // None of the roles
        let mut req3 = Request::builder().body(Body::empty()).unwrap();
        req3.extensions_mut().insert(make_user(vec![Role::User]));
        let resp3 = svc.ready().await.unwrap().call(req3).await.unwrap();
        assert_eq!(resp3.status(), StatusCode::FORBIDDEN);

        // No user
        let req4 = Request::builder().body(Body::empty()).unwrap();
        let resp4 = svc.ready().await.unwrap().call(req4).await.unwrap();
        assert_eq!(resp4.status(), StatusCode::UNAUTHORIZED);
    }
}
