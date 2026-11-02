//! Distributed caching and Redis configuration.

use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::config::secret::serialize_opt_redacted_secret;

/// Giao thức Redis giao tiếp mạng.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RedisProtocol {
    /// Giao thức RESP2 truyền thống.
    Resp2,
    /// Giao thức RESP3 hiện đại, hỗ trợ push notifications và phát hiện mất kết nối chủ động.
    #[default]
    Resp3,
}

impl RedisProtocol {
    /// Trả về chuỗi đại diện giao thức.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Resp2 => "resp2",
            Self::Resp3 => "resp3",
        }
    }
}

/// Thuật toán nén dữ liệu payload cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RedisCompression {
    /// Không nén.
    None,
    /// Nén bằng LZ4 (`lz4_flex`, Pure Rust).
    #[default]
    Lz4,
    /// Nén bằng Zstd (yêu cầu feature `zstd`).
    Zstd,
}

impl RedisCompression {
    /// Trả về chuỗi đại diện thuật toán nén.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lz4 => "lz4",
            Self::Zstd => "zstd",
        }
    }
}

/// Mức độ trọng yếu của Redis đối với dịch vụ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RedisCriticality {
    /// Trọng yếu sống còn (lỗi Redis làm `/readyz` trả về 503).
    Critical,
    /// Phụ thuộc thứ cấp (lỗi Redis cho phép fail-open, `/readyz` trả về 200 degraded).
    #[default]
    NonCritical,
}

impl RedisCriticality {
    /// Trả về chuỗi đại diện mức độ trọng yếu.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::NonCritical => "non_critical",
        }
    }
}

/// Cấu hình mã hóa đường truyền TLS cho Redis.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisTlsConfig {
    /// Bật/tắt kết nối mã hóa qua TLS/SSL.
    pub enabled: bool,
    /// Đường dẫn tới file chứng chỉ CA PEM riêng biệt (tùy chọn).
    pub ca_cert_path: Option<PathBuf>,
    /// Đường dẫn tới file chứng chỉ Client cho mTLS (tùy chọn).
    pub client_cert_path: Option<PathBuf>,
    /// Đường dẫn tới file private key Client cho mTLS (tùy chọn).
    pub client_key_path: Option<PathBuf>,
}

/// Cấu hình tự động kết nối lại ngầm của `ConnectionManager`.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisReconnectConfig {
    /// Số lần thử kết nối lại tối đa trước khi báo lỗi.
    #[validate(range(min = 1, max = 50))]
    pub max_retries: u32,
    /// Thời gian chờ tối thiểu giữa các lần kết nối lại (mili-giây).
    #[validate(range(min = 10, max = 10000))]
    pub min_delay_ms: u64,
    /// Thời gian chờ tối đa giữa các lần kết nối lại (mili-giây).
    #[validate(range(min = 100, max = 60000))]
    pub max_delay_ms: u64,
}

impl Default for RedisReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: 6,
            min_delay_ms: 100,
            max_delay_ms: 2000,
        }
    }
}

/// Cấu hình thử lại kết nối Redis khi khởi động (Startup Connect Retry).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisConnectRetryConfig {
    /// Số lần thử lại tối đa lúc khởi động.
    #[validate(range(min = 1, max = 30))]
    pub max_attempts: u32,
    /// Thời gian chờ khởi điểm (mili-giây).
    #[validate(range(min = 50, max = 5000))]
    pub initial_backoff_ms: u64,
    /// Thời gian chờ tối đa giữa các lần thử (mili-giây).
    #[validate(range(min = 200, max = 30000))]
    pub max_backoff_ms: u64,
}

impl Default for RedisConnectRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            initial_backoff_ms: 250,
            max_backoff_ms: 5000,
        }
    }
}

/// Cấu hình Cache Sentinel giám sát nhịp tim và tự động bật Bypass.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisSentinelConfig {
    /// Chu kỳ thực hiện lệnh PING kiểm tra sức khỏe Redis (giây).
    #[validate(range(min = 1, max = 60))]
    pub interval_secs: u64,
    /// Thời gian chờ phản hồi lệnh PING tối đa (mili-giây).
    #[validate(range(min = 50, max = 5000))]
    pub ping_timeout_ms: u64,
    /// Ngưỡng số lần PING thất bại liên tiếp để kích hoạt chế độ Bypass (hysteresis).
    #[validate(range(min = 1, max = 10))]
    pub failure_threshold: u32,
}

