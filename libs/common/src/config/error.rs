//! Configuration diagnostic and fail-fast validation errors.

use thiserror::Error;

/// Chi tiết một vi phạm ràng buộc cấu hình kèm gợi ý biến môi trường để khắc phục.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationIssue {
    /// Đường dẫn trường bị lỗi (ví dụ: `"database.max_connections"`).
    pub field_path: String,
    /// Mã lỗi chuẩn hóa (ví dụ: `"CFG_DB_POOL_BOUNDS"`).
    pub code: String,
    /// Thông điệp giải thích rõ nguyên nhân lỗi.
    pub message: String,
    /// Tên biến môi trường `OMNI__*` tương ứng để DevOps sửa ngay trên Kubernetes Deployment/ConfigMap.
    pub env_var_hint: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    /// Lỗi khi định danh chuỗi môi trường không hợp lệ.
    #[error("Invalid application environment specification: {0}")]
    InvalidEnvironment(String),

    /// Lỗi phân tích cú pháp hoặc deserialize từ các nguồn cấu hình.
    #[error("Failed to build or deserialize configuration layers: {0}")]
    Deserialization(String),

    /// Thất bại khi kiểm tra các ràng buộc hợp lệ của cấu hình.
    #[error("Configuration validation failed with {} error(s):\n{}", .0.len(), format_issues(.0))]
    ValidationFailed(Vec<ConfigValidationIssue>),
}

/// Chuyển đổi đường dẫn trường dạng dot-notation (ví dụ `"database.max_connections"`) sang tên biến môi trường `OMNI__*`.
#[must_use]
pub fn field_path_to_env_var(path: &str) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    let upper_parts: Vec<String> = parts.iter().map(|p| p.to_ascii_uppercase()).collect();
    format!("OMNI__{}", upper_parts.join("__"))
}

pub(crate) fn format_issues(issues: &[ConfigValidationIssue]) -> String {
    issues
        .iter()
        .enumerate()
        .map(|(idx, issue)| {
            format!(
                "  {}. [{}] Field '{}' (Env: {}): {}",
                idx + 1,
                issue.code,
                issue.field_path,
                issue.env_var_hint,
                issue.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
