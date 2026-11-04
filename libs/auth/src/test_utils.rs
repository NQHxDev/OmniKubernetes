//! Testing fixtures, mock claims builder, and `MockAuthLayer`.

use axum::{body::Body, extract::Request, response::Response};
use chrono::Utc;
use secrecy::SecretString;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use uuid::Uuid;

use crate::{
    config::AuthConfig,
    extractor::AuthenticatedUser,
    token::{AuthUserClaims, Role, TokenEngine, TokenType},
};

/// Builder tiện ích tạo [`AuthUserClaims`] giả lập cho unit & integration tests.
#[derive(Debug, Clone)]
pub struct ClaimsBuilder {
    claims: AuthUserClaims,
}

impl Default for ClaimsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaimsBuilder {
    /// Khởi tạo [`ClaimsBuilder`] với các giá trị mặc định hợp lệ.
    #[must_use]
    pub fn new() -> Self {
        let now = Utc::now().timestamp();
        Self {
            claims: AuthUserClaims {
                sub: Uuid::now_v7(),
                iss: "urn:omni:auth".to_string(),
                aud: "urn:omni:api".to_string(),
                exp: now + 900,
                nbf: now,
                iat: now,
                jti: Uuid::now_v7(),
                token_type: TokenType::Access,
                tenant_id: Some(Uuid::now_v7()),
                session_id: Some(Uuid::now_v7()),
                roles: vec![Role::User],
                permissions: vec!["orders:read".to_string()],
                email: Some("test.user@omni.internal".to_string()),
            },
        }
    }

    /// Tùy biến `user_id` (`sub`).
    #[must_use]
    pub fn with_sub(mut self, sub: Uuid) -> Self {
        self.claims.sub = sub;
        self
    }

    /// Tùy biến `tenant_id`.
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: Option<Uuid>) -> Self {
        self.claims.tenant_id = tenant_id;
        self
    }

    /// Tùy biến `session_id`.
    #[must_use]
    pub fn with_session_id(mut self, session_id: Option<Uuid>) -> Self {
        self.claims.session_id = session_id;
        self
    }

    /// Gán thêm một vai trò (`Role`).
    #[must_use]
    pub fn with_role(mut self, role: Role) -> Self {
        if !self.claims.roles.contains(&role) {
            self.claims.roles.push(role);
        }
        self
    }

    /// Gán danh sách vai trò mới.
    #[must_use]
    pub fn with_roles(mut self, roles: Vec<Role>) -> Self {
        self.claims.roles = roles;
        self
    }

    /// Gán thêm một quyền vi mô.
    #[must_use]
    pub fn with_permission(mut self, permission: impl Into<String>) -> Self {
        self.claims.permissions.push(permission.into());
        self
    }

    /// Gán danh sách quyền vi mô mới.
    #[must_use]
    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.claims.permissions = permissions;
        self
    }

    /// Đặt trạng thái token đã quá hạn (`exp = now - 60s`).
    #[must_use]
    pub fn expired(mut self) -> Self {
        let now = Utc::now().timestamp();
        self.claims.exp = now - 60;
        self.claims.nbf = now - 120;
        self.claims.iat = now - 120;
        self
    }

    /// Hoàn tất dựng [`AuthUserClaims`].
    #[must_use]
    pub fn build(self) -> AuthUserClaims {
        self.claims
    }
}

/// Khởi tạo một [`TokenEngine`] tiêu chuẩn với HMAC secret cho kiểm thử.
#[must_use]
pub fn create_test_token_engine() -> TokenEngine {
    let config = AuthConfig {
        enabled: true,
        jwt_secret: SecretString::from("test_secret_must_be_at_least_32_bytes_long_entropy"),
        ..Default::default()
    };
    TokenEngine::new(config).expect("Test token engine must initialize cleanly")
}

/// Tiện ích sinh token JWT từ claims giả lập.
#[must_use]
pub fn create_test_token(claims: &AuthUserClaims) -> String {
    let engine = create_test_token_engine();
    engine
        .encode_claims(claims)
        .expect("Token encoding must succeed")
}

/// Mock Middleware Layer tự động tiêm một [`AuthenticatedUser`] giả lập vào mọi request.
#[derive(Clone, Debug)]
pub struct MockAuthLayer {
    user: Arc<AuthenticatedUser>,
}

impl MockAuthLayer {
    /// Khởi tạo [`MockAuthLayer`] với danh tính người dùng chỉ định.
    #[must_use]
    pub fn new(user: AuthenticatedUser) -> Self {
        Self {
            user: Arc::new(user),
        }
    }
}

impl<S> Layer<S> for MockAuthLayer {
    type Service = MockAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MockAuthService {
            inner,
            user: self.user.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MockAuthService<S> {
    inner: S,
    user: Arc<AuthenticatedUser>,
}

impl<S> Service<Request<Body>> for MockAuthService<S>
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

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let user = (*self.user).clone();

        Box::pin(async move {
            req.extensions_mut().insert(user);
            inner.call(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use tower::{ServiceBuilder, ServiceExt, service_fn};

    #[test]
    fn test_claims_builder_full() {
        let sub = Uuid::now_v7();
        let tid = Uuid::now_v7();
        let sid = Uuid::now_v7();

        let claims = ClaimsBuilder::new()
            .with_sub(sub)
            .with_tenant_id(Some(tid))
            .with_session_id(Some(sid))
            .with_role(Role::Operator)
            .with_permission("nodes:drain")
            .expired()
            .build();

        assert_eq!(claims.sub, sub);
        assert_eq!(claims.tenant_id, Some(tid));
        assert_eq!(claims.session_id, Some(sid));
        assert!(claims.roles.contains(&Role::Operator));
        assert!(claims.permissions.contains(&"nodes:drain".to_string()));
        assert!(claims.exp < Utc::now().timestamp());

        let default_claims = ClaimsBuilder::default().build();
        assert_eq!(default_claims.iss, "urn:omni:auth");

        let with_roles_claims = ClaimsBuilder::new()
            .with_roles(vec![Role::Admin])
            .with_permissions(vec!["p1".into(), "p2".into()])
            .build();
        assert_eq!(with_roles_claims.roles, vec![Role::Admin]);
        assert_eq!(with_roles_claims.permissions.len(), 2);
    }

    #[test]
    fn test_create_test_token_and_engine() {
        let claims = ClaimsBuilder::new().build();
        let token = create_test_token(&claims);
        assert_ne!(token, "");

        let engine = create_test_token_engine();
        let decoded = engine.decode_token(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
    }

    async fn mock_user_echo_handler(
        req: Request<Body>,
    ) -> Result<Response, std::convert::Infallible> {
        let u = req
            .extensions()
            .get::<AuthenticatedUser>()
            .cloned()
            .unwrap();
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("x-user-id", u.user_id.to_string())
            .body(Body::empty())
            .unwrap())
    }

    #[tokio::test]
    async fn test_mock_auth_layer() {
        let user = AuthenticatedUser {
            user_id: Uuid::now_v7(),
            tenant_id: Some(Uuid::now_v7()),
            session_id: None,
            roles: vec![Role::Admin],
            permissions: vec!["*".to_string()],
            email: None,
            token_id: Uuid::now_v7(),
        };

        let layer = MockAuthLayer::new(user.clone());

        let mut svc = ServiceBuilder::new()
            .layer(layer)
            .service(service_fn(mock_user_echo_handler));

        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("x-user-id").unwrap(),
            &user.user_id.to_string()
        );
    }
}
