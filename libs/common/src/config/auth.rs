//! Authentication and cryptographic security configuration.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::{serialize_opt_redacted_secret, serialize_redacted_secret};

/// Chế độ vận hành khóa ký & thẩm định JWT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthKeyMode {
    /// Sử dụng khóa bí mật đối xứng HMAC (phù hợp dev, staging và microservices nội bộ).
    #[default]
    Symmetric,
    /// Sử dụng cặp khóa bất đối xứng RSA/Ed25519 qua file hoặc chuỗi PEM.
    AsymmetricPem,
    /// Thẩm định token qua endpoint JWKS từ xa của Identity Provider.
    JwksRemote,
}

/// Thuật toán ký JWT hỗ trợ chuẩn hóa trong hệ thống.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SupportedAlgorithm {
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
    /// ECDSA sử dụng SHA-256 trên đường cong NIST P-256.
    ES256,
    /// ECDSA sử dụng SHA-384 trên đường cong NIST P-384.
    ES384,
    /// ECDSA sử dụng SHA-512 trên đường cong NIST P-521.
    ES512,
}

/// Bí danh tương thích ngược cho [`SupportedAlgorithm`].
pub type JwtAlgorithm = SupportedAlgorithm;

/// Cấu hình cặp khóa bất đối xứng qua PEM.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AsymmetricConfig {
    /// Private key PEM bọc trong [`SecretString`] (PKCS#8).
    #[serde(default, serialize_with = "serialize_opt_redacted_secret")]
    pub private_key_pem: Option<SecretString>,

    /// Public key PEM (SPKI).
    #[serde(default)]
    pub public_key_pem: Option<String>,
}

const fn default_jwks_refresh_interval_secs() -> u64 {
    3600
}

const fn default_jwks_cache_ttl_secs() -> u64 {
    86400
}

/// Cấu hình thẩm định token qua JWKS endpoint từ xa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwksConfig {
    /// URL endpoint JWKS (vd: `https://auth.omni.internal/.well-known/jwks.json`).
    #[serde(default)]
    pub url: Option<String>,

    /// Tần suất làm mới khóa từ xa (giây).
    #[serde(default = "default_jwks_refresh_interval_secs")]
    pub refresh_interval_secs: u64,

    /// Thời gian lưu đệm bộ khóa (giây).
    #[serde(default = "default_jwks_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
}

impl Default for JwksConfig {
    fn default() -> Self {
        Self {
            url: None,
            refresh_interval_secs: default_jwks_refresh_interval_secs(),
            cache_ttl_secs: default_jwks_cache_ttl_secs(),
        }
    }
}

const fn default_argon2_memory_kib() -> u32 {
    19456
}

const fn default_argon2_iterations() -> u32 {
    2
}

const fn default_argon2_parallelism() -> u32 {
    1
}

const fn default_argon2_salt_length() -> usize {
    16
}

/// Cấu hình thuật toán băm mật khẩu Argon2id theo chuẩn OWASP.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Argon2Config {
    /// Dung lượng bộ nhớ cấp cho Argon2id (KiB).
    #[validate(range(min = 1024, max = 1_048_576))]
    #[serde(default = "default_argon2_memory_kib")]
    pub memory_kib: u32,

    /// Số vòng lặp băm của Argon2id.
    #[validate(range(min = 1, max = 20))]
    #[serde(default = "default_argon2_iterations")]
    pub iterations: u32,

    /// Mức độ song song hóa luồng băm của Argon2id.
    #[validate(range(min = 1, max = 16))]
    #[serde(default = "default_argon2_parallelism")]
    pub parallelism: u32,

    /// Độ dài salt ngẫu nhiên tính bằng bytes.
    #[validate(range(min = 16, max = 64))]
    #[serde(default = "default_argon2_salt_length")]
    pub salt_length_bytes: usize,
}

impl Default for Argon2Config {
    fn default() -> Self {
        Self {
            memory_kib: default_argon2_memory_kib(),
            iterations: default_argon2_iterations(),
            parallelism: default_argon2_parallelism(),
            salt_length_bytes: default_argon2_salt_length(),
        }
    }
}

