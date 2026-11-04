//! Internationalization and localization configuration.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Cấu hình đa ngôn ngữ Mozilla Fluent (`omni-common::i18n`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct I18nConfig {
    /// Đường dẫn thư mục chứa các tập tin Mozilla Fluent `.ftl`.
    #[validate(length(min = 1))]
    pub locales_dir: String,

    /// Mã ngôn ngữ mặc định (BCP 47) khi không có header `Accept-Language`.
    #[validate(length(min = 2, max = 35))]
    pub default_locale: String,

    /// Danh sách các mã ngôn ngữ được hệ thống hỗ trợ.
    #[validate(length(min = 1))]
    pub supported_locales: Vec<String>,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            locales_dir: "libs/common/locales".to_string(),
            default_locale: "vi-VN".to_string(),
            supported_locales: vec![
                "vi-VN".to_string(),
                "en-US".to_string(),
                "ja-JP".to_string(),
            ],
        }
    }
}
