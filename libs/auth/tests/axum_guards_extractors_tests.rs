//! Axum integration tests for Extractors, Guards, and `AuthLayer`.

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Request,
    http::{StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use chrono::Utc;
use omni_auth::{
    config::AuthConfig,
    extractor::{AuthenticatedUser, CurrentTenant, OptionalUser},
    guard::{require_permission, require_role},
    middleware::{
        AuthLayer, GatewayContext, HEADER_OMNI_SIGNATURE, HEADER_OMNI_TIMESTAMP, HEADER_TENANT_ID,
        HEADER_USER_ID, HEADER_USER_ROLES, sign_gateway_context,
    },
    revocation::{MemoryRevocationStore, RevocationStore},
    test_utils::ClaimsBuilder,
    token::{Role, TokenEngine},
};
use secrecy::SecretString;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn test_auth_config() -> AuthConfig {
    AuthConfig {
        enabled: true,
        jwt_secret: SecretString::from("secret_for_test_entropy_exceeding_32_bytes_ok"),
        ..Default::default()
    }
}

fn build_test_router(engine: TokenEngine, revocation: Arc<dyn RevocationStore>) -> Router {
    Router::new()
        // Public endpoint có OptionalUser
        .route(
            "/public/hello",
            get(|opt_user: OptionalUser| async move {
                match opt_user.0 {
                    Some(u) => format!("Hello user {}", u.user_id),
                    None => "Hello guest".to_string(),
                }
            }),
        )
        // Endpoint yêu cầu đăng nhập
        .route(
            "/api/profile",
            get(|user: AuthenticatedUser| async move {
                format!("User: {}", user.user_id)
            }),
        )
        // Endpoint yêu cầu Tenant
        .route(
            "/api/tenant-data",
            get(|_user: AuthenticatedUser, tenant: CurrentTenant| async move {
                format!("Tenant: {}", tenant.0)
            }),
        )
        // Endpoint yêu cầu vai trò Admin
        .route(
            "/api/admin/dashboard",
            get(|| async { "admin stats" }).route_layer(require_role(Role::Admin)),
        )
        // Endpoint yêu cầu quyền tạo đơn: "orders:create"
        .route(
            "/api/orders",
            post(|| async { "order created" }).route_layer(require_permission("orders:create")),
        )
        .layer(AuthLayer::new(engine).with_revocation(revocation))
}

#[tokio::test]
async fn test_unauthenticated_request_to_protected_route_returns_401() {
    let engine = TokenEngine::new(test_auth_config()).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine, revocation);

    let req = Request::builder()
        .uri("/api/profile")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_public_route_with_optional_user() {
    let engine = TokenEngine::new(test_auth_config()).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine, revocation);

    // Không gửi token -> Khách
    let req_guest = Request::builder()
        .uri("/public/hello")
        .body(Body::empty())
        .unwrap();
    let resp_guest = app.clone().oneshot(req_guest).await.unwrap();
    assert_eq!(resp_guest.status(), StatusCode::OK);
    let body = to_bytes(resp_guest.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"Hello guest");
}

#[tokio::test]
async fn test_authenticated_bearer_jwt_access_flow() {
    let engine = TokenEngine::new(test_auth_config()).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine.clone(), revocation);

    let user_id = Uuid::now_v7();
    let claims = ClaimsBuilder::new()
        .with_sub(user_id)
        .with_roles(vec![Role::User])
        .build();
    let token = engine.encode_claims(&claims).unwrap();

    let req = Request::builder()
        .uri("/api/profile")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_role_guard_allows_admin_rejects_user() {
    let engine = TokenEngine::new(test_auth_config()).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine.clone(), revocation);

    // 1. Regular user bị từ chối 403 Forbidden
    let user_claims = ClaimsBuilder::new().with_roles(vec![Role::User]).build();
    let user_token = engine.encode_claims(&user_claims).unwrap();

    let req_user = Request::builder()
        .uri("/api/admin/dashboard")
        .header(AUTHORIZATION, format!("Bearer {user_token}"))
        .body(Body::empty())
        .unwrap();

    let resp_user = app.clone().oneshot(req_user).await.unwrap();
    assert_eq!(resp_user.status(), StatusCode::FORBIDDEN);

    // 2. Admin được cho phép 200 OK
    let admin_claims = ClaimsBuilder::new().with_roles(vec![Role::Admin]).build();
    let admin_token = engine.encode_claims(&admin_claims).unwrap();

    let req_admin = Request::builder()
        .uri("/api/admin/dashboard")
        .header(AUTHORIZATION, format!("Bearer {admin_token}"))
        .body(Body::empty())
        .unwrap();

    let resp_admin = app.oneshot(req_admin).await.unwrap();
    assert_eq!(resp_admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_permission_guard_with_wildcard() {
    let engine = TokenEngine::new(test_auth_config()).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine.clone(), revocation);

    // User có quyền wildcard "orders:*" được cấp quyền tạo đơn
    let claims = ClaimsBuilder::new()
        .with_roles(vec![Role::User])
        .with_permissions(vec!["orders:*".to_string()])
        .build();
    let token = engine.encode_claims(&claims).unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/api/orders")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_gateway_signed_context_header_propagation_flow() {
    let config = AuthConfig {
        internal_gateway_secret: SecretString::from(
            "dev_internal_gateway_hmac_signing_key_32_bytes_minimum",
        ),
        ..test_auth_config()
    };
    let engine = TokenEngine::new(config).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine.clone(), revocation);

    let user_id = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let now = Utc::now().timestamp();

    let user_id_str = user_id.to_string();
    let tenant_id_str = tenant_id.to_string();
    let ctx = GatewayContext::new(&user_id_str, Some(&tenant_id_str), "admin", now);

    let sig = sign_gateway_context(&engine.config().internal_gateway_secret, &ctx).unwrap();

    let req = Request::builder()
        .uri("/api/admin/dashboard")
        .header(HEADER_USER_ID, user_id.to_string())
        .header(HEADER_TENANT_ID, tenant_id.to_string())
        .header(HEADER_USER_ROLES, "admin")
        .header(HEADER_OMNI_TIMESTAMP, now.to_string())
        .header(HEADER_OMNI_SIGNATURE, sig)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_revoked_token_is_rejected_by_auth_layer() {
    let engine = TokenEngine::new(test_auth_config()).unwrap();
    let revocation = Arc::new(MemoryRevocationStore::new());
    let app = build_test_router(engine.clone(), revocation.clone());

    let claims = ClaimsBuilder::new().build();
    let token = engine.encode_claims(&claims).unwrap();

    // Thu hồi token JTI trong Redis/RevocationStore
    revocation
        .revoke_token(claims.jti, claims.exp, "admin_action")
        .await
        .unwrap();

    let req = Request::builder()
        .uri("/api/profile")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