impl Default for RedisSentinelConfig {
    fn default() -> Self {
        Self {
            interval_secs: 5,
            ping_timeout_ms: 500,
            failure_threshold: 3,
        }
    }
}

/// Tham số mặc định cho tầng Caching (`TypedCache`).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisCacheDefaultsConfig {
    /// Thời gian sống mặc định của dữ liệu trong cache (giây).
    #[validate(range(min = 1, max = 604_800))]
    pub default_ttl_secs: u64,
    /// Trần thời gian sống tối đa của mọi key trong cache (giây).
    #[validate(range(min = 60, max = 604_800))]
    pub max_ttl_secs: u64,
    /// Tỉ lệ phần trăm áp dụng Jitter ngẫu nhiên cho TTL (0–50%).
    #[validate(range(min = 0, max = 50))]
    pub ttl_jitter_pct: u8,
    /// Hệ số nhạy beta cho thuật toán `XFetch` (0 = tắt `XFetch`).
    #[validate(range(min = 0.0, max = 10.0))]
    pub xfetch_beta: f64,
    /// Cửa sổ thời gian cho phép trả stale dữ liệu khi đang revalidate hoặc gặp lỗi (giây).
    #[validate(range(min = 0, max = 3600))]
    pub stale_ttl_secs: u64,
    /// Thời gian sống của bản ghi negative cache khi record không tồn tại (giây).
    #[validate(range(min = 1, max = 300))]
    pub negative_ttl_secs: u64,
    /// Thời gian chờ loader nạp dữ liệu từ database tối đa (mili-giây).
    #[validate(range(min = 100, max = 60000))]
    pub loader_timeout_ms: u64,
    /// Thời gian sống của distributed lease (mili-giây).
    #[validate(range(min = 100, max = 60000))]
    pub lease_ttl_ms: u64,
    /// Thời gian follower chờ cluster leader nạp xong trước khi tự nạp (mili-giây).
    #[validate(range(min = 10, max = 5000))]
    pub lease_wait_ms: u64,
    /// Dung lượng bộ đệm Invalidation Replay Buffer (số lượng key).
    #[validate(range(min = 100, max = 100_000))]
    pub invalidation_replay_capacity: usize,
}

impl Default for RedisCacheDefaultsConfig {
    fn default() -> Self {
        Self {
            default_ttl_secs: 300,
            max_ttl_secs: 86400,
            ttl_jitter_pct: 10,
            xfetch_beta: 1.0,
            stale_ttl_secs: 60,
            negative_ttl_secs: 30,
            loader_timeout_ms: 3000,
            lease_ttl_ms: 4000,
            lease_wait_ms: 200,
            invalidation_replay_capacity: 10000,
        }
    }
}

/// Tham số mặc định cho tầng Distributed Lock.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisLockDefaultsConfig {
    /// Thời hạn mặc định của khóa phân tán (mili-giây).
    #[validate(range(min = 500, max = 300_000))]
    pub default_ttl_ms: u64,
    /// Tỉ lệ gia hạn nhịp tim của Watchdog (mặc định 0.33 tức mỗi ttl * 0.33).
    #[validate(range(min = 0.05, max = 0.5))]
    pub renew_interval_ratio: f64,
    /// Thời gian chờ acquire khóa tối đa (mili-giây, 0 = try-once).
    #[validate(range(min = 0, max = 60000))]
    pub acquire_wait_ms: u64,
    /// Độ trễ thử lại tối thiểu khi chờ khóa (mili-giây).
    #[validate(range(min = 5, max = 1000))]
    pub retry_min_delay_ms: u64,
    /// Độ trễ thử lại tối đa khi chờ khóa (mili-giây).
    #[validate(range(min = 20, max = 10000))]
    pub retry_max_delay_ms: u64,
    /// Hệ số trôi đồng hồ (clock drift factor).
    #[validate(range(min = 0.001, max = 0.1))]
    pub drift_factor: f64,
}

