//! 5-layer hierarchical configuration loader with environment variable alias mapping.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use validator::Validate;

use crate::config::AppConfig;
use crate::config::env::AppEnvironment;
use crate::config::error::ConfigError;

/// Helper phân tích giá trị chuỗi thành [`config::Value`] thích hợp (boolean, integer, float, hoặc string).
fn parse_value(s: &str) -> config::Value {
    if s.eq_ignore_ascii_case("true") {
        return config::Value::new(None, config::ValueKind::Boolean(true));
    }
    if s.eq_ignore_ascii_case("false") {
        return config::Value::new(None, config::ValueKind::Boolean(false));
    }
    if let Ok(i) = s.parse::<i64>() {
        return config::Value::new(None, config::ValueKind::I64(i));
    }
    if let Ok(f) = s.parse::<f64>()
        && !f.is_nan()
        && !f.is_infinite()
    {
        return config::Value::new(None, config::ValueKind::Float(f));
    }
    config::Value::new(None, config::ValueKind::String(s.to_string()))
}

/// Trình nạp cấu hình đa tầng theo thứ tự phân cấp 5 tầng (5-Layer Resolution).
#[derive(Debug, Clone)]
pub struct ConfigLoader {
    /// Tên định danh của microservice.
    service_name: String,
    /// Môi trường thực thi được ghi đè tường minh (nếu có).
    environment: Option<AppEnvironment>,
    /// Đường dẫn thư mục chứa cấu hình `config/` (nếu không cung cấp, tự tìm kiếm).
    config_dir: Option<PathBuf>,
    /// Cờ vô hiệu hóa nạp file `.env` (mặc định cho phép ở môi trường Dev).
    dotenv_disabled: bool,
    /// Bản đồ biến môi trường giả lập trong bộ nhớ phục vụ kiểm thử song song an toàn.
    in_memory_env: HashMap<String, String>,
    /// Danh sách các giá trị ghi đè trực tiếp (Tầng 5).
    overrides: HashMap<String, config::Value>,
}

