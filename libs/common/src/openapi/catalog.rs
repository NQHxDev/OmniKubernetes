//! Centralized Swagger UI Aggregator Configuration Catalog.
//!
//! Implements the Enterprise Gateway Service Catalog defined in Section 5.1
//! of `docs/architecture/api-documentation-architecture.md`.
//!
//! Provides the data contract and router for `/swagger-config.json`, enabling
//! a single Swagger UI to dynamically switch between all backend microservices
//! via a dropdown menu while preserving JWT Bearer authentication tokens.

use std::sync::Arc;

use axum::{Json, Router, http::header, response::IntoResponse, routing::get};
use serde::{Deserialize, Serialize};

fn default_layout() -> String {
    "StandaloneLayout".to_string()
}

const fn default_true() -> bool {
    true
}

/// Metadata entry for a single microservice `OpenAPI` documentation spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwaggerServiceUrl {
    /// Human-readable label displayed in the Swagger UI dropdown selector.
    pub name: String,
    /// Absolute or relative path to the microservice `OpenAPI` spec (e.g. `"/api/v1/auth/v3/api-docs"`).
    ///
    /// # Architecture Rule (Section 6 Pitfall 2)
    /// Must ALWAYS use relative paths on the Gateway domain to prevent cross-origin CORS errors.
    pub url: String,
}

impl SwaggerServiceUrl {
    /// Creates a new service URL entry.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
        }
    }
}

/// Centralized Gateway Swagger UI Configuration.
///
/// Serialized strictly into `swagger-config.json` for consumption by the Swagger UI Aggregator.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwaggerConfig {
    /// List of microservice `OpenAPI` documentation endpoints.
    pub urls: Vec<SwaggerServiceUrl>,
    /// Swagger UI layout style (defaults to `"StandaloneLayout"`).
    #[serde(default = "default_layout")]
    pub layout: String,
    /// Enables tag and operation deep linking in browser URL.
    #[serde(default = "default_true")]
    pub deep_linking: bool,
    /// Preserves Authorization tokens across service transitions in the dropdown selector.
    #[serde(default = "default_true")]
    pub persist_authorization: bool,
    /// Displays response latency (ms) for "Try It Out" requests.
    #[serde(default = "default_true")]
    pub display_request_duration: bool,
    /// Enables the instant filter/search input box on operations.
    #[serde(default = "default_true")]
    pub filter: bool,
}

impl Default for SwaggerConfig {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            layout: default_layout(),
            deep_linking: true,
            persist_authorization: true,
            display_request_duration: true,
            filter: true,
        }
    }
}

impl SwaggerConfig {
    /// Creates a new empty [`SwaggerConfig`].
    pub fn new(urls: Vec<SwaggerServiceUrl>) -> Self {
        Self {
            urls,
            ..Default::default()
        }
    }

    /// Fluent builder for [`SwaggerConfig`].
    pub fn builder() -> SwaggerConfigBuilder {
        SwaggerConfigBuilder::default()
    }

    /// Pre-populates the default enterprise catalog for the standard `OmniKubernetes` microservices.
    pub fn default_enterprise() -> Self {
        Self {
            urls: vec![
                SwaggerServiceUrl::new("01. Auth & IAM Service", "/api/v1/auth/v3/api-docs"),
                SwaggerServiceUrl::new("02. User & Tenant Service", "/api/v1/users/v3/api-docs"),
                SwaggerServiceUrl::new(
                    "03. Order Processing Service",
                    "/api/v1/orders/v3/api-docs",
                ),
                SwaggerServiceUrl::new(
                    "04. Payment & Ledger Service",
                    "/api/v1/payments/v3/api-docs",
                ),
                SwaggerServiceUrl::new(
                    "05. Product & Catalog Service",
                    "/api/v1/products/v3/api-docs",
                ),
                SwaggerServiceUrl::new(
                    "06. Notification Service",
                    "/api/v1/notifications/v3/api-docs",
                ),
            ],
            layout: default_layout(),
            deep_linking: true,
            persist_authorization: true,
            display_request_duration: true,
            filter: true,
        }
    }

    /// Appends a microservice endpoint to the catalog.
    #[must_use]
    pub fn with_service(mut self, name: impl Into<String>, url: impl Into<String>) -> Self {
        self.urls.push(SwaggerServiceUrl::new(name, url));
        self
    }

    /// Adds a microservice endpoint to the existing catalog.
    pub fn add_service(&mut self, name: impl Into<String>, url: impl Into<String>) {
        self.urls.push(SwaggerServiceUrl::new(name, url));
    }

    /// Serializes configuration into a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serializes configuration into a pretty-printed JSON string.
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Builder for constructing [`SwaggerConfig`].
#[derive(Debug, Default)]
pub struct SwaggerConfigBuilder {
    urls: Vec<SwaggerServiceUrl>,
    layout: Option<String>,
    deep_linking: Option<bool>,
    persist_authorization: Option<bool>,
    display_request_duration: Option<bool>,
    filter: Option<bool>,
}

impl SwaggerConfigBuilder {
    /// Adds a service spec endpoint.
    #[must_use]
    pub fn add_service(mut self, name: impl Into<String>, url: impl Into<String>) -> Self {
        self.urls.push(SwaggerServiceUrl::new(name, url));
        self
    }

    /// Sets the UI layout.
    #[must_use]
    pub fn layout(mut self, layout: impl Into<String>) -> Self {
        self.layout = Some(layout.into());
        self
    }

    /// Sets deep linking behavior.
    #[must_use]
    pub fn deep_linking(mut self, deep_linking: bool) -> Self {
        self.deep_linking = Some(deep_linking);
        self
    }

    /// Sets authorization persistence.
    #[must_use]
    pub fn persist_authorization(mut self, persist: bool) -> Self {
        self.persist_authorization = Some(persist);
        self
    }

    /// Sets request duration display.
    #[must_use]
    pub fn display_request_duration(mut self, display: bool) -> Self {
        self.display_request_duration = Some(display);
        self
    }

    /// Sets operation search/filter capability.
    #[must_use]
    pub fn filter(mut self, filter: bool) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Builds the [`SwaggerConfig`].
    pub fn build(self) -> SwaggerConfig {
        SwaggerConfig {
            urls: self.urls,
            layout: self.layout.unwrap_or_else(default_layout),
            deep_linking: self.deep_linking.unwrap_or(true),
            persist_authorization: self.persist_authorization.unwrap_or(true),
            display_request_duration: self.display_request_duration.unwrap_or(true),
            filter: self.filter.unwrap_or(true),
        }
    }
}

/// Axum Router exposing the Swagger UI aggregator configuration endpoint `/swagger-config.json`.
///
/// Aliased also to `/swagger-ui/config/swagger-config.json` for direct container mapping compatibility.
pub fn create_swagger_config_router(config: SwaggerConfig) -> Router {
    let config = Arc::new(config);
    let handler = {
        let config = Arc::clone(&config);
        move || {
            let config = Arc::clone(&config);
            async move {
                (
                    [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
                    Json((*config).clone()),
                )
                    .into_response()
            }
        }
    };

    Router::new()
        .route("/swagger-config.json", get(handler.clone()))
        .route("/swagger-ui/config/swagger-config.json", get(handler))
}
