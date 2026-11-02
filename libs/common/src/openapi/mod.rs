//! `OpenAPI` 3.1 & API Documentation Standards for `OmniKubernetes`.
//!
//! Provides platform tools for:
//! - Standard `BearerAuth` security scheme ([`SecurityAddon`]).
//! - Auto-registration of common response & pagination schemas ([`CommonSchemaAddon`]).
//! - Centralized Gateway Aggregator Catalog configuration ([`SwaggerConfig`]).
//! - Zero-HTML Documentation endpoints with Forwarded-Prefix awareness ([`create_docs_router`]).

pub mod catalog;
pub mod router;
pub mod schemas;
pub mod security;

pub use catalog::{
    SwaggerConfig, SwaggerConfigBuilder, SwaggerServiceUrl, create_swagger_config_router,
};
pub use router::{
    DocsConfig, DocsConfigBuilder, create_docs_router, create_docs_router_with_config,
    resolve_openapi_servers,
};
pub use schemas::{CommonSchemaAddon, register_common_schemas};
pub use security::{BEARER_AUTH_SECURITY_SCHEME, SecurityAddon};
