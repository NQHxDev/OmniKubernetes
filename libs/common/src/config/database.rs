//! Relational database connection pool configuration.

use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::{serialize_opt_redacted_secret, serialize_redacted_secret};

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

/// Chế độ liên kết distributed trace context với truy vấn cơ sở dữ liệu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TraceCorrelation {
    /// Không gắn thông tin trace vào câu lệnh hoặc phiên.
    Off,
    /// Gắn `trace_id` vào `application_name` và biến phiên `omni.trace_id` trong `BEGIN` (khuyến nghị).
    #[default]
    Transaction,
    /// Gắn comment `/* trace_id=... */` vào từng câu lệnh SQL (chế độ sqlcommenter).
    Statement,
}

/// Chế độ thực thi Database Migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationMode {
    /// Không chạy hay kiểm tra migration.
    Disabled,
    /// Tự động áp dụng các migration chưa chạy (dev/staging hoặc K8s Job).
    Apply,
    /// Chỉ kiểm tra schema đã khớp với mã nguồn, từ chối khởi động nếu thiếu migration (khuyến nghị cho Production).
    #[default]
    Verify,
}

/// Cấu hình thử lại kết nối khi khởi tạo Pool (Bounded Exponential Backoff).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct ConnectRetryConfig {
    /// Số lần thử kết nối lại tối đa trước khi báo lỗi khởi động.
    #[validate(range(min = 1, max = 30))]
    pub max_attempts: u32,

    /// Khoảng thời gian chờ ban đầu (mili-giây).
    #[validate(range(min = 50, max = 10000))]
    pub initial_backoff_ms: u64,

    /// Trần thời gian chờ tối đa giữa các lần thử lại (mili-giây).
    #[validate(range(min = 500, max = 60000))]
    pub max_backoff_ms: u64,
}

impl Default for ConnectRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            initial_backoff_ms: 250,
            max_backoff_ms: 5000,
        }
    }
}

/// Cấu hình tác vụ nền Pool Sentinel giám sát sức khỏe kết nối.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct SentinelConfig {
    /// Chu kỳ thực thi kiểm tra nhịp tim (giây).
    #[validate(range(min = 1, max = 60))]
    pub interval_secs: u64,

    /// Thời gian tối đa cho mỗi lần ping (mili-giây).
    #[validate(range(min = 100, max = 10000))]
    pub ping_timeout_ms: u64,

    /// Số lần ping thất bại liên tiếp trước khi đánh dấu /readyz không lành mạnh.
    #[validate(range(min = 1, max = 10))]
    pub failure_threshold: u32,
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            interval_secs: 10,
            ping_timeout_ms: 1000,
            failure_threshold: 3,
        }
    }
}

