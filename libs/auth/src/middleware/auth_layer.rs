//! Global authentication middleware layer supporting JWT and Gateway Signed Context.

use axum::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use std::{
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use uuid::Uuid;

use crate::{
    bridge::{enrich_request_context, enrich_tracing_span},
    error::AuthError,
    extractor::AuthenticatedUser,
    instrument::{record_auth_failure, record_token_validation},
    middleware::signed_context::{
        GatewayContext, HEADER_OMNI_SIGNATURE, HEADER_OMNI_TIMESTAMP, HEADER_TENANT_ID,
        HEADER_USER_ID, HEADER_USER_ROLES, verify_gateway_signature,
    },
    revocation::RevocationStore,
    token::{Role, TokenEngine},
};

/// Middleware layer trung tâm giải mã danh tính người dùng từ Bearer JWT hoặc Signed Gateway Context.
#[derive(Clone)]
pub struct AuthLayer {
    token_engine: Arc<TokenEngine>,
    revocation_store: Option<Arc<dyn RevocationStore>>,
}

impl std::fmt::Debug for AuthLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthLayer")
            .field("token_engine", &self.token_engine)
            .field("has_revocation", &self.revocation_store.is_some())
            .finish()
    }
}

impl AuthLayer {
    /// Khởi tạo [`AuthLayer`] mới với [`TokenEngine`].
    #[must_use]
    pub fn new(token_engine: TokenEngine) -> Self {
        Self {
            token_engine: Arc::new(token_engine),
            revocation_store: None,
        }
    }

    /// Đính kèm kho lưu trữ JTI Blacklist để kiểm tra thu hồi token thời gian thực.
    #[must_use]
    pub fn with_revocation(mut self, revocation_store: Arc<dyn RevocationStore>) -> Self {
        self.revocation_store = Some(revocation_store);
        self
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            token_engine: self.token_engine.clone(),
            revocation_store: self.revocation_store.clone(),
        }
    }
}

/// Service thực thi xử lý xác thực trên từng HTTP Request.
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    token_engine: Arc<TokenEngine>,
    revocation_store: Option<Arc<dyn RevocationStore>>,
}

impl<S: std::fmt::Debug> std::fmt::Debug for AuthService<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("inner", &self.inner)
            .field("token_engine", &self.token_engine)
            .field("has_revocation", &self.revocation_store.is_some())
            .finish()
    }
}

impl<S> Service<Request<Body>> for AuthService<S>
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
        let engine = self.token_engine.clone();
        let revocation = self.revocation_store.clone();

        Box::pin(async move {
            // 1. KỊCH BẢN A: Kiểm tra Gateway Signed Context (Chữ ký nội bộ)
            if let Some(res) = try_gateway_auth(&engine, req.headers()) {
                return match res {
                    Ok(user) => {
                        inject_user(&mut req, user);
                        record_token_validation("success", "gateway_signature");
                        inner.call(req).await
                    }
                    Err(resp) => Ok(resp),
                };
            }

            // 2. KỊCH BẢN B: Kiểm tra Bearer Token trong header `Authorization`
            if let Some(res) = try_bearer_auth(&engine, revocation.as_ref(), req.headers()).await {
                return match res {
                    Ok(user) => {
                        inject_user(&mut req, user);
                        record_token_validation("success", "jwt");
                        inner.call(req).await
                    }
                    Err(resp) => Ok(resp),
                };
            }

            // 3. Yêu cầu không có thông tin xác thực: Cho phép đi tiếp để phục vụ public routes
            inner.call(req).await
        })
    }
}

fn inject_user(req: &mut Request<Body>, user: AuthenticatedUser) {
    enrich_tracing_span(&user);
    if let Some(ctx) = req
        .extensions_mut()
        .get_mut::<omni_common::middleware::RequestContext>()
    {
        enrich_request_context(ctx, &user);
    }
    req.extensions_mut().insert(user);
}

