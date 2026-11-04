//! Argon2id password hashing and verification tests.

use omni_auth::{config::Argon2Config, crypto::PasswordSecurity};
use secrecy::SecretString;

#[tokio::test]
async fn test_argon2id_hashing_and_verification_success() {
    let config = Argon2Config {
        memory_kib: 19456,
        iterations: 2,
        parallelism: 1,
        salt_length_bytes: 16,
    };

    let security = PasswordSecurity::from_config(&config).expect("PasswordSecurity init failed");

    let raw_pass = SecretString::from("MySecureP@ssw0rd!2026");
    let hash = security
        .hash_password(raw_pass.clone())
        .await
        .expect("Hashing must succeed");

    assert!(hash.starts_with("$argon2id$v=19$"));

    // So khớp thành công với mật khẩu đúng
    let is_valid = security
        .verify_password(raw_pass, hash.clone())
        .await
        .expect("Verification must succeed");
    assert!(is_valid, "Correct password must verify to true");

    // So khớp thất bại với mật khẩu sai (Constant-time execution)
    let wrong_pass = SecretString::from("IncorrectPassword!");
    let is_valid_wrong = security
        .verify_password(wrong_pass, hash)
        .await
        .expect("Verification check must complete");
    assert!(!is_valid_wrong, "Wrong password must verify to false");
}

#[tokio::test]
async fn test_argon2id_invalid_hash_format() {
    let security = PasswordSecurity::new(19456, 2, 1).expect("PasswordSecurity init failed");
    let pass = SecretString::from("AnyPassword123");
    let bad_hash = "not_a_valid_argon2_hash_string".to_string();

    let err = security.verify_password(pass, bad_hash).await;
    assert!(err.is_err(), "Invalid hash format must return Err");
}
