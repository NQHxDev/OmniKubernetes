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
