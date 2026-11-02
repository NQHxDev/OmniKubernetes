//! Integration Tests for `OpenAPI` Schema Registration of RFC 9457 Problem Details.
//!
//! Validates:
//! - [`ErrorSchemaAddon`] registration of `ApiErrorResponse` and `InvalidParamError`.
//! - `OpenAPI` schema structure compliance with RFC 9457 Appendix A.1.

use omni_errors::{ErrorSchemaAddon, register_error_schemas};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(title = "Errors Spec Test", version = "1.0.0"),
    modifiers(&ErrorSchemaAddon)
)]
struct TestErrorsApiDoc;

#[test]
fn test_error_schema_addon_registers_problem_details_schemas() {
    let mut openapi = TestErrorsApiDoc::openapi();
    ErrorSchemaAddon.modify(&mut openapi);

    let json = serde_json::to_value(&openapi).unwrap();
    let schemas = json["components"]["schemas"].as_object().unwrap();

    assert!(
        schemas.contains_key("ApiErrorResponse"),
        "Missing ApiErrorResponse schema"
    );
    assert!(
        schemas.contains_key("InvalidParamError"),
        "Missing InvalidParamError schema"
    );
}

#[test]
fn test_api_error_response_schema_properties() {
    let mut openapi = TestErrorsApiDoc::openapi();
    register_error_schemas(&mut openapi);

    let json = serde_json::to_value(&openapi).unwrap();
    let api_error_schema = &json["components"]["schemas"]["ApiErrorResponse"];

    let properties = api_error_schema["properties"].as_object().unwrap();
    assert!(properties.contains_key("type"));
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("status"));
    assert!(properties.contains_key("detail"));
    assert!(properties.contains_key("instance"));
    assert!(properties.contains_key("code"));
    assert!(properties.contains_key("invalid_params"));
    assert!(properties.contains_key("meta"));
}

#[test]
fn test_invalid_param_error_schema_properties() {
    let mut openapi = TestErrorsApiDoc::openapi();
    register_error_schemas(&mut openapi);

    let json = serde_json::to_value(&openapi).unwrap();
    let invalid_param_schema = &json["components"]["schemas"]["InvalidParamError"];

    let properties = invalid_param_schema["properties"].as_object().unwrap();
    assert!(properties.contains_key("name"));
    assert!(properties.contains_key("reason"));
    assert!(properties.contains_key("code"));
    assert!(properties.contains_key("rejected_value"));
    assert!(properties.contains_key("params"));
}