impl Default for RedisLockDefaultsConfig {
    fn default() -> Self {
        Self {
            default_ttl_ms: 10000,
            renew_interval_ratio: 0.33,
            acquire_wait_ms: 0,
            retry_min_delay_ms: 20,
            retry_max_delay_ms: 250,
            drift_factor: 0.01,
        }
    }
}

/// Cấu hình endpoint Redis độc lập (dành cho coordination tách biệt khỏi cache).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct RedisEndpointConfig {
    /// Địa chỉ máy chủ.
    #[validate(length(min = 1, message = "coordination.host must not be empty"))]
    pub host: String,
    /// Cổng kết nối (mặc định 6379).
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,
    /// ACL username (Redis 6+).
    pub username: Option<String>,
    /// Mật khẩu xác thực bọc trong [`SecretString`].
    #[serde(default, serialize_with = "serialize_opt_redacted_secret")]
    pub password: Option<SecretString>,
    /// Cấu hình TLS cho endpoint này.
    #[validate(nested)]
    pub tls: RedisTlsConfig,
}

impl Default for RedisEndpointConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 6379,
            username: None,
            password: None,
            tls: RedisTlsConfig::default(),
        }
    }
}

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

    /// Tên tài khoản ACL Redis 6+ (nếu `None`, mặc định sử dụng user `"default"`).
    pub username: Option<String>,

    /// Mật khẩu xác thực Redis (nếu có) được bảo vệ bằng [`SecretString`].
    #[serde(default, serialize_with = "serialize_opt_redacted_secret")]
    pub password: Option<SecretString>,

    /// Chỉ mục database Redis (mặc định 0, Cluster chỉ hỗ trợ DB 0).
    pub database_index: u8,

    /// Phiên bản giao thức Redis (mặc định RESP3).
    pub protocol: RedisProtocol,

    /// Cấu hình mã hóa đường truyền TLS.
    #[validate(nested)]
    pub tls: RedisTlsConfig,

    /// Số lượng kết nối multiplexed `ConnectionManager` chạy round-robin (1–16).
    #[validate(range(min = 1, max = 16))]
    pub connections: usize,

    /// Kích thước connection pool (bị deprecated, không còn ngữ nghĩa với `ConnectionManager`).
    #[validate(range(min = 1, max = 500))]
    pub pool_size: usize,

    /// Kích thước hàng đợi pipeline buffer trên mỗi kết nối `ConnectionManager`.
    #[validate(range(min = 16, max = 65536))]
    pub pipeline_buffer_size: usize,

    /// Thời gian chờ kết nối tối đa (mili-giây).
    #[validate(range(min = 100, max = 30000))]
    pub connection_timeout_ms: u64,

    /// Thời gian chờ lệnh thực thi tối đa (mili-giây).
    #[validate(range(min = 50, max = 30000))]
    pub command_timeout_ms: u64,

    /// Thời gian gửi gói tin TCP keepalive (giây).
    #[validate(range(min = 1, max = 300))]
    pub tcp_keepalive_secs: u64,

    /// Cấu hình kết nối lại ngầm của `ConnectionManager`.
    #[validate(nested)]
    pub reconnect: RedisReconnectConfig,

    /// Cấu hình thử lại kết nối khi khởi động.
    #[validate(nested)]
    pub connect_retry: RedisConnectRetryConfig,

    /// Tiền tố phân cách namespace key Redis.
    #[validate(length(min = 1, max = 16))]
    pub key_prefix: String,

    /// Khóa bí mật HMAC-SHA256 (≥ 32 bytes) dùng để băm các định danh nhạy cảm trong key.
    #[serde(default, serialize_with = "serialize_opt_redacted_secret")]
    pub key_hash_secret: Option<SecretString>,

    /// Thuật toán nén dữ liệu payload (mặc định LZ4).
    pub compression: RedisCompression,

    /// Ngưỡng nén dữ liệu tự động (bytes).
    #[validate(range(min = 256, max = 1_048_576))]
    pub compression_threshold_bytes: usize,

    /// Kích thước trần tối đa của payload sau nén (bytes, mặc định 1MB).
    #[validate(range(min = 1024, max = 10_485_760))]
    pub max_value_bytes: usize,

    /// Cấu hình Cache Sentinel giám sát sức khỏe.
    #[validate(nested)]
    pub sentinel: RedisSentinelConfig,

    /// Mức độ trọng yếu của Redis trong service (`critical` hoặc `non_critical`).
    pub criticality: RedisCriticality,

    /// Cờ tự động bỏ qua Redis (Bypass) khi Sentinel phát hiện Redis không khỏe mạnh.
    pub bypass_on_unhealthy: bool,

    /// Cấu hình mặc định cho các cache policies.
    #[validate(nested)]
    pub cache: RedisCacheDefaultsConfig,

    /// Cấu hình mặc định cho khóa phân tán.
    #[validate(nested)]
    pub lock: RedisLockDefaultsConfig,

    /// Endpoint điều phối độc lập (coordination), nếu `None` thì dùng chung endpoint cache.
    #[validate(nested)]
    pub coordination: Option<RedisEndpointConfig>,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "localhost".to_string(),
            port: 6379,
            username: None,
            password: None,
            database_index: 0,
            protocol: RedisProtocol::Resp3,
            tls: RedisTlsConfig::default(),
            connections: 2,
            pool_size: 16,
            pipeline_buffer_size: 1024,
            connection_timeout_ms: 2000,
            command_timeout_ms: 1000,
            tcp_keepalive_secs: 30,
            reconnect: RedisReconnectConfig::default(),
            connect_retry: RedisConnectRetryConfig::default(),
            key_prefix: "omni".to_string(),
            key_hash_secret: None,
            compression: RedisCompression::Lz4,
            compression_threshold_bytes: 8192,
            max_value_bytes: 1_048_576,
            sentinel: RedisSentinelConfig::default(),
            criticality: RedisCriticality::NonCritical,
            bypass_on_unhealthy: true,
            cache: RedisCacheDefaultsConfig::default(),
            lock: RedisLockDefaultsConfig::default(),
            coordination: None,
        }
    }
}

