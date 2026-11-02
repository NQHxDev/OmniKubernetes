//! Authentication and cryptographic security configuration.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::serialize_redacted_secret;

/// Thuật toán ký và xác thực JWT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JwtAlgorithm {
    /// HMAC sử dụng SHA-256 (Khóa bí mật đối xứng tối thiểu 32 bytes).
    #[default]
    HS256,
    /// HMAC sử dụng SHA-384.
    HS384,
    /// HMAC sử dụng SHA-512.
    HS512,
    /// RSA Signature với SHA-256 (Khóa bất đối xứng).
    RS256,
    /// Edwards-curve Digital Signature Algorithm (Ed25519).
    EdDSA,
}

/// Cấu hình bảo mật danh tính & xác thực (`omni-auth`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct AuthConfig {
    /// Cờ kích hoạt phân hệ xác thực.
    pub enabled: bool,

    /// Thuật toán ký JWT.
    #[serde(default)]
    pub jwt_algorithm: JwtAlgorithm,

    /// Khóa ký đối xứng HMAC hoặc chuỗi PEM bí mật bọc trong [`SecretString`].
    #[serde(serialize_with = "serialize_redacted_secret")]
    pub jwt_secret: SecretString,

    /// Thời gian sống của Access Token (giây).
    #[validate(range(min = 1, max = 86400))]
    pub access_token_ttl_secs: u64,

    /// Thời gian sống của Refresh Token (giây).
    #[validate(range(min = 1, max = 2_592_000))]
    pub refresh_token_ttl_secs: u64,

    /// Nhà phát hành mã thông báo (`iss`).
    #[validate(length(min = 1))]
    pub issuer: String,

    /// Đối tượng sử dụng mã thông báo (`aud`).
    #[validate(length(min = 1))]
    pub audience: String,

    /// Dung lượng bộ nhớ cấp cho thuật toán băm Argon2 (KiB).
    #[validate(range(min = 1024, max = 1_048_576))]
    pub argon2_memory_kib: u32,

    /// Số vòng lặp băm của Argon2.
    #[validate(range(min = 1, max = 20))]
    pub argon2_iterations: u32,

    /// Mức độ song song hóa luồng băm của Argon2.
    #[validate(range(min = 1, max = 16))]
    pub argon2_parallelism: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jwt_algorithm: JwtAlgorithm::HS256,
            jwt_secret: SecretString::from(
                "dev_only_jwt_secret_key_must_be_at_least_32_bytes_long",
            ),
            access_token_ttl_secs: 900,
            refresh_token_ttl_secs: 604_800,
            issuer: "urn:omni:auth".to_string(),
            audience: "urn:omni:api".to_string(),
            argon2_memory_kib: 19456,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        }
    }
}
