//! `JsonWebToken` encode, decode, and key management engine.

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use secrecy::ExposeSecret;
use uuid::Uuid;

use crate::{
    config::{AuthConfig, SupportedAlgorithm},
    error::AuthError,
    instrument::CryptoTimer,
    token::{Role, TokenType, claims::AuthUserClaims, pair::TokenPair},
};

/// Động cơ ký và thẩm định JWT chuẩn RFC 7515/7519 với đa thuật toán.
#[derive(Clone)]
pub struct TokenEngine {
    config: AuthConfig,
    encoding_key: Option<EncodingKey>,
    decoding_key: DecodingKey,
    validation: Validation,
    algorithm: Algorithm,
}

impl std::fmt::Debug for TokenEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenEngine")
            .field("config", &self.config)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

impl TokenEngine {
    /// Khởi tạo [`TokenEngine`] mới từ cấu hình phân hệ Auth.
    pub fn new(config: AuthConfig) -> Result<Self, AuthError> {
        let (encoding_key, decoding_key, algorithm) = match config.jwt_algorithm {
            SupportedAlgorithm::HS256 => init_symmetric_keys(&config, Algorithm::HS256),
            SupportedAlgorithm::HS384 => init_symmetric_keys(&config, Algorithm::HS384),
            SupportedAlgorithm::HS512 => init_symmetric_keys(&config, Algorithm::HS512),
            SupportedAlgorithm::RS256
            | SupportedAlgorithm::EdDSA
            | SupportedAlgorithm::ES256
            | SupportedAlgorithm::ES384
            | SupportedAlgorithm::ES512 => init_asymmetric_keys(&config)?,
        };

        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[&config.issuer]);
        validation.set_audience(&[&config.audience]);
        validation.leeway = config.clock_skew_leeway_secs;
        validation.validate_exp = true;
        validation.validate_nbf = true;

        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            validation,
            algorithm,
        })
    }

    /// Trả về tham chiếu tới cấu hình [`AuthConfig`].
    #[must_use]
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// Ký một tập claims thành chuỗi JWT (Compact Serialization).
    pub fn encode_claims(&self, claims: &AuthUserClaims) -> Result<String, AuthError> {
        let timer = CryptoTimer::start("jwt_encode");
        let enc_key = self.encoding_key.as_ref().ok_or_else(|| {
            AuthError::CryptoConfiguration(
                "Cannot encode claims: private key or secret not configured".into(),
            )
        })?;

        let mut header = Header::new(self.algorithm);
        header.kid = Some(self.config.jwt_key_id.clone());

        let res = encode(&header, claims, enc_key)
            .map_err(|e| AuthError::TokenEncodingFailed(e.to_string()));
        timer.complete();
        res
    }

    /// Giải mã và thẩm định chữ ký số, thời hạn (`exp`, `nbf`), `iss` và `aud` của JWT.
    pub fn decode_token(&self, token: &str) -> Result<AuthUserClaims, AuthError> {
        let timer = CryptoTimer::start("jwt_decode");
        let res = decode::<AuthUserClaims>(token, &self.decoding_key, &self.validation)
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                jsonwebtoken::errors::ErrorKind::InvalidSignature => AuthError::InvalidSignature,
                jsonwebtoken::errors::ErrorKind::ImmatureSignature => AuthError::TokenNotYetValid,
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => AuthError::InvalidIssuer,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => AuthError::InvalidAudience,
                _ => AuthError::MalformedToken(e.to_string()),
            });
        timer.complete();
        res
    }

    /// Sinh cặp Access Token và Refresh Token hoàn chỉnh theo chuẩn `UUIDv7`.
    pub fn generate_token_pair(
        &self,
        sub: Uuid,
        tenant_id: Option<Uuid>,
        session_id: Option<Uuid>,
        roles: Vec<Role>,
        permissions: Vec<String>,
        email: Option<String>,
    ) -> Result<TokenPair, AuthError> {
        let now = Utc::now().timestamp();
        let access_ttl = i64::try_from(self.config.access_token_ttl_secs).unwrap_or(i64::MAX);
        let refresh_ttl = i64::try_from(self.config.refresh_token_ttl_secs).unwrap_or(i64::MAX);
        let access_exp = now.saturating_add(access_ttl);
        let refresh_exp = now.saturating_add(refresh_ttl);

        let access_jti = Uuid::now_v7();
        let refresh_jti = Uuid::now_v7();

        let access_claims = AuthUserClaims {
            sub,
            iss: self.config.issuer.clone(),
            aud: self.config.audience.clone(),
            exp: access_exp,
            nbf: now,
            iat: now,
            jti: access_jti,
            token_type: TokenType::Access,
            tenant_id,
            session_id,
            roles: roles.clone(),
            permissions: permissions.clone(),
            email: email.clone(),
        };

        let refresh_claims = AuthUserClaims {
            sub,
            iss: self.config.issuer.clone(),
            aud: self.config.audience.clone(),
            exp: refresh_exp,
            nbf: now,
            iat: now,
            jti: refresh_jti,
            token_type: TokenType::Refresh,
            tenant_id,
            session_id,
            roles,
            permissions,
            email,
        };

        let access_token = self.encode_claims(&access_claims)?;
        let refresh_token = self.encode_claims(&refresh_claims)?;

        Ok(TokenPair::new(
            access_token,
            refresh_token,
            self.config.access_token_ttl_secs,
        ))
    }
}

