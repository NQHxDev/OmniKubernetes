//! Standard `AppBuilder` Pattern Implementation.
//!
//! Provides a declarative, type-safe builder to construct Axum applications
//! with a strictly ordered 9-layer middleware onion architecture.

use std::sync::Arc;

use axum::Router;
use tower_http::cors::CorsLayer;

use crate::{
    bootstrap::{fallback::default_not_found_handler, panic_handler::handle_panic},
    config::ResilienceConfig,
    middleware::{observability_middleware, security_headers_middleware, timing_middleware},
    openapi::{DocsConfig, create_docs_router_with_config},
    resilience::{ShutdownDrainGate, create_probes_router, load_shed_layer, timeout_layer},
};

/// Declarative, type-safe builder for initializing Axum microservices.
#[allow(clippy::struct_excessive_bools)]
pub struct AppBuilder<S = ()> {
    service_name: &'static str,
    state: Option<S>,
    custom_routes: Vec<(String, Router<S>)>,
    enable_observability: bool,
    enable_metrics: bool,
    enable_resilience: bool,
    enable_security_headers: bool,
    cors_layer: Option<CorsLayer>,
    drain_gate: Option<Arc<dyn ShutdownDrainGate>>,
    metrics_recorder: Option<metrics_exporter_prometheus::PrometheusHandle>,
    openapi_spec: Option<utoipa::openapi::OpenApi>,
    docs_config: Option<DocsConfig>,
    resilience_config: Option<ResilienceConfig>,
}

impl<S> std::fmt::Debug for AppBuilder<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppBuilder")
            .field("service_name", &self.service_name)
            .field("enable_observability", &self.enable_observability)
            .field("enable_metrics", &self.enable_metrics)
            .field("enable_resilience", &self.enable_resilience)
            .field("enable_security_headers", &self.enable_security_headers)
            .field("custom_routes_count", &self.custom_routes.len())
            .finish_non_exhaustive()
    }
}

impl AppBuilder<()> {
    /// Creates a new `AppBuilder` instance for the given service name.
    #[must_use]
    pub fn new(service_name: &'static str) -> Self {
        Self {
            service_name,
            state: None,
            custom_routes: Vec::new(),
            enable_observability: true,
            enable_metrics: true,
            enable_resilience: true,
            enable_security_headers: true,
            cors_layer: None,
            drain_gate: None,
            metrics_recorder: None,
            openapi_spec: None,
            docs_config: None,
            resilience_config: None,
        }
    }
}