/// Cấu hình riêng biệt cho quy trình thực thi Database Migrations.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct MigrationConfig {
    /// Chế độ migration: disabled, apply, hoặc verify.
    pub mode: MigrationMode,

    /// Địa chỉ máy chủ kết nối trực tiếp PostgreSQL (bỏ qua `PgBouncer`).
    pub host: Option<String>,

    /// Cổng kết nối trực tiếp PostgreSQL.
    pub port: Option<u16>,

    /// Tên tài khoản có quyền DDL (schema owner).
    pub username: Option<String>,

    /// Mật khẩu tài khoản DDL.
    #[serde(serialize_with = "serialize_opt_redacted_secret")]
    pub password: Option<SecretString>,

    /// Thời gian chờ acquire lock DDL (mili-giây).
    #[validate(range(min = 500, max = 60000))]
    pub lock_timeout_ms: u64,

    /// Thời gian chờ thực thi câu lệnh migration (0 = không giới hạn).
    pub statement_timeout_ms: u64,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            mode: MigrationMode::Verify,
            host: None,
            port: None,
            username: None,
            password: None,
            lock_timeout_ms: 5000,
            statement_timeout_ms: 0,
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

    /// Đường dẫn file chứng chỉ CA bundle cho chế độ verify-ca / verify-full.
    pub ssl_root_cert_path: Option<PathBuf>,

    /// Tên ứng dụng gửi sang PostgreSQL (mặc định kế thừa từ `service.name`).
    pub application_name: Option<String>,

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

    /// Phần trăm rút ngắn ngẫu nhiên `max_lifetime` để tránh dồn dập kết nối lại (0-50%).
    #[validate(range(min = 0, max = 50))]
    pub max_lifetime_jitter_pct: u8,

    /// Ngưỡng nhàn rỗi cần ping kiểm tra trước khi cấp kết nối (giây).
    #[validate(range(min = 1, max = 300))]
    pub ping_idle_threshold_secs: u64,

    /// Ngưỡng thời gian cảnh báo mượn kết nối chậm (mili-giây).
    #[validate(range(min = 10, max = 10000))]
    pub slow_acquire_threshold_ms: u64,

    /// Dung lượng bộ nhớ đệm Prepared Statements LRU trên từng kết nối.
    #[validate(range(min = 0, max = 1000))]
    pub statement_cache_capacity: usize,

    /// Thời gian tối đa thực thi một câu lệnh SQL phía máy chủ (mili-giây).
    #[validate(range(min = 0, max = 300_000))]
    pub statement_timeout_ms: u64,

    /// Thời gian tối đa chờ khóa bảng / dòng (mili-giây).
    #[validate(range(min = 100, max = 60000))]
    pub lock_timeout_ms: u64,

    /// Thời gian tối đa duy trì transaction nhàn rỗi phía máy chủ (mili-giây).
    #[validate(range(min = 1000, max = 300_000))]
    pub idle_in_transaction_timeout_ms: u64,

    /// Ngưỡng thời gian cảnh báo truy vấn chậm (mili-giây).
    #[validate(range(min = 10, max = 60000))]
    pub slow_query_threshold_ms: u64,

    /// Chế độ liên kết distributed trace context với câu lệnh/phiên SQL.
    pub trace_correlation: TraceCorrelation,

    /// Chế độ tương thích `PgBouncer` (vô hiệu hóa startup param `options`).
    pub pgbouncer_mode: bool,

    /// Cấu hình thử lại kết nối khi khởi động.
    #[validate(nested)]
    pub connect_retry: ConnectRetryConfig,

    /// Cấu hình Pool Sentinel kiểm tra định kỳ.
    #[validate(nested)]
    pub sentinel: SentinelConfig,

    /// Tự động thực thi migration khi khởi động Pod (cũ, ưu tiên migration.mode).
    pub auto_migrate: bool,

    /// Cấu hình chi tiết Database Migrations.
    #[validate(nested)]
    pub migration: MigrationConfig,
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
            ssl_root_cert_path: None,
            application_name: None,
            max_connections: 20,
            min_connections: 2,
            acquire_timeout_secs: 5,
            idle_timeout_secs: 300,
            max_lifetime_secs: 1800,
            max_lifetime_jitter_pct: 10,
            ping_idle_threshold_secs: 30,
            slow_acquire_threshold_ms: 500,
            statement_cache_capacity: 100,
            statement_timeout_ms: 10000,
            lock_timeout_ms: 3000,
            idle_in_transaction_timeout_ms: 30000,
            slow_query_threshold_ms: 200,
            trace_correlation: TraceCorrelation::Transaction,
            pgbouncer_mode: false,
            connect_retry: ConnectRetryConfig::default(),
            sentinel: SentinelConfig::default(),
            auto_migrate: false,
            migration: MigrationConfig::default(),
        }
    }
}

fn percent_encode_userinfo(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            other => {
                use std::fmt::Write as _;
                let _ = write!(encoded, "%{other:02X}");
            }
        }
    }
    encoded
}

impl DatabaseConfig {
    /// Xác định chế độ migration có hiệu lực (hỗ trợ tương thích ngược với `auto_migrate`).
    #[must_use]
    pub fn effective_migration_mode(&self) -> MigrationMode {
        if self.auto_migrate && self.migration.mode == MigrationMode::Verify {
            MigrationMode::Apply
        } else {
            self.migration.mode
        }
    }

    /// Tạo chuỗi URL kết nối đầy đủ bọc trong [`SecretString`].
    ///
    /// Mật khẩu và username được percent-encode tự động để tránh hỏng định dạng URL.
    #[must_use]
    pub fn connection_url(&self) -> SecretString {
        let user = percent_encode_userinfo(&self.username);
        let pass = percent_encode_userinfo(self.password.expose_secret());
        SecretString::from(format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            user,
            pass,
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode.as_str()
        ))
    }

    /// Tạo chuỗi URL kết nối đã che mật khẩu phục vụ ghi log chẩn đoán an toàn.
    #[must_use]
    pub fn redacted_url(&self) -> String {
        let user = percent_encode_userinfo(&self.username);
        format!(
            "postgres://{}:[REDACTED]@{}:{}/{}?sslmode={}",
            user,
            self.host,
            self.port,
            self.database_name,
            self.ssl_mode.as_str()
        )
    }
}
