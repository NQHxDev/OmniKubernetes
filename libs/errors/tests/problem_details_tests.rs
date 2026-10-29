//! Integration & Unit Tests for `omni-errors` (RFC 9457 Problem Details, Validation, Security Hardening).

use std::collections::HashMap;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use chrono::TimeZone;
use omni_common::{I18nManager, ResponseMeta, response::X_REQUEST_ID_HEADER};
use omni_errors::{
    ApiErrorResponse, AppError, ERR_INTERNAL_SERVER, ERR_VALIDATION_FAILED, InvalidParamError,
    PROBLEM_JSON_MEDIA_TYPE, TYPE_INTERNAL_SERVER_ERROR, TYPE_VALIDATION_ERROR,
    VAL_INVALID_ENUM_VALUE, VAL_INVALID_POSTAL_CODE, VAL_NUMBER_MIN_VALUE,
    dot_notation_to_json_pointer, json_pointer_to_dot_notation,
};
use tower::ServiceExt;

#[test]
fn test_rfc6901_json_pointer_and_dot_notation_conversion() {
    assert_eq!(
        json_pointer_to_dot_notation("/items/0/quantity"),
        "items.0.quantity"
    );
    assert_eq!(
        json_pointer_to_dot_notation("/shipping_address/postal_code"),
        "shipping_address.postal_code"
    );
    assert_eq!(
        dot_notation_to_json_pointer("items.0.quantity"),
        "/items/0/quantity"
    );
    assert_eq!(
        dot_notation_to_json_pointer("shipping_address.postal_code"),
        "/shipping_address/postal_code"
    );
}

#[test]
fn test_rfc9457_validation_error_contract_and_pii_security_policy() {
    let i18n = I18nManager::default();
    let fixed_time = chrono::Utc
        .with_ymd_and_hms(2026, 10, 29, 8, 30, 0)
        .unwrap()
        .checked_add_signed(chrono::TimeDelta::milliseconds(125))
        .unwrap();

    let meta = ResponseMeta::with_timestamp(
        "01a185e2-2fed-7781-8100-1b8e31a0eee5",
        "4bf92f3577b34da6a3ce929d0e0e4736",
        fixed_time,
        "vi-VN",
    );

    let mut qty_params = HashMap::new();
    qty_params.insert("min".to_string(), serde_json::json!(1));
    qty_params.insert("actual".to_string(), serde_json::json!(0));

    let param1 = InvalidParamError::localized(
        "items.0.quantity",
        VAL_NUMBER_MIN_VALUE,
        VAL_NUMBER_MIN_VALUE,
        Some(qty_params),
        &i18n,
        "vi-VN",
    );
    assert_eq!(param1.to_json_pointer(), "/items/0/quantity");
    // Secure-by-Default: rejected_value is None
    assert!(param1.rejected_value.is_none());

    let mut postal_params = HashMap::new();
    postal_params.insert("pattern".to_string(), serde_json::json!("^[0-9]{5,6}$"));

    let param2 = InvalidParamError::localized(
        "shipping_address.postal_code",
        VAL_INVALID_POSTAL_CODE,
        VAL_INVALID_POSTAL_CODE,
        Some(postal_params),
        &i18n,
        "vi-VN",
    );

    let problem =
        ApiErrorResponse::validation_failed(vec![param1, param2], "/api/v1/orders", meta, &i18n);

    let json_val = serde_json::to_value(&problem).expect("Failed to serialize ApiErrorResponse");
    assert_eq!(json_val["type"], TYPE_VALIDATION_ERROR);
    assert_eq!(json_val["title"], "Dữ liệu yêu cầu không hợp lệ");
    assert_eq!(json_val["status"], 422);
    assert_eq!(
        json_val["detail"],
        "Có 2 trường dữ liệu không vượt qua kiểm tra ràng buộc nghiệp vụ."
    );
    assert_eq!(json_val["instance"], "/api/v1/orders");
    assert_eq!(json_val["code"], ERR_VALIDATION_FAILED);

    let invalid_params = json_val["invalid_params"].as_array().unwrap();
    assert_eq!(invalid_params.len(), 2);
    assert_eq!(invalid_params[0]["name"], "items.0.quantity");
    assert_eq!(
        invalid_params[0]["reason"],
        "Số lượng sản phẩm trong đơn hàng phải lớn hơn 0."
    );
    assert_eq!(invalid_params[0]["code"], "NUMBER_MIN_VALUE");
    assert_eq!(invalid_params[0]["params"]["min"], 1);
    assert_eq!(invalid_params[0]["params"]["actual"], 0);
    // Verify rejected_value is completely omitted from JSON!
    assert!(invalid_params[0].get("rejected_value").is_none());

    // Verify finite non-PII enum rejected_value opt-in works when explicitly called
    let enum_param = InvalidParamError::new(
        "role",
        VAL_INVALID_ENUM_VALUE,
        "Vai trò người dùng không hợp lệ.",
    )
    .with_rejected_enum_value("SUPER_ADMIN")
    .with_param("allowed", serde_json::json!(["MEMBER", "ADMIN"]));

    let enum_json = serde_json::to_value(&enum_param).unwrap();
    assert_eq!(enum_json["rejected_value"], "SUPER_ADMIN");
}

#[tokio::test]
async fn test_rfc9457_into_response_headers_and_500_information_leakage_prevention() {
    let i18n = I18nManager::default();

    let app = Router::new().route(
        "/api/v1/orders",
        post({
            let i18n = i18n.clone();
            move || {
                let i18n = i18n.clone();
                async move {
                    let meta = ResponseMeta::new(
                        "01a185e2-2fed-7781-8100-1b8e31a0eee5",
                        "4bf92f3577b34da6a3ce929d0e0e4736",
                        "vi-VN",
                    );
                    // Simulate a sensitive raw SQL / DB connection error
                    let err = AppError::Internal(
                        "FATAL: password authentication failed for user 'postgres' at 10.0.14.22:5432"
                            .to_string(),
                    );
                    err.into_problem("/api/v1/orders", meta, &i18n).into_response()
                }
            }
        }),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/orders")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        PROBLEM_JSON_MEDIA_TYPE
    );
    assert_eq!(
        resp.headers().get(header::CONTENT_LANGUAGE).unwrap(),
        "vi-VN"
    );
    assert_eq!(
        resp.headers().get(X_REQUEST_ID_HEADER).unwrap(),
        "01a185e2-2fed-7781-8100-1b8e31a0eee5"
    );

    let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json_body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json_body["type"], TYPE_INTERNAL_SERVER_ERROR);
    assert_eq!(json_body["status"], 500);
    assert_eq!(json_body["code"], ERR_INTERNAL_SERVER);
    assert_eq!(
        json_body["detail"],
        "Hệ thống đang gặp gián đoạn tạm thời. Vui lòng thử lại sau."
    );
    // Strictly ensure sensitive SQL/infrastructure string is NOT leaked in the response payload
    let raw_body_str = String::from_utf8_lossy(&body_bytes);
    assert!(!raw_body_str.contains("postgres"));
    assert!(!raw_body_str.contains("10.0.14.22"));
}
