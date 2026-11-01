//! Relational database connection pool configuration.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::serialize_redacted_secret;

/// Chế độ mã hóa kết nối TLS/SSL của PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DbSslMode {
    /// Không sử dụng SSL.
    Disable,
    /// Ưu tiên SSL, cho phép fallback về plaintext nếu server không hỗ trợ.
    #[default]
    Prefer,
    /// Bắt buộc kết nối SSL.
    Require,
    /// Kiểm tra tính hợp lệ của chứng chỉ Certificate Authority.
    VerifyCa,
    /// Kiểm tra toàn diện CA và tên Hostname trên chứng chỉ.
    VerifyFull,
}

impl DbSslMode {
    /// Trả về chuỗi đại diện query param của `sslmode`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }
}

/// Cấu hình kết nối cơ sở dữ liệu PostgreSQL (`omni-database`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Cờ kích hoạt kết nối cơ sở dữ liệu.
    pub enabled: bool,

    /// Địa chỉ máy chủ PostgreSQL.
    #[validate(length(min = 1, message = "database.host must not be empty"))]
    pub host: String,

    /// Cổng kết nối PostgreSQL (mặc định 5432).
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Tên tài khoản người dùng kết nối cơ sở dữ liệu.
    #[validate(length(min = 1, message = "database.username must not be empty"))]
    pub username: String,

    /// Mật khẩu cơ sở dữ liệu được bảo vệ an toàn bằng [`SecretString`].
    #[serde(serialize_with = "serialize_redacted_secret")]
    pub password: SecretString,

    /// Tên cơ sở dữ liệu kết nối.
    #[validate(length(min = 1, message = "database.database_name must not be empty"))]
    pub database_name: String,

    /// Chế độ mã hóa SSL/TLS.
    #[serde(default)]
    pub ssl_mode: DbSslMode,

    /// Số lượng kết nối tối đa trong Connection Pool.
    #[validate(range(min = 1, max = 500))]
    pub max_connections: u32,

    /// Số lượng kết nối duy trì tối thiểu trong Connection Pool.
    #[validate(range(min = 0, max = 500))]
    pub min_connections: u32,

    /// Thời gian tối đa chờ mượn kết nối từ Pool (giây).
    #[validate(range(min = 1, max = 120))]
    pub acquire_timeout_secs: u64,

    /// Thời gian đóng kết nối nhàn rỗi (giây).
    #[validate(range(min = 10, max = 3600))]
    pub idle_timeout_secs: u64,

    /// Thời gian sống tối đa của một kết nối vật lý (giây).
    #[validate(range(min = 60, max = 86400))]
    pub max_lifetime_secs: u64,

    /// Ngưỡng thời gian cảnh báo truy vấn chậm (mili-giây).
    #[validate(range(min = 10, max = 60000))]
    pub slow_query_threshold_ms: u64,

    /// Tự động thực thi migration khi khởi động Pod.
    pub auto_migrate: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "localhost".to_string(),
            port: 5432,
            username: "omni_user".to_string(),
            password: SecretString::from("dev_only_password_change_me"),
            database_name: "omni_db".to_string(),
            ssl_mode: DbSslMode::Prefer,
            max_connections: 20,
            min_connections: 2,
            acquire_timeout_secs: 5,
            idle_timeout_secs: 300,
            max_lifetime_secs: 1800,
            slow_query_threshold_ms: 200,
            auto_migrate: false,
        }
    }
}

impl DatabaseConfig {
    /// Tạo chuỗi URL kết nối đầy đủ bọc trong [`SecretString`].
    #[must_use]
    pub fn connection_url(&self) -> SecretString {
        SecretString::from(format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode.as_str()
        ))
    }

    /// Tạo chuỗi URL kết nối đã che mật khẩu phục vụ ghi log chẩn đoán an toàn.
    #[must_use]
    pub fn redacted_url(&self) -> String {
        format!(
            "postgres://{}:[REDACTED]@{}:{}/{}?sslmode={}",
            self.username,
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode.as_str()
        )
    }
}
