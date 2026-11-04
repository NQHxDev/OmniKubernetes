//! Enterprise Security Schemes for `OpenAPI` Specification.
//!
//! Standardizes authentication contracts across all `OmniKubernetes` microservices.
//! Adheres strictly to Section 6 (Pitfall 3) of the Enterprise API Documentation Specification:
//! - Security Scheme Identifier: `"BearerAuth"`
//! - Type: `http`
//! - Scheme: `bearer`
//! - Format: `JWT`
//! - Enables JWT token persistence across service dropdown transitions in Swagger UI.

use utoipa::{
    Modify,
    openapi::{
        OpenApi,
        security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    },
};

/// Canonical Security Scheme Name used across all `OmniKubernetes` services.
pub const BEARER_AUTH_SECURITY_SCHEME: &str = "BearerAuth";

/// `OpenAPI` Modifier for registering the canonical [`BEARER_AUTH_SECURITY_SCHEME`].
///
/// Attaches the `BearerAuth` HTTP Bearer JWT security scheme to `components.securitySchemes`.
///
/// # Usage
/// ```rust,ignore
/// use omni_common::openapi::SecurityAddon;
/// use utoipa::OpenApi;
///
/// #[derive(OpenApi)]
/// #[openapi(
///     paths(...),
///     modifiers(&SecurityAddon)
/// )]
/// pub struct ApiDoc;
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BEARER_AUTH_SECURITY_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "Enter your JWT access token (e.g. Bearer eyJhbGci...)",
                    ))
                    .build(),
            ),
        );
    }
}
