//! Distributed caching and Redis configuration.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::serialize_opt_redacted_secret;

/// Cấu hình kết nối bộ nhớ đệm phân tán Redis (`omni-cache`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisConfig {
    /// Cờ kích hoạt kết nối Redis.
    pub enabled: bool,

    /// Địa chỉ máy chủ Redis.
    #[validate(length(min = 1, message = "redis.host must not be empty"))]
    pub host: String,

    /// Cổng kết nối Redis (mặc định 6379).
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Mật khẩu xác thực Redis (nếu có) được bảo vệ bằng [`SecretString`].
    #[serde(default, serialize_with = "serialize_opt_redacted_secret")]
    pub password: Option<SecretString>,

    /// Chỉ mục database Redis (mặc định 0).
    pub database_index: u8,

    /// Kích thước connection pool kết nối Redis.
    #[validate(range(min = 1, max = 500))]
    pub pool_size: usize,

    /// Thời gian chờ kết nối tối đa (mili-giây).
    #[validate(range(min = 100, max = 30000))]
    pub connection_timeout_ms: u64,

    /// Thời gian chờ lệnh thực thi tối đa (mili-giây).
    #[validate(range(min = 100, max = 30000))]
    pub command_timeout_ms: u64,

    /// Tiền tố phân cách namespace key Redis.
    #[validate(length(min = 1))]
    pub key_prefix: String,

    /// Ngưỡng nén dữ liệu tự động (bytes).
    #[validate(range(min = 256, max = 1_048_576))]
    pub compression_threshold_bytes: usize,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            database_index: 0,
            pool_size: 16,
            connection_timeout_ms: 2000,
            command_timeout_ms: 1000,
            key_prefix: "omni".to_string(),
            compression_threshold_bytes: 8192,
        }
    }
}

impl RedisConfig {
    /// Tạo chuỗi URL kết nối Redis bọc trong [`SecretString`].
    #[must_use]
    pub fn connection_url(&self) -> SecretString {
        if let Some(ref pass) = self.password {
            SecretString::from(format!(
                "redis://:{}@{}:{}/{}",
                pass.expose_secret(),
                self.host,
                self.port,
                self.database_index
            ))
        } else {
            SecretString::from(format!(
                "redis://{}:{}/{}",
                self.host, self.port, self.database_index
            ))
        }
    }

    /// Tạo chuỗi URL kết nối Redis đã che mật khẩu phục vụ ghi log chẩn đoán an toàn.
    #[must_use]
    pub fn redacted_url(&self) -> String {
        if self.password.is_some() {
            format!(
                "redis://:[REDACTED]@{}:{}/{}",
                self.host, self.port, self.database_index
            )
        } else {
            format!(
                "redis://{}:{}/{}",
                self.host, self.port, self.database_index
            )
        }
    }
}