fn try_gateway_auth(
    engine: &TokenEngine,
    headers: &axum::http::HeaderMap,
) -> Option<Result<AuthenticatedUser, Response<Body>>> {
    let user_id_val = headers.get(HEADER_USER_ID)?;
    let sig_val = headers.get(HEADER_OMNI_SIGNATURE)?;
    let ts_val = headers.get(HEADER_OMNI_TIMESTAMP)?;

    let user_id_str = user_id_val.to_str().unwrap_or_default();
    let sig_str = sig_val.to_str().unwrap_or_default();
    let ts_str = ts_val.to_str().unwrap_or_default();

    let tenant_id_str = headers.get(HEADER_TENANT_ID).and_then(|v| v.to_str().ok());
    let roles_str = headers
        .get(HEADER_USER_ROLES)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let Ok(timestamp) = ts_str.parse::<i64>() else {
        record_auth_failure("invalid_gateway_timestamp");
        return Some(Err(AuthError::SignatureExpired.into_response()));
    };

    let now = Utc::now().timestamp();
    let config = engine.config();
    let max_age_secs = i64::try_from(config.signature_max_age_secs).unwrap_or(30);

    let ctx = GatewayContext::new(user_id_str, tenant_id_str, roles_str, timestamp);
    match verify_gateway_signature(
        &config.internal_gateway_secret,
        &ctx,
        now,
        max_age_secs,
        sig_str,
    ) {
        Ok(true) => {
            let Ok(user_id) = Uuid::parse_str(user_id_str) else {
                record_auth_failure("invalid_user_id_format");
                return Some(Err(AuthError::MalformedToken(
                    "Invalid user_id UUID".into(),
                )
                .into_response()));
            };

            let tenant_id = tenant_id_str.and_then(|s| Uuid::parse_str(s).ok());
            let roles: Vec<Role> = roles_str
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Role::from_str(s).unwrap_or(Role::Custom(s.to_string())))
                .collect();

            Some(Ok(AuthenticatedUser {
                user_id,
                tenant_id,
                session_id: None,
                roles,
                permissions: Vec::new(),
                email: None,
                token_id: Uuid::now_v7(),
            }))
        }
        Ok(false) => {
            record_auth_failure("invalid_gateway_sig");
            Some(Err(AuthError::InvalidSignature.into_response()))
        }
        Err(e) => {
            record_auth_failure("gateway_sig_err");
            Some(Err(e.into_response()))
        }
    }
}

async fn try_bearer_auth(
    engine: &TokenEngine,
    revocation: Option<&Arc<dyn RevocationStore>>,
    headers: &axum::http::HeaderMap,
) -> Option<Result<AuthenticatedUser, Response<Body>>> {
    let auth_val = headers.get(axum::http::header::AUTHORIZATION)?;
    let auth_str = auth_val.to_str().ok()?;

    let Some(token) = auth_str.strip_prefix("Bearer ") else {
        if !auth_str.is_empty() {
            record_auth_failure("missing_token");
            return Some(Err(AuthError::MissingCredentials.into_response()));
        }
        return None;
    };

    let trimmed = token.trim();
    match engine.decode_token(trimmed) {
        Ok(claims) => {
            // Tầng 2: JTI Blacklist Check trên Redis
            if let Some(rev_store) = revocation {
                match rev_store.is_token_revoked(claims.jti).await {
                    Ok(true) => {
                        record_auth_failure("revoked");
                        return Some(Err(AuthError::TokenRevoked.into_response()));
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "JTI revocation check failed; proceeding with caution");
                    }
                }
            }

            Some(Ok(AuthenticatedUser {
                user_id: claims.sub,
                tenant_id: claims.tenant_id,
                session_id: claims.session_id,
                roles: claims.roles,
                permissions: claims.permissions,
                email: claims.email,
                token_id: claims.jti,
            }))
        }
        Err(e) => {
            let reason = match e {
                AuthError::TokenExpired => "expired",
                AuthError::InvalidSignature => "invalid_sig",
                _ => "malformed",
            };
            record_auth_failure(reason);
            Some(Err(e.into_response()))
        }
    }
}