impl RedisConfig {
    /// Tạo chuỗi URL kết nối Redis bọc trong [`SecretString`].
    ///
    /// # Deprecated
    /// Phương thức này không an toàn khi mật khẩu chứa ký tự đặc biệt (`@`, `:`, `/`).
    /// Khuyến nghị sử dụng builder `ConnectionInfo` trong `omni-cache::config` thay thế.
    #[deprecated(
        since = "0.1.0",
        note = "Do not use connection URLs; construct ConnectionInfo using builder to avoid secret leak or URL encoding breakage"
    )]
    #[must_use]
    pub fn connection_url(&self) -> SecretString {
        let scheme = if self.tls.enabled { "rediss" } else { "redis" };
        if let Some(ref pass) = self.password {
            if let Some(ref user) = self.username {
                SecretString::from(format!(
                    "{scheme}://{user}:{}@{}:{}/{}",
                    pass.expose_secret(),
                    self.host,
                    self.port,
                    self.database_index
                ))
            } else {
                SecretString::from(format!(
                    "{scheme}://:{}@{}:{}/{}",
                    pass.expose_secret(),
                    self.host,
                    self.port,
                    self.database_index
                ))
            }
        } else {
            SecretString::from(format!(
                "{scheme}://{}:{}/{}",
                self.host, self.port, self.database_index
            ))
        }
    }

    /// Tạo chuỗi URL kết nối Redis đã che mật khẩu phục vụ ghi log chẩn đoán an toàn.
    #[must_use]
    pub fn redacted_url(&self) -> String {
        let scheme = if self.tls.enabled { "rediss" } else { "redis" };
        if self.password.is_some() {
            if let Some(ref user) = self.username {
                format!(
                    "{scheme}://{user}:[REDACTED]@{}:{}/{}",
                    self.host, self.port, self.database_index
                )
            } else {
                format!(
                    "{scheme}://:[REDACTED]@{}:{}/{}",
                    self.host, self.port, self.database_index
                )
            }
        } else {
            format!(
                "{scheme}://{}:{}/{}",
                self.host, self.port, self.database_index
            )
        }
    }
}
