//! Cache Client Settings & Safe `ConnectionInfo` Builder (`config.rs`).
//!
//! Converts [`RedisConfig`] into Redis [`redis::ConnectionInfo`] via safe builders,
//! completely avoiding plaintext URL formatting to eliminate secret leakage in logs.

use std::sync::Arc;

use omni_common::config::{
    AppConfig,
    redis::{RedisConfig, RedisCriticality, RedisProtocol, RedisTlsConfig},
};
use redis::{
    ConnectionAddr, ConnectionInfo, IntoConnectionInfo, ProtocolVersion, RedisConnectionInfo,
};
use secrecy::{ExposeSecret, SecretString};
use validator::Validate;

use crate::error::CacheError;

/// Cấu hình endpoint Redis vật lý.
#[derive(Debug, Clone)]
pub struct RedisEndpoint {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<SecretString>,
    pub tls: RedisTlsConfig,
}

impl RedisEndpoint {
    /// Trích xuất endpoint cache chính từ [`RedisConfig`].
    #[must_use]
    pub fn from_cache_config(cfg: &RedisConfig) -> Self {
        Self {
            host: cfg.host.clone(),
            port: cfg.port,
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            tls: cfg.tls.clone(),
        }
    }

    /// Trích xuất endpoint coordination từ [`RedisConfig`], hoặc fallback về endpoint cache nếu `coordination = null`.
    #[must_use]
    pub fn from_coordination_config(cfg: &RedisConfig) -> Self {
        if let Some(ref coord) = cfg.coordination {
            Self {
                host: coord.host.clone(),
                port: coord.port,
                username: coord.username.clone(),
                password: coord.password.clone(),
                tls: coord.tls.clone(),
            }
        } else {
            Self::from_cache_config(cfg)
        }
    }
}

/// Thiết lập toàn diện cho `omni-cache` tích hợp với cấu hình microservice.
#[derive(Debug, Clone)]
pub struct CacheSettings {
    pub service_name: Arc<str>,
    pub config: RedisConfig,
    pub request_timeout_secs: u64,
}

impl CacheSettings {
    /// Khởi tạo [`CacheSettings`] từ [`AppConfig`].
    pub fn from_app_config<C: serde::Serialize + Validate + Send + Sync + 'static>(
        app_config: &AppConfig<C>,
    ) -> Result<Self, CacheError> {
        // Kiểm tra hợp lệ trước khi khởi tạo
        if let Err(e) = app_config.validate_all() {
            return Err(CacheError::InvalidKey(format!(
                "Application configuration validation failed: {e}"
            )));
        }

        Ok(Self {
            service_name: Arc::from(app_config.service.name.as_str()),
            config: app_config.redis.clone(),
            request_timeout_secs: app_config.server.request_timeout_secs,
        })
    }

    /// Dựng [`ConnectionInfo`] an toàn không qua URL cho một endpoint chỉ định.
    pub fn build_connection_info(&self, ep: &RedisEndpoint) -> Result<ConnectionInfo, CacheError> {
        let addr = if ep.tls.enabled {
            ConnectionAddr::TcpTls {
                host: ep.host.clone(),
                port: ep.port,
                insecure: false,
                tls_params: None,
            }
        } else {
            ConnectionAddr::Tcp(ep.host.clone(), ep.port)
        };

        let mut redis = RedisConnectionInfo::default()
            .set_db(i64::from(self.config.database_index))
            .set_protocol(match self.config.protocol {
                RedisProtocol::Resp2 => ProtocolVersion::RESP2,
                RedisProtocol::Resp3 => ProtocolVersion::RESP3,
            });

        if let Some(ref user) = ep.username {
            redis = redis.set_username(user);
        }
        if let Some(ref pass) = ep.password {
            redis = redis.set_password(pass.expose_secret());
        }

        let conn_info = addr
            .into_connection_info()
            .map_err(|e| CacheError::Unavailable(format!("Invalid Redis connection address: {e}")))?
            .set_redis_settings(redis);

        Ok(conn_info)
    }

    /// Trả về mức độ trọng yếu của Redis (`Critical` hay `NonCritical`).
    #[must_use]
    pub fn criticality(&self) -> RedisCriticality {
        self.config.criticality
    }
}
