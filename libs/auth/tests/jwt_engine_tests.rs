//! JWT `TokenEngine` and claims specification tests.

use omni_auth::{
    config::{AuthConfig, SupportedAlgorithm},
    error::AuthError,
    test_utils::ClaimsBuilder,
    token::{Role, TokenEngine, TokenType},
};
use secrecy::SecretString;
use uuid::Uuid;

fn test_auth_config() -> AuthConfig {
    AuthConfig {
        enabled: true,
        jwt_secret: SecretString::from("very_strong_secret_key_exceeding_32_bytes_entropy_123"),
        ..Default::default()
    }
}

#[test]
fn test_jwt_symmetric_encode_decode_roundtrip() {
    let engine = TokenEngine::new(test_auth_config()).expect("TokenEngine must initialize");

    let sub = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    let claims = ClaimsBuilder::new()
        .with_sub(sub)
        .with_tenant_id(Some(tenant_id))
        .with_session_id(Some(session_id))
        .with_role(Role::Admin)
        .with_permission("orders:create")
        .build();

    let token = engine.encode_claims(&claims).expect("Encode must succeed");
    assert_ne!(token, "");

    let decoded = engine.decode_token(&token).expect("Decode must succeed");
    assert_eq!(decoded.sub, sub);
    assert_eq!(decoded.tenant_id, Some(tenant_id));
    assert_eq!(decoded.session_id, Some(session_id));
    assert!(decoded.has_role(&Role::Admin));
    assert!(decoded.has_permission("orders:create"));
    assert!(!decoded.has_permission("orders:delete"));
}

#[test]
fn test_jwt_expired_token_rejection() {
    let config = AuthConfig {
        clock_skew_leeway_secs: 0,
        ..test_auth_config()
    };
    let engine = TokenEngine::new(config).expect("TokenEngine must initialize");

    let expired_claims = ClaimsBuilder::new().expired().build();
    let token = engine
        .encode_claims(&expired_claims)
        .expect("Encode must succeed");

    let result = engine.decode_token(&token);
    match result {
        Err(AuthError::TokenExpired) => (),
        other => panic!("Expected AuthError::TokenExpired, got: {other:?}"),
    }
}

#[test]
fn test_jwt_tampered_token_rejection() {
    let engine = TokenEngine::new(test_auth_config()).expect("TokenEngine must initialize");

    let claims = ClaimsBuilder::new().build();
    let token = engine.encode_claims(&claims).expect("Encode must succeed");

    // Sửa đổi 1 ký tự trong payload để giả lập tấn công can thiệp dữ liệu
    let mut parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let tampered_payload = format!("{}a", &parts[1][..parts[1].len() - 1]);
    parts[1] = &tampered_payload;
    let tampered_token = parts.join(".");

    let result = engine.decode_token(&tampered_token);
    match result {
        Err(AuthError::InvalidSignature | AuthError::MalformedToken(_)) => (),
        other => panic!("Expected signature/format rejection, got: {other:?}"),
    }
}

#[test]
fn test_jwt_wildcard_permission_matching() {
    let mut claims = ClaimsBuilder::new()
        .with_roles(vec![Role::User])
        .with_permissions(vec!["orders:*".to_string(), "payments:read".to_string()])
        .build();

    assert!(claims.has_permission("orders:create"));
    assert!(claims.has_permission("orders:delete"));
    assert!(claims.has_permission("payments:read"));
    assert!(!claims.has_permission("payments:refund"));

    // SuperAdmin vượt qua mọi kiểm tra quyền
    claims.roles = vec![Role::SuperAdmin];
    assert!(claims.has_permission("anything:at:all"));
    assert!(claims.has_role(&Role::Admin));
}

#[test]
fn test_generate_token_pair() {
    let config = AuthConfig {
        access_token_ttl_secs: 900,
        refresh_token_ttl_secs: 604_800,
        ..test_auth_config()
    };
    let engine = TokenEngine::new(config).expect("TokenEngine must initialize");

    let sub = Uuid::now_v7();
    let pair = engine
        .generate_token_pair(
            sub,
            Some(Uuid::now_v7()),
            Some(Uuid::now_v7()),
            vec![Role::User],
            vec!["profile:read".to_string()],
            Some("test@example.com".to_string()),
        )
        .expect("generate_token_pair must succeed");

    assert_eq!(pair.token_type, "Bearer");
    assert_eq!(pair.expires_in, 900);

    let access_claims = engine
        .decode_token(&pair.access_token)
        .expect("Access decode ok");
    assert_eq!(access_claims.token_type, TokenType::Access);
    assert_eq!(access_claims.sub, sub);

    let refresh_claims = engine
        .decode_token(&pair.refresh_token)
        .expect("Refresh decode ok");
    assert_eq!(refresh_claims.token_type, TokenType::Refresh);
    assert_eq!(refresh_claims.sub, sub);
}

#[test]
fn test_jwt_algorithms_hs384_and_hs512() {
    for alg in [SupportedAlgorithm::HS384, SupportedAlgorithm::HS512] {
        let config = AuthConfig {
            enabled: true,
            jwt_algorithm: alg,
            jwt_secret: SecretString::from(
                "very_strong_secret_key_exceeding_64_bytes_entropy_1234567890abcdef",
            ),
            ..Default::default()
        };
        let engine = TokenEngine::new(config).expect("TokenEngine must initialize");

        let claims = ClaimsBuilder::new().build();
        let token = engine.encode_claims(&claims).expect("Encode ok");
        let decoded = engine.decode_token(&token).expect("Decode ok");
        assert_eq!(decoded.sub, claims.sub);
    }
}