fn init_symmetric_keys(
    config: &AuthConfig,
    algo: Algorithm,
) -> (Option<EncodingKey>, DecodingKey, Algorithm) {
    let secret = config.jwt_secret.expose_secret().as_bytes();
    (
        Some(EncodingKey::from_secret(secret)),
        DecodingKey::from_secret(secret),
        algo,
    )
}

fn init_asymmetric_keys(
    config: &AuthConfig,
) -> Result<(Option<EncodingKey>, DecodingKey, Algorithm), AuthError> {
    match config.jwt_algorithm {
        SupportedAlgorithm::RS256 => {
            let enc = if let Some(ref priv_pem) = config.asymmetric.private_key_pem {
                Some(
                    EncodingKey::from_rsa_pem(priv_pem.expose_secret().as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                )
            } else {
                None
            };
            let pub_pem = config.asymmetric.public_key_pem.as_ref().ok_or_else(|| {
                AuthError::CryptoConfiguration("RS256 requires public_key_pem".into())
            })?;
            let dec = DecodingKey::from_rsa_pem(pub_pem.as_bytes())
                .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
            Ok((enc, dec, Algorithm::RS256))
        }
        SupportedAlgorithm::EdDSA => {
            let enc = if let Some(ref priv_pem) = config.asymmetric.private_key_pem {
                Some(
                    EncodingKey::from_ed_pem(priv_pem.expose_secret().as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                )
            } else {
                None
            };
            let pub_pem = config.asymmetric.public_key_pem.as_ref().ok_or_else(|| {
                AuthError::CryptoConfiguration("EdDSA requires public_key_pem".into())
            })?;
            let dec = DecodingKey::from_ed_pem(pub_pem.as_bytes())
                .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
            Ok((enc, dec, Algorithm::EdDSA))
        }
        SupportedAlgorithm::ES256 => {
            let enc = if let Some(ref priv_pem) = config.asymmetric.private_key_pem {
                Some(
                    EncodingKey::from_ec_pem(priv_pem.expose_secret().as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                )
            } else {
                None
            };
            let pub_pem = config.asymmetric.public_key_pem.as_ref().ok_or_else(|| {
                AuthError::CryptoConfiguration("ES256 requires public_key_pem".into())
            })?;
            let dec = DecodingKey::from_ec_pem(pub_pem.as_bytes())
                .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
            Ok((enc, dec, Algorithm::ES256))
        }
        SupportedAlgorithm::ES384 => {
            let enc = if let Some(ref priv_pem) = config.asymmetric.private_key_pem {
                Some(
                    EncodingKey::from_ec_pem(priv_pem.expose_secret().as_bytes())
                        .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?,
                )
            } else {
                None
            };
            let pub_pem = config.asymmetric.public_key_pem.as_ref().ok_or_else(|| {
                AuthError::CryptoConfiguration("ES384 requires public_key_pem".into())
            })?;
            let dec = DecodingKey::from_ec_pem(pub_pem.as_bytes())
                .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
            Ok((enc, dec, Algorithm::ES384))
        }
        SupportedAlgorithm::ES512 => Err(AuthError::CryptoConfiguration(
            "ES512 (P-521) is not supported by current crypto provider; use ES256, ES384, or EdDSA"
                .into(),
        )),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn test_config(algo: SupportedAlgorithm) -> AuthConfig {
        AuthConfig {
            enabled: true,
            jwt_algorithm: algo,
            jwt_secret: SecretString::from(
                "test_super_secret_signing_key_at_least_32_bytes_entropy_123456789",
            ),
            issuer: "urn:omni:auth".to_string(),
            audience: "urn:omni:api".to_string(),
            access_token_ttl_secs: 900,
            refresh_token_ttl_secs: 604_800,
            clock_skew_leeway_secs: 10,
            ..Default::default()
        }
    }

    #[test]
    fn test_token_engine_hs256_roundtrip() {
        let config = test_config(SupportedAlgorithm::HS256);
        let engine = TokenEngine::new(config).expect("HS256 init failed");

        let sub = Uuid::now_v7();
        let now = Utc::now().timestamp();
        let claims = AuthUserClaims {
            sub,
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: now + 300,
            nbf: now - 10,
            iat: now,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: Some(Uuid::now_v7()),
            session_id: Some(Uuid::now_v7()),
            roles: vec![Role::User, Role::Admin],
            permissions: vec!["orders:read".to_string()],
            email: Some("user@omni.internal".to_string()),
        };

        let token = engine.encode_claims(&claims).expect("Encode failed");
        let decoded = engine.decode_token(&token).expect("Decode failed");
        assert_eq!(decoded.sub, sub);
        assert_eq!(decoded.roles, vec![Role::User, Role::Admin]);
        assert_eq!(decoded.permissions, vec!["orders:read".to_string()]);
    }

    #[test]
    fn test_token_engine_algorithms_hs384_and_hs512() {
        for algo in [SupportedAlgorithm::HS384, SupportedAlgorithm::HS512] {
            let config = test_config(algo);
            let engine = TokenEngine::new(config).expect("Engine init failed");

            let now = Utc::now().timestamp();
            let claims = AuthUserClaims {
                sub: Uuid::now_v7(),
                iss: "urn:omni:auth".to_string(),
                aud: "urn:omni:api".to_string(),
                exp: now + 300,
                nbf: now - 10,
                iat: now,
                jti: Uuid::now_v7(),
                token_type: TokenType::Access,
                tenant_id: None,
                session_id: None,
                roles: vec![Role::SuperAdmin],
                permissions: vec!["*".to_string()],
                email: None,
            };

            let token = engine.encode_claims(&claims).expect("Encode failed");
            let decoded = engine.decode_token(&token).expect("Decode failed");
            assert_eq!(decoded.sub, claims.sub);
            assert!(decoded.is_super_admin());
        }
    }

    #[test]
    fn test_token_engine_expired_token() {
        let config = test_config(SupportedAlgorithm::HS256);
        let engine = TokenEngine::new(config).unwrap();

        let now = Utc::now().timestamp();
        let claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: now - 60, // expired 60 seconds ago
            nbf: now - 120,
            iat: now - 120,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![Role::User],
            permissions: vec![],
            email: None,
        };

        let token = engine.encode_claims(&claims).unwrap();
        let res = engine.decode_token(&token);
        match res {
            Err(AuthError::TokenExpired) => {}
            other => panic!("Expected AuthError::TokenExpired, got {other:?}"),
        }
    }

    #[test]
    fn test_token_engine_issuer_and_audience_mismatch() {
        let config = test_config(SupportedAlgorithm::HS256);
        let engine = TokenEngine::new(config).unwrap();
        let now = Utc::now().timestamp();

        // Mismatched issuer
        let mut claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:other:issuer".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: now + 300,
            nbf: now,
            iat: now,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![],
            permissions: vec![],
            email: None,
        };
        let token = engine.encode_claims(&claims).unwrap();
        match engine.decode_token(&token) {
            Err(AuthError::InvalidIssuer) => {}
            other => panic!("Expected InvalidIssuer, got {other:?}"),
        }

        // Mismatched audience
        claims.iss = "urn:omni:auth".to_string();
        claims.aud = "urn:other:aud".to_string();
        let token = engine.encode_claims(&claims).unwrap();
        match engine.decode_token(&token) {
            Err(AuthError::InvalidAudience) => {}
            other => panic!("Expected InvalidAudience, got {other:?}"),
        }
    }

    #[test]
    fn test_token_engine_invalid_signature() {
        let config1 = test_config(SupportedAlgorithm::HS256);
        let mut config2 = test_config(SupportedAlgorithm::HS256);
        config2.jwt_secret =
            SecretString::from("completely_different_secret_key_at_least_32_bytes_entropy");

        let engine1 = TokenEngine::new(config1).unwrap();
        let engine2 = TokenEngine::new(config2).unwrap();
        let now = Utc::now().timestamp();

        let claims = AuthUserClaims {
            sub: Uuid::now_v7(),
            iss: "urn:omni:auth".to_string(),
            aud: "urn:omni:api".to_string(),
            exp: now + 300,
            nbf: now,
            iat: now,
            jti: Uuid::now_v7(),
            token_type: TokenType::Access,
            tenant_id: None,
            session_id: None,
            roles: vec![],
            permissions: vec![],
            email: None,
        };

        let token = engine1.encode_claims(&claims).unwrap();
        match engine2.decode_token(&token) {
            Err(AuthError::InvalidSignature) => {}
            other => panic!("Expected InvalidSignature, got {other:?}"),
        }

        // Also test truly malformed token
        match engine1.decode_token("not.a.valid.jwt.string") {
            Err(AuthError::MalformedToken(_)) => {}
            other => panic!("Expected MalformedToken, got {other:?}"),
        }
    }

    #[test]
    fn test_token_engine_generate_token_pair() {
        let config = test_config(SupportedAlgorithm::HS256);
        let engine = TokenEngine::new(config).unwrap();

        let sub = Uuid::now_v7();
        let tenant_id = Some(Uuid::now_v7());
        let session_id = Some(Uuid::now_v7());
        let roles = vec![Role::Admin];
        let permissions = vec!["clusters:read".to_string()];
        let email = Some("admin@example.com".to_string());

        let pair = engine
            .generate_token_pair(
                sub,
                tenant_id,
                session_id,
                roles.clone(),
                permissions.clone(),
                email.clone(),
            )
            .expect("generate_token_pair failed");

        assert_eq!(pair.token_type, "Bearer");
        assert_eq!(pair.expires_in, 900);

        let access = engine.decode_token(&pair.access_token).unwrap();
        assert_eq!(access.sub, sub);
        assert_eq!(access.token_type, TokenType::Access);
        assert_eq!(access.tenant_id, tenant_id);
        assert_eq!(access.session_id, session_id);
        assert_eq!(access.roles, roles);
        assert_eq!(access.permissions, permissions);
        assert_eq!(access.email, email);

        let refresh = engine.decode_token(&pair.refresh_token).unwrap();
        assert_eq!(refresh.sub, sub);
        assert_eq!(refresh.token_type, TokenType::Refresh);
        assert_eq!(refresh.tenant_id, tenant_id);
    }

    #[test]
    fn test_token_engine_unsupported_asymmetric() {
        let mut config = test_config(SupportedAlgorithm::ES512);
        config.jwt_algorithm = SupportedAlgorithm::ES512;
        let res = TokenEngine::new(config);
        match res {
            Err(AuthError::CryptoConfiguration(msg)) => {
                assert!(msg.contains("ES512"));
            }
            other => panic!("Expected CryptoConfiguration error for ES512, got {other:?}"),
        }
    }

    #[test]
    fn test_token_engine_debug_and_config() {
        let config = test_config(SupportedAlgorithm::HS256);
        let engine = TokenEngine::new(config.clone()).unwrap();
        let debug_str = format!("{engine:?}");
        assert!(debug_str.contains("TokenEngine"));
        assert_eq!(engine.config().issuer, config.issuer);
    }
}
