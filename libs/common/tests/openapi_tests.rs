//! Integration Tests for `OpenAPI` Documentation & Swagger Catalog Aggregation.
//!
//! Validates:
//! - Automated `OpenAPI` runtime endpoints (`/v3/api-docs`, `/v3/api-docs/openapi.json`).
//! - Dynamic `X-Forwarded-Prefix` server URL adjustment (Pitfall 1 defense).
//! - Security Scheme `BearerAuth` registration (Pitfall 3 defense).
//! - Common schema registrations (`ResponseMeta`, `PaginationMeta`, etc.).
//! - Swagger UI Aggregator configuration `/swagger-config.json` (Section 5.1).

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use omni_common::{
    CommonSchemaAddon, DocsConfig, SecurityAddon, SwaggerConfig, SwaggerConfigBuilder,
    create_docs_router, create_docs_router_with_config, create_swagger_config_router,
    openapi::{BEARER_AUTH_SECURITY_SCHEME, resolve_openapi_servers},
};
use serde_json::Value;
use tower::ServiceExt;
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Test Order Service",
        version = "1.0.0",
        description = "Test Order API Specification"
    ),
    modifiers(&SecurityAddon, &CommonSchemaAddon)
)]
struct TestApiDoc;

#[tokio::test]
async fn test_create_docs_router_serves_spec_json() {
    let app = create_docs_router(TestApiDoc::openapi());

    // 1. Test /v3/api-docs
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v3/api-docs")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json; charset=utf-8"
    );

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["info"]["title"], "Test Order Service");
    assert_eq!(json["info"]["version"], "1.0.0");

    // 2. Test /v3/api-docs/openapi.json
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v3/api-docs/openapi.json")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["info"]["title"], "Test Order Service");
}

#[tokio::test]
async fn test_dynamic_x_forwarded_prefix_injection() {
    let app = create_docs_router(TestApiDoc::openapi());

    // When Ingress strips prefix and forwards request with X-Forwarded-Prefix: /api/v1/orders
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v3/api-docs")
                .method("GET")
                .header("x-forwarded-prefix", "/api/v1/orders/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();

    // Verify servers array contains normalized /api/v1/orders
    let servers = json["servers"].as_array().expect("servers array missing");
    assert_ne!(servers.as_slice(), &[] as &[Value]);
    assert_eq!(servers[0]["url"], "/api/v1/orders");
    assert_eq!(
        servers[0]["description"],
        "Through OmniKubernetes Ingress Gateway"
    );
}

#[tokio::test]
async fn test_static_service_prefix_config() {
    let config = DocsConfig::builder()
        .service_name("order-service")
        .service_prefix("/api/v1/orders")
        .build();

    let app = create_docs_router_with_config(TestApiDoc::openapi(), config);

    // Call without header
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v3/api-docs")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();

    let servers = json["servers"].as_array().expect("servers array missing");
    assert_eq!(servers[0]["url"], "/api/v1/orders");
}

#[test]
fn test_security_addon_registers_bearer_auth() {
    let mut openapi = TestApiDoc::openapi();
    SecurityAddon.modify(&mut openapi);

    let components = openapi.components.as_ref().unwrap();
    assert!(
        components
            .security_schemes
            .contains_key(BEARER_AUTH_SECURITY_SCHEME)
    );

    let json = serde_json::to_value(&openapi).unwrap();
    let scheme = &json["components"]["securitySchemes"][BEARER_AUTH_SECURITY_SCHEME];
    assert_eq!(scheme["type"], "http");
    assert_eq!(scheme["scheme"], "bearer");
    assert_eq!(scheme["bearerFormat"], "JWT");
}

#[test]
fn test_common_schemas_addon_registers_all_metadata_schemas() {
    let mut openapi = TestApiDoc::openapi();
    CommonSchemaAddon.modify(&mut openapi);

    let json = serde_json::to_value(&openapi).unwrap();
    let schemas = json["components"]["schemas"].as_object().unwrap();

    assert!(schemas.contains_key("ResponseMeta"));
    assert!(schemas.contains_key("OffsetPaginationMeta"));
    assert!(schemas.contains_key("CursorPaginationMeta"));
    assert!(schemas.contains_key("PaginationMeta"));
    assert!(schemas.contains_key("PaginatedResponseMeta"));
    assert!(schemas.contains_key("ReloadLocalesResult"));
}

#[test]
fn test_resolve_openapi_servers_prefers_header_over_config() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "x-forwarded-prefix",
        "/api/v1/dynamic-orders/".parse().unwrap(),
    );

    let openapi = resolve_openapi_servers(
        TestApiDoc::openapi(),
        &headers,
        Some("/api/v1/static-orders"),
    );

    let servers = openapi.servers.unwrap();
    assert_eq!(servers[0].url, "/api/v1/dynamic-orders");
}

#[tokio::test]
async fn test_swagger_config_router_and_schema_compliance() {
    let catalog = SwaggerConfig::default_enterprise();
    let app = create_swagger_config_router(catalog.clone());

    // 1. Test /swagger-config.json
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/swagger-config.json")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();

    // Verify Section 5.1 JSON schema contracts
    assert_eq!(json["layout"], "StandaloneLayout");
    assert_eq!(json["deepLinking"], true);
    assert_eq!(json["persistAuthorization"], true);
    assert_eq!(json["displayRequestDuration"], true);
    assert_eq!(json["filter"], true);

    let urls = json["urls"].as_array().unwrap();
    assert_eq!(urls.len(), 6);
    assert_eq!(urls[0]["name"], "01. Auth & IAM Service");
    assert_eq!(urls[0]["url"], "/api/v1/auth/v3/api-docs");
    assert_eq!(urls[1]["name"], "02. User & Tenant Service");
    assert_eq!(urls[1]["url"], "/api/v1/users/v3/api-docs");
    assert_eq!(urls[2]["name"], "03. Order Processing Service");
    assert_eq!(urls[2]["url"], "/api/v1/orders/v3/api-docs");

    // 2. Test alias /swagger-ui/config/swagger-config.json
    let response = app
        .oneshot(
            Request::builder()
                .uri("/swagger-ui/config/swagger-config.json")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_swagger_config_builder_customization() {
    let config = SwaggerConfigBuilder::default()
        .add_service("Billing Service", "/api/v1/billing/v3/api-docs")
        .deep_linking(false)
        .filter(true)
        .layout("BaseLayout")
        .build();

    assert_eq!(config.urls.len(), 1);
    assert_eq!(config.urls[0].name, "Billing Service");
    assert_eq!(config.urls[0].url, "/api/v1/billing/v3/api-docs");
    assert!(!config.deep_linking);
    assert!(config.persist_authorization); // defaults to true
    assert_eq!(config.layout, "BaseLayout");

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: SwaggerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}

#[cfg(feature = "swagger-ui")]
#[tokio::test]
async fn test_swagger_ui_embedded_endpoint_when_feature_enabled() {
    let config = DocsConfig::builder().enable_swagger_ui(true).build();

    let app = create_docs_router_with_config(TestApiDoc::openapi(), config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/swagger-ui/")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