impl<S> AppBuilder<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Sets the application state shared across handlers.
    #[must_use]
    pub fn with_state<NewS>(self, state: NewS) -> AppBuilder<NewS>
    where
        NewS: Clone + Send + Sync + 'static,
    {
        AppBuilder {
            service_name: self.service_name,
            state: Some(state),
            custom_routes: Vec::new(),
            enable_observability: self.enable_observability,
            enable_metrics: self.enable_metrics,
            enable_resilience: self.enable_resilience,
            enable_security_headers: self.enable_security_headers,
            cors_layer: self.cors_layer,
            drain_gate: self.drain_gate,
            metrics_recorder: self.metrics_recorder,
            openapi_spec: self.openapi_spec,
            docs_config: self.docs_config,
            resilience_config: self.resilience_config,
        }
    }

    /// Enables Kubernetes health probes (`/livez`, `/readyz`, `/startupz`, `/healthz`).
    #[must_use]
    pub fn with_k8s_probes(mut self, drain_gate: Arc<dyn ShutdownDrainGate>) -> Self {
        self.drain_gate = Some(drain_gate);
        self
    }

    /// Enables the `/metrics` exposition endpoint with Prometheus recorder.
    #[must_use]
    pub fn with_metrics_endpoint(
        mut self,
        recorder: metrics_exporter_prometheus::PrometheusHandle,
    ) -> Self {
        self.metrics_recorder = Some(recorder);
        self
    }

    /// Configures `OpenAPI` documentation routes (`/v3/api-docs/openapi.json`, Swagger UI).
    #[must_use]
    pub fn with_openapi_docs(
        mut self,
        spec: utoipa::openapi::OpenApi,
        docs_config: DocsConfig,
    ) -> Self {
        self.openapi_spec = Some(spec);
        self.docs_config = Some(docs_config);
        self
    }

    /// Attaches a custom CORS layer.
    #[must_use]
    pub fn with_cors(mut self, cors: CorsLayer) -> Self {
        self.cors_layer = Some(cors);
        self
    }

    /// Toggles observability middleware (Trace Context, Correlation IDs, Spans).
    #[must_use]
    pub fn with_observability(mut self, enable: bool) -> Self {
        self.enable_observability = enable;
        self
    }

    /// Toggles Prometheus metrics recording.
    #[must_use]
    pub fn with_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }

    /// Toggles resilience middleware (Timeout, Load Shedding, Rate Limiting).
    #[must_use]
    pub fn with_resilience(mut self, enable: bool) -> Self {
        self.enable_resilience = enable;
        self
    }

    /// Configures custom resilience settings.
    #[must_use]
    pub fn with_resilience_config(mut self, config: ResilienceConfig) -> Self {
        self.resilience_config = Some(config);
        self
    }

    /// Toggles OWASP security headers middleware.
    #[must_use]
    pub fn with_security_headers(mut self, enable: bool) -> Self {
        self.enable_security_headers = enable;
        self
    }

    /// Mounts business domain route handlers under a path prefix.
    #[must_use]
    pub fn mount_routes(mut self, path_prefix: &str, router: Router<S>) -> Self {
        self.custom_routes.push((path_prefix.to_string(), router));
        self
    }

    /// Assembles the complete Axum Router with all 9 middleware onion layers.
    pub fn build(self) -> Router {
        let state = self
            .state
            .expect("AppState must be provided via with_state() before building AppBuilder");
        let mut app_router = Router::new();

        // 1. Mount business domain routes with application state
        for (prefix, router) in self.custom_routes {
            if prefix.is_empty() || prefix == "/" {
                app_router = app_router.merge(router);
            } else {
                app_router = app_router.nest(&prefix, router);
            }
        }
        let mut app_router = app_router.with_state(state);

        // 2. Mount infrastructure sub-routers (Probes, Metrics, OpenAPI)
        if let Some(drain_gate) = self.drain_gate {
            app_router = app_router.merge(create_probes_router(drain_gate));
        }

        if let Some(recorder) = self.metrics_recorder {
            app_router = app_router.route(
                "/metrics",
                axum::routing::get(move || {
                    let metrics_data = recorder.render();
                    async move { metrics_data }
                }),
            );
        }

        if let (Some(spec), Some(docs_config)) = (self.openapi_spec, self.docs_config) {
            app_router = app_router.merge(create_docs_router_with_config(spec, docs_config));
        }

        // 3. Fallback Route (RFC 9457 404 Problem Details)
        app_router = app_router.fallback(default_not_found_handler);

        // 4. Apply Middleware Onion Layers in reverse order (outer layer added last)
        if let Some(cors) = self.cors_layer {
            app_router = app_router.layer(cors);
        }

        if self.enable_security_headers {
            app_router = app_router.layer(axum::middleware::from_fn(security_headers_middleware));
        }

        if self.enable_resilience {
            let resilience_cfg = self.resilience_config.unwrap_or_default();
            app_router = app_router.layer(timeout_layer(std::time::Duration::from_secs(30)));
            app_router = app_router.layer(load_shed_layer(&resilience_cfg, self.service_name));
        }

        app_router = app_router.layer(axum::middleware::from_fn(timing_middleware));
        app_router = app_router.layer(tower_http::catch_panic::CatchPanicLayer::custom(
            handle_panic,
        ));

        if self.enable_observability {
            app_router = app_router.layer(axum::middleware::from_fn(observability_middleware));
        }

        app_router
    }
}
