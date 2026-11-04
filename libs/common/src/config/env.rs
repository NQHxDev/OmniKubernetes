//! Runtime environment specification and Kubernetes Downward API metadata.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Môi trường vận hành chuẩn hóa của hệ thống `OmniKubernetes`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppEnvironment {
    /// Môi trường phát triển cục bộ trên máy lập trình viên hoặc Docker Compose.
    #[default]
    #[serde(alias = "local", alias = "development")]
    Dev,
    /// Môi trường kiểm thử tích hợp / nghiệm thu trước khi lên Production.
    #[serde(alias = "uat", alias = "preprod")]
    Staging,
    /// Môi trường vận hành thực tế phục vụ người dùng cuối (Áp dụng các quy tắc bảo mật nghiêm ngặt nhất).
    #[serde(alias = "production")]
    Prod,
    /// Môi trường chạy kiểm thử tự động (Unit / Integration Tests).
    #[serde(alias = "testing")]
    Test,
}

impl AppEnvironment {
    /// Tên thư mục cấu hình tương ứng trong `config/{env}/default.yaml`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
            Self::Test => "test",
        }
    }

    /// Kiểm tra có phải môi trường Production hay không.
    #[must_use]
    pub const fn is_prod(&self) -> bool {
        matches!(self, Self::Prod)
    }

    /// Kiểm tra có phải môi trường Staging hay không.
    #[must_use]
    pub const fn is_staging(&self) -> bool {
        matches!(self, Self::Staging)
    }

    /// Kiểm tra có phải môi trường Development hay không.
    #[must_use]
    pub const fn is_dev(&self) -> bool {
        matches!(self, Self::Dev)
    }

    /// Kiểm tra có cho phép nạp tập tin `.env` cục bộ hay không (Chỉ bật ở Dev).
    #[must_use]
    pub const fn allows_dotenv(&self) -> bool {
        matches!(self, Self::Dev)
    }
}

impl fmt::Display for AppEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AppEnvironment {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dev" | "local" | "development" => Ok(Self::Dev),
            "staging" | "uat" | "preprod" => Ok(Self::Staging),
            "prod" | "production" => Ok(Self::Prod),
            "test" | "testing" => Ok(Self::Test),
            other => Err(format!(
                "Invalid environment '{other}'. Expected one of: dev, local, staging, prod, test"
            )),
        }
    }
}

/// Thông tin định danh Pod từ Kubernetes Downward API.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct K8sRuntimeMeta {
    /// Tên định danh Pod (`metadata.name`).
    #[serde(default)]
    pub pod_name: Option<String>,

    /// Không gian tên Kubernetes (`metadata.namespace`).
    #[serde(default)]
    pub namespace: Option<String>,

    /// Tên Node hạ tầng Kubernetes (`spec.nodeName`).
    #[serde(default)]
    pub node_name: Option<String>,
}
