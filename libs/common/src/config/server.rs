//! HTTP and gRPC network server configuration.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use validator::Validate;

use crate::config::env::AppEnvironment;

/// Cấu hình bảo mật Cross-Origin Resource Sharing (CORS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct CorsConfig {
    /// Danh sách origins được phép truy cập.
    #[serde(default = "default_allow_origins")]
    pub allow_origins: Vec<String>,

    /// Cho phép gửi kèm thông tin định danh (cookies, Authorization header).
    #[serde(default = "default_true")]
    pub allow_credentials: bool,

    /// Thời gian tối đa (giây) trình duyệt được phép lưu kết quả preflight check.
    #[serde(default = "default_max_age_secs")]
    pub max_age_secs: u64,
}

fn default_allow_origins() -> Vec<String> {
    vec!["http://localhost:3000".to_string()]
}

const fn default_true() -> bool {
    true
}

const fn default_max_age_secs() -> u64 {
    3600
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_origins: default_allow_origins(),
            allow_credentials: default_true(),
            max_age_secs: default_max_age_secs(),
        }
    }
}

/// Thông tin định danh dịch vụ microservice.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct ServiceMetaConfig {
    /// Tên định danh của microservice (ví dụ: `"order-service"`).
    #[validate(length(
        min = 2,
        max = 64,
        message = "service.name must be between 2 and 64 characters"
    ))]
    pub name: String,

    /// Phiên bản ngữ nghĩa `SemVer` của dịch vụ
    #[validate(length(min = 1, max = 32))]
    pub version: String,

    /// Môi trường triển khai hiện tại (`dev`, `staging`, `prod`, `test`).
    pub environment: AppEnvironment,

    /// Khu vực đám mây hoặc trung tâm dữ liệu (ví dụ: `"ap-southeast-1"`).
    #[validate(length(min = 1, max = 64))]
    pub region: String,
}

impl Default for ServiceMetaConfig {
    fn default() -> Self {
        Self {
            name: "omni-service".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: AppEnvironment::Dev,
            region: "ap-southeast-1".to_string(),
        }
    }
}

/// Cấu hình máy chủ mạng HTTP REST (Axum) và gRPC (Tonic).
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct HttpServerConfig {
    /// Địa chỉ IP lắng nghe (mặc định `0.0.0.0` trong container).
    pub host: IpAddr,

    /// Cổng phục vụ REST API (Bắt buộc >= 1024 để chạy với non-root user UID 65532).
    #[validate(range(
        min = 1024,
        max = 65535,
        message = "server.port must be in unprivileged range 1024..=65535"
    ))]
    pub port: u16,

    /// Cổng phục vụ gRPC nội bộ East-West traffic.
    #[validate(range(
        min = 1024,
        max = 65535,
        message = "server.grpc_port must be in unprivileged range 1024..=65535"
    ))]
    pub grpc_port: u16,

    /// Thời gian tối đa xử lý một HTTP request trước khi timeout (giây).
    #[validate(range(
        min = 1,
        max = 300,
        message = "server.request_timeout_secs must be between 1 and 300 seconds"
    ))]
    pub request_timeout_secs: u64,

    /// Thời gian cửa sổ chờ tắt duyên dáng khi nhận SIGTERM (giây).
    #[validate(range(
        min = 1,
        max = 120,
        message = "server.shutdown_timeout_secs must be between 1 and 120 seconds"
    ))]
    pub shutdown_timeout_secs: u64,

    /// Giới hạn kích thước tối đa của HTTP Request Body (bytes).
    #[validate(range(min = 1024, max = 104_857_600))]
    pub max_body_size_bytes: usize,

    /// Danh sách các CIDR mạng / Proxy tin cậy (như Ingress / Envoy).
    #[serde(default)]
    pub trusted_proxies: Vec<String>,

    /// Cấu hình bảo mật CORS.
    #[validate(nested)]
    #[serde(default)]
    pub cors: CorsConfig,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 8080,
            grpc_port: 50051,
            request_timeout_secs: 30,
            shutdown_timeout_secs: 15,
            max_body_size_bytes: 2 * 1024 * 1024, // 2 MiB
            trusted_proxies: Vec::new(),
            cors: CorsConfig::default(),
        }
    }
}

impl HttpServerConfig {
    /// Trả về [`SocketAddr`] cho máy chủ HTTP REST.
    #[must_use]
    pub const fn http_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    /// Trả về [`SocketAddr`] cho máy chủ gRPC nội bộ.
    #[must_use]
    pub const fn grpc_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.grpc_port)
    }
}