/// Cấu hình bảo mật danh tính & xác thực (`omni-auth`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct AuthConfig {
    /// Cờ kích hoạt phân hệ xác thực.
    pub enabled: bool,

    /// Chế độ quản lý và kiểm tra khóa ký token.
    pub mode: AuthKeyMode,

    /// Thuật toán ký JWT.
    pub jwt_algorithm: SupportedAlgorithm,

    /// Khóa ký đối xứng HMAC bọc trong [`SecretString`].
    #[serde(serialize_with = "serialize_redacted_secret")]
    pub jwt_secret: SecretString,

    /// Key ID (`kid`) đính kèm trong header của JWT.
    pub jwt_key_id: String,

    /// Cấu hình khóa bất đối xứng.
    pub asymmetric: AsymmetricConfig,

    /// Cấu hình JWKS.
    pub jwks: JwksConfig,

    /// Thời gian sống của Access Token (giây).
    #[validate(range(min = 1, max = 86400))]
    pub access_token_ttl_secs: u64,

    /// Thời gian sống của Refresh Token (giây).
    #[validate(range(min = 1, max = 2_592_000))]
    pub refresh_token_ttl_secs: u64,

    /// Dung sai lệch đồng hồ giữa các node (giây).
    #[validate(range(min = 0, max = 120))]
    pub clock_skew_leeway_secs: u64,

    /// Nhà phát hành mã thông báo (`iss`).
    #[validate(length(min = 1))]
    pub issuer: String,

    /// Đối tượng sử dụng mã thông báo (`aud`).
    #[validate(length(min = 1))]
    pub audience: String,

    /// Khóa bí mật ký Context nội bộ từ API Gateway sang các Downstream microservices.
    #[serde(serialize_with = "serialize_redacted_secret")]
    pub internal_gateway_secret: SecretString,

    /// Thời gian sống tối đa của chữ ký Gateway chống Replay Attack (giây).
    #[validate(range(min = 5, max = 300))]
    pub signature_max_age_secs: u64,

    /// Cấu hình băm mật khẩu Argon2id.
    #[validate(nested)]
    pub argon2: Argon2Config,

    /// Trường tương thích ngược: `argon2_memory_kib` cũ.
    #[serde(default, skip_serializing)]
    pub argon2_memory_kib: Option<u32>,

    /// Trường tương thích ngược: `argon2_iterations` cũ.
    #[serde(default, skip_serializing)]
    pub argon2_iterations: Option<u32>,

    /// Trường tương thích ngược: `argon2_parallelism` cũ.
    #[serde(default, skip_serializing)]
    pub argon2_parallelism: Option<u32>,
}

impl AuthConfig {
    /// Trả về dung lượng bộ nhớ Argon2id hiệu lực (KiB).
    #[must_use]
    pub fn argon2_memory_kib(&self) -> u32 {
        self.argon2_memory_kib.unwrap_or(self.argon2.memory_kib)
    }

    /// Trả về số vòng lặp Argon2id hiệu lực.
    #[must_use]
    pub fn argon2_iterations(&self) -> u32 {
        self.argon2_iterations.unwrap_or(self.argon2.iterations)
    }

    /// Trả về mức song song Argon2id hiệu lực.
    #[must_use]
    pub fn argon2_parallelism(&self) -> u32 {
        self.argon2_parallelism.unwrap_or(self.argon2.parallelism)
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AuthKeyMode::Symmetric,
            jwt_algorithm: SupportedAlgorithm::HS256,
            jwt_secret: SecretString::from(
                "dev_only_jwt_secret_key_must_be_at_least_32_bytes_long",
            ),
            jwt_key_id: "omni-key-2026-v1".to_string(),
            asymmetric: AsymmetricConfig::default(),
            jwks: JwksConfig::default(),
            access_token_ttl_secs: 900,
            refresh_token_ttl_secs: 604_800,
            clock_skew_leeway_secs: 30,
            issuer: "urn:omni:auth".to_string(),
            audience: "urn:omni:api".to_string(),
            internal_gateway_secret: SecretString::from(
                "dev_internal_gateway_hmac_signing_key_32_bytes_minimum",
            ),
            signature_max_age_secs: 30,
            argon2: Argon2Config::default(),
            argon2_memory_kib: None,
            argon2_iterations: None,
            argon2_parallelism: None,
        }
    }
}