impl ConfigLoader {
    /// Khởi tạo [`ConfigLoader`] mới với tên dịch vụ chỉ định.
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            environment: None,
            config_dir: None,
            dotenv_disabled: false,
            in_memory_env: HashMap::new(),
            overrides: HashMap::new(),
        }
    }

    /// Khởi tạo [`ConfigLoader`] phục vụ kiểm thử: mặc định tắt `.env` cục bộ.
    #[must_use]
    pub fn for_test(service_name: impl Into<String>) -> Self {
        Self::new(service_name).with_dotenv_disabled()
    }

    /// Chỉ định tường minh môi trường hoạt động.
    #[must_use]
    pub fn with_environment(mut self, env: AppEnvironment) -> Self {
        self.environment = Some(env);
        self
    }

    /// Chỉ định thư mục cấu hình tùy chỉnh.
    #[must_use]
    pub fn with_config_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config_dir = Some(dir.into());
        self
    }

    /// Vô hiệu hóa việc nạp file `.env`.
    #[must_use]
    pub fn with_dotenv_disabled(mut self) -> Self {
        self.dotenv_disabled = true;
        self
    }

    /// Ghi nhận một biến môi trường giả lập trong bộ nhớ (an toàn trong Rust 2024 đa luồng).
    #[must_use]
    pub fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.in_memory_env.insert(key.into(), value.into());
        self
    }

    /// Ghi nhận danh sách biến môi trường giả lập trong bộ nhớ.
    #[must_use]
    pub fn with_env_vars<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.in_memory_env.insert(k.into(), v.into());
        }
        self
    }

    /// Ghi đè một giá trị cấu hình trực tiếp (Tầng 5 - Programmatic Override).
    #[must_use]
    pub fn with_override<T: Into<config::Value>>(
        mut self,
        key: impl Into<String>,
        value: T,
    ) -> Self {
        self.overrides.insert(key.into(), value.into());
        self
    }

    /// Nạp và kiểm tra hợp lệ toàn diện cấu hình vào cấu trúc [`AppConfig<C>`].
    pub fn load<C>(self) -> Result<AppConfig<C>, ConfigError>
    where
        C: serde::de::DeserializeOwned + Validate + Default,
    {
        // 1. Xác định môi trường thực thi (AppEnvironment)
        let env = self.resolve_environment()?;

        // 2. Nạp `.env` nếu ở môi trường Dev và không bị vô hiệu hóa
        if !self.dotenv_disabled && env.allows_dotenv() {
            let _ = dotenvy::dotenv();
        }

        // 3. Khởi tạo ConfigBuilder
        let mut builder = config::Config::builder();

        // Tầng 1: Compiled-in Safe Defaults
        builder = builder
            .set_default("service.name", self.service_name.clone())
            .map_err(|e| ConfigError::Deserialization(e.to_string()))?
            .set_default("service.environment", env.as_str())
            .map_err(|e| ConfigError::Deserialization(e.to_string()))?;

        // Tìm kiếm thư mục cấu hình
        let config_dir = self.find_config_dir();

        // Tầng 2: Base YAML (`config/base.yaml`)
        if let Some(ref dir) = config_dir {
            let base_yaml = dir.join("base.yaml");
            if base_yaml.exists() {
                builder = builder.add_source(config::File::from(base_yaml).required(false));
            }
        }

        // Tầng 3: Environment Profile YAML (`config/{env}/default.yaml` hoặc `config/{env}.yaml`)
        if let Some(ref dir) = config_dir {
            let env_profile = dir.join(env.as_str()).join("default.yaml");
            if env_profile.exists() {
                builder = builder.add_source(config::File::from(env_profile).required(false));
            } else {
                let env_yaml = dir.join(format!("{}.yaml", env.as_str()));
                if env_yaml.exists() {
                    builder = builder.add_source(config::File::from(env_yaml).required(false));
                }
            }
        }

        // Đảm bảo tên dịch vụ từ ConfigLoader::new(service_name) có hiệu lực
        // ghi đè lên tên mẫu mặc định "omni-service" trong base.yaml
        builder = builder
            .set_override("service.name", self.service_name.clone())
            .map_err(|e| ConfigError::Deserialization(e.to_string()))?
            .set_override("service.environment", env.as_str())
            .map_err(|e| ConfigError::Deserialization(e.to_string()))?;

        // Tầng 4: System Environment Variables & Fallbacks
        // A. Xử lý các bí danh / fallback tiêu chuẩn công nghiệp nếu OMNI__* chưa đặt
        self.apply_standard_fallbacks(&mut builder)?;

        // B. Nạp biến môi trường hệ thống thực tế có tiền tố OMNI__
        builder = builder.add_source(
            config::Environment::with_prefix("OMNI")
                .separator("__")
                .try_parsing(true),
        );

        // C. Nạp biến môi trường giả lập từ `in_memory_env`
        for (k, v) in &self.in_memory_env {
            if let Some(stripped) = k.strip_prefix("OMNI__") {
                let parts: Vec<&str> = stripped.split("__").collect();
                let lower_parts: Vec<String> =
                    parts.iter().map(|p| p.to_ascii_lowercase()).collect();
                let path = lower_parts.join(".");
                let parsed_val = parse_value(v);
                builder = builder
                    .set_override(path, parsed_val)
                    .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
            }
        }

        // Tầng 5: Programmatic Overrides
        for (k, v) in self.overrides {
            builder = builder
                .set_override(k, v)
                .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
        }

        // Deserialization & Fail-Fast Validation
        let built = builder
            .build()
            .map_err(|e| ConfigError::Deserialization(e.to_string()))?;

        let app_config: AppConfig<C> = built
            .try_deserialize()
            .map_err(|e| ConfigError::Deserialization(e.to_string()))?;

        // Chạy quy trình kiểm tra hợp lệ 2 pha (Field-Level + Cross-Field & Prod Guardrails)
        app_config.validate_all()?;

        Ok(app_config)
    }

    fn resolve_environment(&self) -> Result<AppEnvironment, ConfigError> {
        if let Some(env) = self.environment {
            return Ok(env);
        }

        let env_str = self
            .get_env_lookup("OMNI__SERVICE__ENVIRONMENT")
            .or_else(|| self.get_env_lookup("APP_ENV"))
            .or_else(|| self.get_env_lookup("RUST_ENV"))
            .or_else(|| self.get_env_lookup("ENV"));

        if let Some(s) = env_str {
            AppEnvironment::from_str(&s).map_err(ConfigError::InvalidEnvironment)
        } else {
            Ok(AppEnvironment::Dev)
        }
    }

    fn get_env_lookup(&self, key: &str) -> Option<String> {
        self.in_memory_env
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .filter(|s| !s.trim().is_empty())
    }

    fn apply_standard_fallbacks(
        &self,
        builder: &mut config::ConfigBuilder<config::builder::DefaultState>,
    ) -> Result<(), ConfigError> {
        // Fallback cho logging.log_level: OMNI__LOGGING__LOG_LEVEL -> RUST_LOG -> LOG_LEVEL
        if self.get_env_lookup("OMNI__LOGGING__LOG_LEVEL").is_none()
            && let Some(val) = self
                .get_env_lookup("RUST_LOG")
                .or_else(|| self.get_env_lookup("LOG_LEVEL"))
        {
            *builder = std::mem::take(builder)
                .set_override("logging.log_level", val)
                .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
        }

        // Fallback cho logging.otlp_endpoint: OMNI__LOGGING__OTLP_ENDPOINT -> OTEL_EXPORTER_OTLP_ENDPOINT -> OTLP_ENDPOINT
        if self
            .get_env_lookup("OMNI__LOGGING__OTLP_ENDPOINT")
            .is_none()
            && let Some(val) = self
                .get_env_lookup("OTEL_EXPORTER_OTLP_ENDPOINT")
                .or_else(|| self.get_env_lookup("OTLP_ENDPOINT"))
            && !val.trim().is_empty()
        {
            *builder = std::mem::take(builder)
                .set_override("logging.otlp_endpoint", val)
                .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
        }

        // Fallback cho Kubernetes Downward API:
        // k8s.pod_name: OMNI__K8S__POD_NAME -> K8S_POD_NAME -> HOSTNAME
        if self.get_env_lookup("OMNI__K8S__POD_NAME").is_none()
            && let Some(val) = self
                .get_env_lookup("K8S_POD_NAME")
                .or_else(|| self.get_env_lookup("HOSTNAME"))
        {
            *builder = std::mem::take(builder)
                .set_override("k8s.pod_name", val)
                .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
        }

        // k8s.namespace: OMNI__K8S__NAMESPACE -> K8S_NAMESPACE
        if self.get_env_lookup("OMNI__K8S__NAMESPACE").is_none()
            && let Some(val) = self.get_env_lookup("K8S_NAMESPACE")
        {
            *builder = std::mem::take(builder)
                .set_override("k8s.namespace", val)
                .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
        }

        // k8s.node_name: OMNI__K8S__NODE_NAME -> K8S_NODE_NAME
        if self.get_env_lookup("OMNI__K8S__NODE_NAME").is_none()
            && let Some(val) = self.get_env_lookup("K8S_NODE_NAME")
        {
            *builder = std::mem::take(builder)
                .set_override("k8s.node_name", val)
                .map_err(|e| ConfigError::Deserialization(e.to_string()))?;
        }

        Ok(())
    }

    fn find_config_dir(&self) -> Option<PathBuf> {
        if let Some(ref dir) = self.config_dir
            && dir.exists()
        {
            return Some(dir.clone());
        }

        if let Some(custom) = self.get_env_lookup("OMNI_CONFIG_DIR") {
            let p = PathBuf::from(custom);
            if p.exists() {
                return Some(p);
            }
        }

        let candidates = [
            PathBuf::from("config"),
            PathBuf::from("../config"),
            PathBuf::from("../../config"),
        ];

        candidates.into_iter().find(|c| c.exists() && c.is_dir())
    }
}
