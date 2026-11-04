//! Automated `OpenAPI` Specification Router & Endpoints.
//!
//! Exposes runtime `OpenAPI` specs at internal endpoints `/v3/api-docs` and `/v3/api-docs/openapi.json`.
//! Implements strict Forwarded-Prefix awareness to resolve Pitfall 1 (Try It Out 404).
//! Adheres to the Zero-HTML backend policy (Section 4.1.1) while providing optional Swagger UI
//! for isolated local debugging when enabled.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::get,
};
use utoipa::openapi::{OpenApi, ServerBuilder};

pub const X_FORWARDED_PREFIX_HEADER: &str = "x-forwarded-prefix";
pub const DEFAULT_SPEC_PATH: &str = "/v3/api-docs";
pub const DEFAULT_SPEC_JSON_PATH: &str = "/v3/api-docs/openapi.json";
pub const DEFAULT_SWAGGER_UI_PATH: &str = "/swagger-ui";

/// Configuration for the API Documentation Router.
#[derive(Debug, Clone)]
pub struct DocsConfig {
    /// Canonical service name (e.g. `"order-service"`).
    pub service_name: Option<String>,
    /// Default relative routing prefix when called through Gateway (e.g. `"/api/v1/orders"`).
    pub service_prefix: Option<String>,
    /// Primary `OpenAPI` spec JSON path (defaults to `"/v3/api-docs"`).
    pub spec_path: String,
    /// Path for Swagger UI if enabled (defaults to `"/swagger-ui"`).
    pub swagger_ui_path: String,
    /// Whether to embed Swagger UI HTML in this service process.
    ///
    /// # Architecture Rule (Section 4.1.1)
    /// **Strictly `false` by default** on backend pods to save 20-50MB RAM and eliminate attack surface.
    /// May be set to `true` exclusively for local offline testing.
    pub enable_swagger_ui: bool,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            service_name: None,
            service_prefix: None,
            spec_path: DEFAULT_SPEC_PATH.to_string(),
            swagger_ui_path: DEFAULT_SWAGGER_UI_PATH.to_string(),
            enable_swagger_ui: false,
        }
    }
}

impl DocsConfig {
    /// Creates a new default [`DocsConfig`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Fluent builder for [`DocsConfig`].
    pub fn builder() -> DocsConfigBuilder {
        DocsConfigBuilder::default()
    }

    /// Configures the service name and Gateway path prefix.
    #[must_use]
    pub fn with_service_prefix(
        mut self,
        service_name: impl Into<String>,
        service_prefix: impl Into<String>,
    ) -> Self {
        self.service_name = Some(service_name.into());
        self.service_prefix = Some(service_prefix.into());
        self
    }
}

/// Builder for [`DocsConfig`].
#[derive(Debug, Default)]
pub struct DocsConfigBuilder {
    service_name: Option<String>,
    service_prefix: Option<String>,
    spec_path: Option<String>,
    swagger_ui_path: Option<String>,
    enable_swagger_ui: Option<bool>,
}

impl DocsConfigBuilder {
    #[must_use]
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn service_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.service_prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn spec_path(mut self, path: impl Into<String>) -> Self {
        self.spec_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn swagger_ui_path(mut self, path: impl Into<String>) -> Self {
        self.swagger_ui_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn enable_swagger_ui(mut self, enabled: bool) -> Self {
        self.enable_swagger_ui = Some(enabled);
        self
    }

    pub fn build(self) -> DocsConfig {
        DocsConfig {
            service_name: self.service_name,
            service_prefix: self.service_prefix,
            spec_path: self
                .spec_path
                .unwrap_or_else(|| DEFAULT_SPEC_PATH.to_string()),
            swagger_ui_path: self
                .swagger_ui_path
                .unwrap_or_else(|| DEFAULT_SWAGGER_UI_PATH.to_string()),
            enable_swagger_ui: self.enable_swagger_ui.unwrap_or(false),
        }
    }
}

#[derive(Clone)]
struct DocsRouterState {
    openapi: Arc<OpenApi>,
    config: DocsConfig,
}

/// Adjusts the `servers` field of an [`OpenApi`] specification based on Gateway forward headers or configuration.
///
/// Resolves Pitfall 1: Bẫy 404 khi bấm "Try It Out" do sai tiền tố định tuyến Gateway.
pub fn resolve_openapi_servers(
    mut openapi: OpenApi,
    headers: &HeaderMap,
    config_prefix: Option<&str>,
) -> OpenApi {
    // 1. Prioritize X-Forwarded-Prefix injected by Ingress (RFC 7239 / NGINX Ingress Strategy A)
    let dynamic_prefix = headers
        .get(X_FORWARDED_PREFIX_HEADER)
        .and_then(|val| val.to_str().ok())
        .map(|s| s.trim_end_matches('/'))
        .filter(|s| !s.is_empty());

    let target_prefix = dynamic_prefix.or(config_prefix);

    if let Some(prefix) = target_prefix {
        let servers = openapi.servers.get_or_insert_with(Vec::new);
        // Prepend target prefix if not already present as first server
        if servers.first().is_none_or(|s| s.url != prefix) {
            let server = ServerBuilder::new()
                .url(prefix)
                .description(Some("Through OmniKubernetes Ingress Gateway"))
                .build();
            servers.insert(0, server);
        }
    }

    openapi
}

async fn handle_spec_request(
    State(state): State<DocsRouterState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let resolved_spec = resolve_openapi_servers(
        (*state.openapi).clone(),
        &headers,
        state.config.service_prefix.as_deref(),
    );

    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        Json(resolved_spec),
    )
}

/// Creates an Axum [`Router`] serving `OpenAPI` documentation specs at:
/// - `GET /v3/api-docs`
/// - `GET /v3/api-docs/openapi.json`
///
/// Zero Swagger UI HTML is served (safe & lightweight).
pub fn create_docs_router(openapi: OpenApi) -> Router {
    create_docs_router_with_config(openapi, DocsConfig::default())
}

/// Creates an Axum [`Router`] serving `OpenAPI` documentation specs with custom [`DocsConfig`].
pub fn create_docs_router_with_config(openapi: OpenApi, config: DocsConfig) -> Router {
    let spec_path = config.spec_path.clone();
    let spec_json_path = if std::path::Path::new(&spec_path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        spec_path.clone()
    } else {
        format!("{}/openapi.json", spec_path.trim_end_matches('/'))
    };

    #[cfg(feature = "swagger-ui")]
    let (swagger_ui_path, enable_swagger_ui) =
        (config.swagger_ui_path.clone(), config.enable_swagger_ui);

    let state = DocsRouterState {
        openapi: Arc::new(openapi),
        config,
    };

    let mut router = Router::new().route(&spec_path, get(handle_spec_request));

    if spec_json_path != spec_path {
        router = router.route(&spec_json_path, get(handle_spec_request));
    }

    #[allow(unused_mut)]
    let mut router = router.with_state(state);

    #[cfg(feature = "swagger-ui")]
    if enable_swagger_ui {
        use utoipa_swagger_ui::{Config, SwaggerUi};
        let static_spec_path: &'static str = Box::leak(spec_path.into_boxed_str());
        let static_ui_path: &'static str = Box::leak(swagger_ui_path.into_boxed_str());
        let ui_config = Config::new([static_spec_path])
            .persist_authorization(true)
            .display_request_duration(true)
            .filter(true);
        router = router.merge(SwaggerUi::new(static_ui_path).config(ui_config));
    }

    router
}
