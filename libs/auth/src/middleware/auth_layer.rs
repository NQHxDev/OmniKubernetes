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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AuthConfig,
        middleware::signed_context::sign_gateway_context,
        revocation::MemoryRevocationStore,
        token::{AuthUserClaims, TokenType},
    };
    use axum::http::{HeaderMap, StatusCode};
    use secrecy::SecretString;
    use tower::{ServiceBuilder, ServiceExt, service_fn};

    fn make_test_engine() -> (TokenEngine, SecretString) {
        let gateway_secret = SecretString::from("test_gateway_shared_hmac_secret_key_12345");
        let config = AuthConfig {
            enabled: true,
            jwt_secret: SecretString::from("test_jwt_secret_must_be_at_least_32_bytes_long_1234"),
            internal_gateway_secret: gateway_secret.clone(),
            signature_max_age_secs: 30,
            ..Default::default()
        };
        (TokenEngine::new(config).unwrap(), gateway_secret)
    }

    #[test]
    fn test_auth_layer_debug_and_builder() {
        let (engine, _) = make_test_engine();
        let rev_store = Arc::new(MemoryRevocationStore::new());
        let layer = AuthLayer::new(engine).with_revocation(rev_store);

        let debug_str = format!("{layer:?}");
        assert!(debug_str.contains("AuthLayer"));
        assert!(debug_str.contains("has_revocation: true"));
    }

    #[tokio::test]
    async fn test_try_gateway_auth() {
        let (engine, gateway_secret) = make_test_engine();

        // 1. Missing gateway headers
        let empty_headers = HeaderMap::new();
        assert!(try_gateway_auth(&engine, &empty_headers).is_none());

        // 2. Valid gateway headers
        let uid = Uuid::now_v7();
        let tid = Uuid::now_v7();
        let uid_str = uid.to_string();
        let tid_str = tid.to_string();
        let ts = Utc::now().timestamp();
        let ctx = GatewayContext::new(&uid_str, Some(&tid_str), "admin,operator", ts);
        let sig = sign_gateway_context(&gateway_secret, &ctx).unwrap();

        let mut valid_headers = HeaderMap::new();
        valid_headers.insert(HEADER_USER_ID, uid_str.parse().unwrap());
        valid_headers.insert(HEADER_TENANT_ID, tid_str.parse().unwrap());
        valid_headers.insert(HEADER_USER_ROLES, "admin,operator".parse().unwrap());
        valid_headers.insert(HEADER_OMNI_TIMESTAMP, ts.to_string().parse().unwrap());
        valid_headers.insert(HEADER_OMNI_SIGNATURE, sig.parse().unwrap());

        let res = try_gateway_auth(&engine, &valid_headers).unwrap();
        let user = res.expect("Gateway auth should succeed");
        assert_eq!(user.user_id, uid);
        assert_eq!(user.tenant_id, Some(tid));
        assert!(user.has_role(&Role::Admin));
        assert!(user.has_role(&Role::Operator));

        // 3. Invalid UUID in user_id header
        let mut invalid_uid_headers = valid_headers.clone();
        invalid_uid_headers.insert(HEADER_USER_ID, "not-a-valid-uuid".parse().unwrap());
        let bad_ctx = GatewayContext::new("not-a-valid-uuid", Some(&tid_str), "admin,operator", ts);
        let bad_sig = sign_gateway_context(&gateway_secret, &bad_ctx).unwrap();
        invalid_uid_headers.insert(HEADER_OMNI_SIGNATURE, bad_sig.parse().unwrap());

        let res = try_gateway_auth(&engine, &invalid_uid_headers).unwrap();
        assert!(res.is_err());

        // 4. Invalid timestamp
        let mut bad_ts_headers = valid_headers.clone();
        bad_ts_headers.insert(HEADER_OMNI_TIMESTAMP, "not-a-number".parse().unwrap());
        let res = try_gateway_auth(&engine, &bad_ts_headers).unwrap();
        assert!(res.is_err());

        // 5. Invalid signature
        let mut bad_sig_headers = valid_headers.clone();
        bad_sig_headers.insert(HEADER_OMNI_SIGNATURE, "invalid_sig_hex".parse().unwrap());
        let res = try_gateway_auth(&engine, &bad_sig_headers).unwrap();
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_try_bearer_auth() {
        let (engine, _) = make_test_engine();
        let rev_store: Arc<dyn RevocationStore> = Arc::new(MemoryRevocationStore::new());

        // 1. Missing header
        let headers = HeaderMap::new();
        assert!(try_bearer_auth(&engine, None, &headers).await.is_none());

        // 2. Non-Bearer authorization header
        let mut basic_headers = HeaderMap::new();
        basic_headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic abc:123".parse().unwrap(),
        );
        let res = try_bearer_auth(&engine, None, &basic_headers)
            .await
            .unwrap();
        assert!(res.is_err());

        // 3. Valid Bearer JWT
        let sub = Uuid::now_v7();
        let jti = Uuid::now_v7();
        let now = Utc::now().timestamp();
        let claims = AuthUserClaims {
            sub,
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: now + 300,
            nbf: now - 10,
            iat: now,
            jti,
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec!["pods:list".to_string()],
            email: None,
        };
        let token = engine.encode_claims(&claims).unwrap();

        let mut bearer_headers = HeaderMap::new();
        bearer_headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );

        let res = try_bearer_auth(&engine, Some(&rev_store), &bearer_headers)
            .await
            .unwrap();
        let user = res.expect("Valid token must decode");
        assert_eq!(user.user_id, sub);
        assert_eq!(user.token_id, jti);

        // 4. Revoked token
        rev_store
            .revoke_token(jti, now + 300, "user_logout")
            .await
            .unwrap();
        let res_revoked = try_bearer_auth(&engine, Some(&rev_store), &bearer_headers)
            .await
            .unwrap();
        assert!(res_revoked.is_err());

        // 5. Expired token
        let expired_claims = AuthUserClaims {
            exp: now - 60,
            nbf: now - 120,
            iat: now - 120,
            ..claims
        };
        let expired_token = engine.encode_claims(&expired_claims).unwrap();
        let mut expired_headers = HeaderMap::new();
        expired_headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {expired_token}").parse().unwrap(),
        );
        let res_expired = try_bearer_auth(&engine, None, &expired_headers)
            .await
            .unwrap();
        assert!(res_expired.is_err());
    }

    async fn echo_user_handler(req: Request<Body>) -> Result<Response, std::convert::Infallible> {
        if let Some(user) = req.extensions().get::<AuthenticatedUser>() {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("x-authenticated-user", user.user_id.to_string())
                .body(Body::empty())
                .unwrap())
        } else {
            Ok(Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(Body::empty())
                .unwrap())
        }
    }

    #[tokio::test]
    async fn test_auth_service_full_flow() {
        let (engine, gateway_secret) = make_test_engine();
        let layer = AuthLayer::new(engine.clone());

        let mut svc = ServiceBuilder::new()
            .layer(layer)
            .service(service_fn(echo_user_handler));

        // Request without auth headers -> NO_CONTENT (204)
        let empty_req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.ready().await.unwrap().call(empty_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Request with valid Bearer token -> OK (200) with header
        let sub = Uuid::now_v7();
        let now = Utc::now().timestamp();
        let claims = AuthUserClaims {
            sub,
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: now + 300,
            nbf: now,
            iat: now,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
        };
        let token = engine.encode_claims(&claims).unwrap();
        let bearer_req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = svc.ready().await.unwrap().call(bearer_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("x-authenticated-user").unwrap(),
            &sub.to_string()
        );

        // Request with valid Gateway headers -> OK (200)
        let gw_user_id = Uuid::now_v7();
        let gw_user_id_str = gw_user_id.to_string();
        let ts = Utc::now().timestamp();
        let ctx = GatewayContext::new(&gw_user_id_str, None, "admin", ts);
        let sig = sign_gateway_context(&gateway_secret, &ctx).unwrap();
        let gw_req = Request::builder()
            .header(HEADER_USER_ID, &gw_user_id_str)
            .header(HEADER_USER_ROLES, "admin")
            .header(HEADER_OMNI_TIMESTAMP, ts.to_string())
            .header(HEADER_OMNI_SIGNATURE, sig)
            .body(Body::empty())
            .unwrap();
        let resp = svc.ready().await.unwrap().call(gw_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("x-authenticated-user").unwrap(),
            &gw_user_id.to_string()
        );

        // Request with invalid Bearer token -> UNAUTHORIZED (401)
        let bad_token_req = Request::builder()
            .header("Authorization", "Bearer invalid.jwt.token")
            .body(Body::empty())
            .unwrap();
        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(bad_token_req)
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
