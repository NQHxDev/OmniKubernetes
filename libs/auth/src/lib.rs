//! # `OmniKubernetes` Identity & Security Context Platform (`omni-auth`).
//!
//! Provides zero-trust defense-in-depth, cryptographic token verification, OWASP Argon2id password hashing,
//! real-time distributed revocation with JTI Blacklists, and declarative Axum 0.8 guards and extractors.
//!
//! ## Core Architecture Principles
//! 1. **Zero-Trust Internal Mesh:** All internal context headers (`X-User-Id`, `X-Tenant-Id`) require HMAC-SHA256 signatures from API Gateway.
//! 2. **Zero-Leak Credentials:** All passwords, private keys, and HMAC secrets are memory-zeroized via [`secrecy::SecretString`].
//! 3. **Stateless by Default, Statefully Revocable:** Stateless JWT claims with sub-millisecond Redis JTI Blacklist checks.
//! 4. **Strict Type-Safe Claims:** RFC 7519 / RFC 9562 time-ordered `UUIDv7` subject, session, and tenant identifiers.
//! 5. **Declarative Axum Guards:** Route-level authorization (`require_role`, `require_permission`) and extractors (`AuthenticatedUser`, `CurrentTenant`).

pub mod bridge;
pub mod config;
pub mod crypto;
pub mod error;
#[cfg(feature = "axum")]
pub mod extractor;
#[cfg(feature = "axum")]
pub mod guard;
pub mod instrument;
#[cfg(feature = "axum")]
pub mod middleware;
pub mod revocation;
#[cfg(any(test, feature = "test-util"))]
pub mod test_utils;
pub mod token;

// Re-exports for primary public API
#[cfg(feature = "db-session")]
pub use bridge::create_session_context;
pub use bridge::{enrich_request_context, enrich_tracing_span};

pub use config::{
    Argon2Config, AsymmetricConfig, AuthConfig, AuthKeyMode, JwksConfig, JwtAlgorithm,
    SupportedAlgorithm,
};
pub use crypto::PasswordSecurity;
pub use error::{
    AuthError, AuthResult, ERR_AUTH_INSUFFICIENT_PERMS, ERR_AUTH_INSUFFICIENT_ROLES,
    ERR_AUTH_INVALID_CREDENTIALS, ERR_AUTH_INVALID_SIGNATURE, ERR_AUTH_MALFORMED_TOKEN,
    ERR_AUTH_MISSING_CREDENTIALS, ERR_AUTH_SIGNATURE_EXPIRED, ERR_AUTH_TENANT_MISMATCH,
    ERR_AUTH_TOKEN_EXPIRED, ERR_AUTH_TOKEN_REVOKED, ERR_AUTH_TOKEN_THEFT_DETECTED,
};

#[cfg(feature = "axum")]
pub use extractor::{AuthenticatedUser, BearerToken, CurrentTenant, OptionalUser};

#[cfg(feature = "axum")]
pub use guard::{
    RequireAllPermissionsLayer, RequireAnyRoleLayer, RequirePermissionLayer, RequireRoleLayer,
    require_all_permissions, require_any_role, require_permission, require_role,
};

#[cfg(feature = "axum")]
pub use middleware::{
    AuthLayer, AuthService, GatewayContext, HEADER_OMNI_SIGNATURE, HEADER_OMNI_TIMESTAMP,
    HEADER_TENANT_ID, HEADER_USER_ID, HEADER_USER_ROLES, sign_gateway_context,
    verify_gateway_signature,
};

#[cfg(feature = "revocation")]
pub use revocation::CacheRevocationStore;
pub use revocation::{
    MemoryRevocationStore, MemoryTokenFamilyStore, RefreshTokenRecord, RevocationEntry,
    RevocationStore, TokenFamilyStore,
};

#[cfg(any(test, feature = "test-util"))]
pub use test_utils::{ClaimsBuilder, MockAuthLayer, create_test_token, create_test_token_engine};

pub use token::{AuthUserClaims, Role, TokenEngine, TokenPair, TokenType};
