//! Gateway Signed Context protocol verification and replay attack resistance tests.

use chrono::Utc;
use omni_auth::{
    error::AuthError,
    middleware::signed_context::{GatewayContext, sign_gateway_context, verify_gateway_signature},
};
use secrecy::SecretString;

#[test]
fn test_gateway_signed_context_happy_path() {
    let secret = SecretString::from("dev_internal_gateway_hmac_signing_key_32_bytes_minimum");
    let user_id = "018f4a2b-8a5e-7a1b-9c3d-e4f5a6b7c8d9";
    let tenant_id = Some("018f4a2b-8a5e-7a1b-9c3d-e4f5a6b7c8da");
    let roles = "admin,operator";
    let now = Utc::now().timestamp();

    let ctx = GatewayContext::new(user_id, tenant_id, roles, now);
    let sig = sign_gateway_context(&secret, &ctx).expect("Signing must succeed");

    let is_valid =
        verify_gateway_signature(&secret, &ctx, now, 30, &sig).expect("Verification must complete");

    assert!(is_valid, "Valid signature must pass");
}

#[test]
fn test_gateway_signed_context_replay_attack_rejected() {
    let secret = SecretString::from("dev_internal_gateway_hmac_signing_key_32_bytes_minimum");
    let user_id = "018f4a2b-8a5e-7a1b-9c3d-e4f5a6b7c8d9";
    let now = Utc::now().timestamp();
    let old_timestamp = now - 45; // Quá ngưỡng max_age 30 giây

    let ctx = GatewayContext::new(user_id, None, "user", old_timestamp);
    let sig = sign_gateway_context(&secret, &ctx).expect("Signing ok");

    let err = verify_gateway_signature(&secret, &ctx, now, 30, &sig);

    match err {
        Err(AuthError::SignatureExpired) => (),
        other => panic!("Expected SignatureExpired on replay attack, got: {other:?}"),
    }
}

#[test]
fn test_gateway_signed_context_tampered_payload_rejected() {
    let secret = SecretString::from("dev_internal_gateway_hmac_signing_key_32_bytes_minimum");
    let user_id = "018f4a2b-8a5e-7a1b-9c3d-e4f5a6b7c8d9";
    let now = Utc::now().timestamp();

    let original_ctx = GatewayContext::new(user_id, None, "user", now);
    let sig = sign_gateway_context(&secret, &original_ctx).expect("Signing ok");

    // Kẻ tấn công sửa đổi vai trò từ `user` thành `admin`
    let tampered_ctx = GatewayContext::new(user_id, None, "admin", now);
    let is_valid =
        verify_gateway_signature(&secret, &tampered_ctx, now, 30, &sig).expect("Verification ok");

    assert!(!is_valid, "Tampered roles must be rejected");
}
