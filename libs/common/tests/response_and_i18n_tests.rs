//! Integration & Unit Tests for `omni-common` (`response`, `i18n`, `middleware`).

use std::{borrow::Cow, collections::HashMap};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    middleware::from_fn,
    routing::{get, post},
};
use chrono::TimeZone;
use omni_common::{
    ApiResponse, AuthUserClaims, CursorPaginationMeta, DEFAULT_LOCALE, I18nManager,
    OffsetPaginationMeta, PaginatedResponse, RequestContext, RequestLocale, ResponseMeta,
    create_reload_locales_router,
    i18n::{match_accept_language, match_canonical_locale},
    middleware::{
        SERVER_TIMING_HEADER, TRACEPARENT_HEADER, X_RESPONSE_TIME_HEADER, parse_traceparent,
        request_context_middleware, timing_middleware,
    },
    no_content_from_meta,
    response::X_REQUEST_ID_HEADER,
};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UserDto {
    user_id: String,
    email: String,
    full_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OrderItemDto {
    id: String,
    total_amount: f64,
    currency: String,
}

#[test]
fn test_deterministic_rfc3339_millis_timestamp_and_no_duration_in_meta() {
    let fixed_time = chrono::Utc
        .with_ymd_and_hms(2026, 10, 29, 8, 30, 0)
        .unwrap()
        .checked_add_signed(chrono::TimeDelta::milliseconds(125))
        .unwrap();

    let meta = ResponseMeta::with_timestamp(
        "01a185db-a70e-777a-8cee-9ad64ad10253",
        "4bf92f3577b34da6a3ce929d0e0e4736",
        fixed_time,
        "vi-VN",
    );

    let json_val = serde_json::to_value(&meta).expect("Failed to serialize ResponseMeta");
    assert_eq!(
        json_val["request_id"],
        "01a185db-a70e-777a-8cee-9ad64ad10253"
    );
    assert_eq!(json_val["trace_id"], "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(json_val["timestamp"], "2026-10-29T08:30:00.125Z");
    assert_eq!(json_val["locale"], "vi-VN");
    // Must not contain duration_ms in body (Zero-Cost Abstraction tenet)
    assert!(json_val.get("duration_ms").is_none());
}

#[test]
fn test_dual_pagination_contracts_serialization() {
    let fixed_time = chrono::Utc
        .with_ymd_and_hms(2026, 10, 29, 8, 30, 0)
        .unwrap()
        .checked_add_signed(chrono::TimeDelta::milliseconds(125))
        .unwrap();

    let meta = ResponseMeta::with_timestamp(
        "01a185e0-c938-73d1-ba0a-38a152c436de",
        "4bf92f3577b34da6a3ce929d0e0e4736",
        fixed_time,
        "vi-VN",
    );

    // 1. Offset Pagination Contract (Section 4.3.A)
    let offset_meta = OffsetPaginationMeta::new(1, 20, 142);
    assert_eq!(offset_meta.total_pages, 8);
    assert!(offset_meta.has_next);
    assert!(!offset_meta.has_prev);

    let (_, _, axum::Json(offset_resp)) = PaginatedResponse::ok_offset(
        vec![OrderItemDto {
            id: "ord_01HZX89AB".to_string(),
            total_amount: 250_000.0,
            currency: "VND".to_string(),
        }],
        offset_meta,
        "Truy vấn danh sách đơn hàng thành công.",
        meta.clone(),
    );

    let offset_json = serde_json::to_value(&offset_resp).unwrap();
    assert_eq!(offset_json["success"], true);
    assert_eq!(offset_json["code"], "OK");
    assert_eq!(offset_json["meta"]["pagination"]["mode"], "offset");
    assert_eq!(offset_json["meta"]["pagination"]["page"], 1);
    assert_eq!(offset_json["meta"]["pagination"]["page_size"], 20);
    assert_eq!(offset_json["meta"]["pagination"]["total_items"], 142);
    assert_eq!(offset_json["meta"]["pagination"]["total_pages"], 8);
    assert_eq!(offset_json["meta"]["pagination"]["has_next"], true);
    assert_eq!(offset_json["meta"]["pagination"]["has_prev"], false);

    // 2. Cursor Pagination Contract (Section 4.3.B)
    let cursor_meta = CursorPaginationMeta::forward(50, true, Some("ZXhwb3J0X2lkOjAxSkFaWjgwMDE="));
    let (_, _, axum::Json(cursor_resp)) = PaginatedResponse::ok_cursor(
        vec![OrderItemDto {
            id: "evt_01JAZZ8001".to_string(),
            total_amount: 100.0,
            currency: "USD".to_string(),
        }],
        cursor_meta,
        "Truy vấn dòng sự kiện kiểm toán thành công.",
        meta,
    );

    let cursor_json = serde_json::to_value(&cursor_resp).unwrap();
    assert_eq!(cursor_json["meta"]["pagination"]["mode"], "cursor");
    assert_eq!(
        cursor_json["meta"]["pagination"]["next_cursor"],
        "ZXhwb3J0X2lkOjAxSkFaWjgwMDE="
    );
    assert!(cursor_json["meta"]["pagination"]["prev_cursor"].is_null());
    assert_eq!(cursor_json["meta"]["pagination"]["has_more"], true);
    assert_eq!(cursor_json["meta"]["pagination"]["limit"], 50);
}

#[test]
fn test_bcp47_canonical_locale_matching_and_accept_language() {
    // Exact matches (case-insensitive & underscore tolerant)
    assert_eq!(match_canonical_locale("vi-VN"), Some("vi-VN"));
    assert_eq!(match_canonical_locale("vi_vn"), Some("vi-VN"));
    assert_eq!(match_canonical_locale("en-US"), Some("en-US"));
    assert_eq!(match_canonical_locale("ja-JP"), Some("ja-JP"));

    // Subtag prefix & sibling region fallback (Section 5.1 & 5.2)
    assert_eq!(match_canonical_locale("vi"), Some("vi-VN"));
    assert_eq!(match_canonical_locale("en"), Some("en-US"));
    assert_eq!(match_canonical_locale("en-GB"), Some("en-US"));
    assert_eq!(match_canonical_locale("en-AU"), Some("en-US"));
    assert_eq!(match_canonical_locale("ja"), Some("ja-JP"));

    // Unsupported locales return None
    assert_eq!(match_canonical_locale("fr-FR"), None);
    assert_eq!(match_canonical_locale("*"), None);
    assert_eq!(match_canonical_locale(""), None);

    // Accept-Language quality factor sorting
    assert_eq!(
        match_accept_language("fr-CH, fr;q=0.9, en-GB;q=0.8, vi;q=0.7"),
        Some("en-US")
    );
    assert_eq!(
        match_accept_language("en-US;q=0.6, vi;q=0.9, ja;q=0.8"),
        Some("vi-VN")
    );
    assert_eq!(match_accept_language("de-DE, fr;q=0.8"), None);
}

#[test]
fn test_i18n_manager_fluent_translations_and_rcu_hot_reload() {
    let i18n = I18nManager::new_embedded().expect("Should load embedded Fluent bundles");

    // 1. Simple translations across all 3 supported locales
    assert_eq!(
        i18n.translate_simple("vi-VN", "msg-user-fetch-success"),
        "Lấy thông tin người dùng thành công."
    );
    assert_eq!(
        i18n.translate_simple("en-US", "msg-user-fetch-success"),
        "User information retrieved successfully."
    );
    assert_eq!(
        i18n.translate_simple("ja-JP", "msg-user-fetch-success"),
        "ユーザー情報を取得しました。"
    );

    // 2. Dynamic parameter interpolation & pluralization in English vs Vietnamese
    let mut one_param = HashMap::new();
    one_param.insert("count".to_string(), serde_json::json!(1));
    assert_eq!(
        i18n.translate_with_params("en-US", "err-validation-failed-detail", Some(&one_param)),
        "1 field failed business constraint validation."
    );

    let mut two_params = HashMap::new();
    two_params.insert("count".to_string(), serde_json::json!(2));
    assert_eq!(
        i18n.translate_with_params("en-US", "err-validation-failed-detail", Some(&two_params)),
        "2 fields failed business constraint validation."
    );
    assert_eq!(
        i18n.translate_with_params("vi-VN", "err-validation-failed-detail", Some(&two_params)),
        "Có 2 trường dữ liệu không vượt qua kiểm tra ràng buộc nghiệp vụ."
    );

    // Ensure no Unicode bidi isolation marks (\u{2068} / \u{2069}) pollute JSON strings
    let mut name_param = HashMap::new();
    name_param.insert("name".to_string(), serde_json::json!("Alice"));
    let greeting = i18n.translate_with_params("vi-VN", "greeting-user", Some(&name_param));
    assert_eq!(greeting, "Xin chào, Alice!");
    assert!(!greeting.contains('\u{2068}'));
    assert!(!greeting.contains('\u{2069}'));

    // 3. RCU Hot-Reload resilience: invalid Fluent syntax must abort reload and retain existing state
    let broken_sources = vec![
        ("vi-VN", vec![("common.ftl", "broken_key = { $unclosed")]),
        ("en-US", vec![("common.ftl", "app-name = Omni")]),
        ("ja-JP", vec![("common.ftl", "app-name = Omni")]),
    ];
    assert!(i18n.reload_from_memory(&broken_sources).is_err());
    // Previous bundle must still be intact!
    assert_eq!(
        i18n.translate_simple("vi-VN", "msg-user-fetch-success"),
        "Lấy thông tin người dùng thành công."
    );
}

#[test]
fn test_w3c_traceparent_parsing() {
    let valid = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let parsed = parse_traceparent(valid).expect("Should parse valid W3C traceparent");
    assert_eq!(parsed.version, "00");
    assert_eq!(parsed.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(parsed.parent_id, "00f067aa0ba902b7");
    assert_eq!(parsed.trace_flags, "01");

    // All-zero trace_id is invalid per W3C Trace Context spec
    let all_zero_trace = "00-00000000000000000000000000000000-00f067aa0ba902b7-01";
    assert!(parse_traceparent(all_zero_trace).is_none());

    // Version ff is invalid
    let invalid_ver = "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    assert!(parse_traceparent(invalid_ver).is_none());
}

fn build_test_router(i18n: I18nManager) -> Router {
    Router::new()
        .route(
            "/api/v1/users/{id}",
            get({
                let i18n = i18n.clone();
                move |meta: ResponseMeta| {
                    let i18n = i18n.clone();
                    async move {
                        let msg = i18n.translate_simple(&meta.locale, "msg-user-fetch-success");
                        let user = UserDto {
                            user_id: "01a185dd-a81b-72fe-be38-bc8b3c2fb39f".to_string(),
                            email: "user@example.com".to_string(),
                            full_name: "Nguyen Van A".to_string(),
                        };
                        ApiResponse::ok(user, Cow::Owned(msg), meta)
                    }
                }
            })
            .delete(|meta: ResponseMeta| async move { no_content_from_meta(&meta) }),
        )
        .route(
            "/api/v1/users",
            post({
                let i18n = i18n.clone();
                move |meta: ResponseMeta| {
                    let i18n = i18n.clone();
                    async move {
                        let msg = i18n.translate_simple(&meta.locale, "msg-resource-created");
                        let user = UserDto {
                            user_id: "usr_01HZX87Q1K7".to_string(),
                            email: "new@example.com".to_string(),
                            full_name: "Tran Thi B".to_string(),
                        };
                        ApiResponse::created(
                            user,
                            "/api/v1/users/usr_01HZX87Q1K7",
                            Cow::Owned(msg),
                            meta,
                        )
                    }
                }
            }),
        )
        .merge(create_reload_locales_router(i18n))
        .layer(from_fn(request_context_middleware))
        .layer(from_fn(timing_middleware))
}

#[tokio::test]
async fn test_axum_get_200_waterfall_and_timing_headers() {
    let app = build_test_router(I18nManager::default());

    let req = Request::builder()
        .uri("/api/v1/users/123?lang=en-US")
        .header("x-locale", "ja-JP")
        .header(header::ACCEPT_LANGUAGE, "vi-VN")
        .header(X_REQUEST_ID_HEADER, "01a185dd-dbd1-74eb-96df-2f4a27de348b")
        .header(
            TRACEPARENT_HEADER,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_LANGUAGE).unwrap(),
        "en-US"
    );
    assert_eq!(
        resp.headers().get(X_REQUEST_ID_HEADER).unwrap(),
        "01a185dd-dbd1-74eb-96df-2f4a27de348b"
    );
    assert!(
        resp.headers()
            .get(SERVER_TIMING_HEADER)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("total;dur=")
    );
    assert!(
        resp.headers()
            .get(X_RESPONSE_TIME_HEADER)
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("ms")
    );

    let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json_body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json_body["success"], true);
    assert_eq!(json_body["code"], "OK");
    assert_eq!(
        json_body["message"],
        "User information retrieved successfully."
    );
    assert_eq!(
        json_body["meta"]["request_id"],
        "01a185dd-dbd1-74eb-96df-2f4a27de348b"
    );
    assert_eq!(
        json_body["meta"]["trace_id"],
        "4bf92f3577b34da6a3ce929d0e0e4736"
    );
    assert_eq!(json_body["meta"]["locale"], "en-US");
}

#[tokio::test]
async fn test_axum_post_201_created_delete_204_and_reload_endpoint() {
    let app = build_test_router(I18nManager::default());

    // 1. POST 201 Created with Location header and JWT AuthUserClaims (Step 3 of Waterfall)
    let mut req_created = Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header(header::ACCEPT_LANGUAGE, "vi-VN")
        .body(Body::empty())
        .unwrap();
    req_created
        .extensions_mut()
        .insert(AuthUserClaims::new("usr_1", Some("ja")));

    let resp_created = app.clone().oneshot(req_created).await.unwrap();
    assert_eq!(resp_created.status(), StatusCode::CREATED);
    assert_eq!(
        resp_created.headers().get(header::LOCATION).unwrap(),
        "/api/v1/users/usr_01HZX87Q1K7"
    );
    assert_eq!(
        resp_created
            .headers()
            .get(header::CONTENT_LANGUAGE)
            .unwrap(),
        "ja-JP"
    );

    // 2. DELETE 204 No Content MUST have 0-byte body (RFC 9110 Section 15.3.5)
    let req_delete = Request::builder()
        .method("DELETE")
        .uri("/api/v1/users/123")
        .body(Body::empty())
        .unwrap();

    let resp_delete = app.clone().oneshot(req_delete).await.unwrap();
    assert_eq!(resp_delete.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        resp_delete.headers().get(header::CONTENT_LANGUAGE).unwrap(),
        DEFAULT_LOCALE
    );
    assert!(resp_delete.headers().contains_key(X_REQUEST_ID_HEADER));
    let delete_bytes = to_bytes(resp_delete.into_body(), usize::MAX).await.unwrap();
    assert!(
        delete_bytes.is_empty(),
        "HTTP 204 No Content must have an empty response body"
    );

    // 3. POST /internal/v1/reload-locales
    let req_reload = Request::builder()
        .method("POST")
        .uri("/internal/v1/reload-locales")
        .body(Body::empty())
        .unwrap();
    let resp_reload = app.oneshot(req_reload).await.unwrap();
    assert_eq!(resp_reload.status(), StatusCode::OK);
}

#[test]
fn test_request_context_and_locale_defaults() {
    let ctx = RequestContext::default();
    assert_eq!(ctx.locale, DEFAULT_LOCALE);
    assert!(!ctx.request_id.is_empty());
    assert_eq!(ctx.trace_id.len(), 32);

    let loc = RequestLocale::default();
    assert_eq!(loc.as_str(), DEFAULT_LOCALE);
}
