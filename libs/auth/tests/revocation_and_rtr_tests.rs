//! Token revocation and Refresh Token Rotation (RTR) tests.

use chrono::Utc;
use omni_auth::{
    error::AuthError,
    revocation::{
        MemoryRevocationStore, MemoryTokenFamilyStore, RefreshTokenRecord, RevocationStore,
        TokenFamilyStore,
    },
};
use uuid::Uuid;

#[tokio::test]
async fn test_jti_blacklist_revocation_lifecycle() {
    let store = MemoryRevocationStore::new();
    let jti = Uuid::now_v7();
    let now = Utc::now().timestamp();
    let expires_at = now + 600; // 10 phút nữa hết hạn

    // Ban đầu token chưa bị thu hồi
    let is_revoked = store.is_token_revoked(jti).await.expect("Check ok");
    assert!(!is_revoked);

    // Người dùng đăng xuất -> Thu hồi JTI
    store
        .revoke_token(jti, expires_at, "user_logout")
        .await
        .expect("Revoke ok");

    // Kiểm tra lại: Token đã nằm trong blacklist
    let is_revoked_after = store.is_token_revoked(jti).await.expect("Check ok");
    assert!(is_revoked_after, "Token must be reported as revoked");
}

#[tokio::test]
async fn test_rtr_token_family_normal_rotation() {
    let family_store = MemoryTokenFamilyStore::new();
    let user_id = Uuid::now_v7();
    let family_id = Uuid::now_v7();
    let token_1 = Uuid::now_v7();
    let token_2 = Uuid::now_v7();
    let now = Utc::now().timestamp();

    // 1. Cấp Refresh Token ban đầu (Gen 1)
    let initial_record = RefreshTokenRecord {
        token_id: token_1,
        family_id,
        user_id,
        generation: 1,
        is_used: false,
        expires_at: now + 604_800,
    };
    family_store
        .record_token(&initial_record)
        .await
        .expect("Record ok");

    // 2. Client gọi Refresh hợp lệ -> Luân chuyển sang token_2 (Gen 2)
    let record_2 = family_store
        .rotate_token(token_1, token_2, now + 604_800)
        .await
        .expect("Rotate ok");

    assert_eq!(record_2.token_id, token_2);
    assert_eq!(record_2.family_id, family_id);
    assert_eq!(record_2.generation, 2);
    assert!(!record_2.is_used);

    // 3. Family vẫn hợp lệ
    let is_valid = family_store
        .is_family_valid(family_id)
        .await
        .expect("Check ok");
    assert!(is_valid);
}

#[tokio::test]
async fn test_rtr_token_reuse_theft_detection() {
    let family_store = MemoryTokenFamilyStore::new();
    let user_id = Uuid::now_v7();
    let family_id = Uuid::now_v7();
    let token_1 = Uuid::now_v7();
    let token_2 = Uuid::now_v7();
    let token_attacker = Uuid::now_v7();
    let now = Utc::now().timestamp();

    // 1. Cấp Refresh Token 1
    let initial_record = RefreshTokenRecord {
        token_id: token_1,
        family_id,
        user_id,
        generation: 1,
        is_used: false,
        expires_at: now + 604_800,
    };
    family_store
        .record_token(&initial_record)
        .await
        .expect("Record ok");

    // 2. Client hợp lệ đã dùng token_1 để đổi sang token_2
    let _ = family_store
        .rotate_token(token_1, token_2, now + 604_800)
        .await
        .expect("Rotate 1 ok");

    // 3. Kẻ tấn công đánh cắp token_1 cũ và cố gắng dùng lại!
    let theft_result = family_store
        .rotate_token(token_1, token_attacker, now + 604_800)
        .await;

    match theft_result {
        Err(AuthError::TokenTheftDetected) => (),
        other => panic!("Expected TokenTheftDetected on reused token, got: {other:?}"),
    }

    // 4. Toàn bộ family của người dùng đã bị hủy ngay lập tức
    let is_valid = family_store
        .is_family_valid(family_id)
        .await
        .expect("Check ok");
    assert!(
        !is_valid,
        "Whole family must be invalidated after theft detection"
    );
}
