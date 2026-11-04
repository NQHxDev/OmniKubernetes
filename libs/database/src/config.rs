//! Connection pool and PostgreSQL connection options configuration bridge.
//!
//! Converts [`omni_common::config::DatabaseConfig`] into [`PgConnectOptions`]
//! and [`PgPoolOptions`] using direct builders without materializing plaintext connection URLs.

use std::time::Duration;

use omni_common::config::{DatabaseConfig, DbSslMode};
use secrecy::ExposeSecret;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::{ConnectOptions as _, Connection as _};

/// Hệ số ngẫu nhiên trong khoảng `[1 - pct/100, 1.0]`: chỉ rút ngắn, không bao giờ vượt cấu hình.
fn shrink_factor(pct: u8) -> f64 {
    use std::hash::{BuildHasher, RandomState};
    let r = RandomState::new().hash_one(std::process::id());
    let unit = f64::from(u32::try_from(r % 10_000).unwrap_or(0)) / 10_000.0;
    1.0 - (f64::from(pct.min(50)) / 100.0) * unit
}

const fn map_ssl_mode(mode: DbSslMode) -> PgSslMode {
    match mode {
        DbSslMode::Disable => PgSslMode::Disable,
        DbSslMode::Prefer => PgSslMode::Prefer,
        DbSslMode::Require => PgSslMode::Require,
        DbSslMode::VerifyCa => PgSslMode::VerifyCa,
        DbSslMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

/// Thiết lập nội bộ đã được tiền xử lý và áp dụng jitter của `omni-database`.
#[derive(Debug, Clone)]
pub struct DatabaseSettings {
    /// Cấu hình thô từ hệ thống cấu hình chung.
    pub raw: DatabaseConfig,
    /// Tên ứng dụng gửi sang PostgreSQL (đã chuẩn hóa `[a-z0-9-]`, tối đa 30 ký tự).
    pub application_name: String,
    /// Thời gian sống tối đa của một kết nối đã được rút ngắn ngẫu nhiên qua jitter.
    pub max_lifetime: Duration,
}

impl DatabaseSettings {
    /// Khởi tạo [`DatabaseSettings`] mới từ [`DatabaseConfig`] và tên microservice.
    #[must_use]
    pub fn new(raw: DatabaseConfig, service_name: &str) -> Self {
        let app_name = if let Some(custom) = &raw.application_name {
            custom.clone()
        } else {
            let normalized: String = service_name
                .to_ascii_lowercase()
                .chars()
                .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                .take(30)
                .collect();
            if normalized.is_empty() {
                "omni-service".to_string()
            } else {
                normalized
            }
        };

        let jitter_factor = shrink_factor(raw.max_lifetime_jitter_pct);
        let max_lifetime = Duration::from_secs(raw.max_lifetime_secs).mul_f64(jitter_factor);

        Self {
            raw,
            application_name: app_name,
            max_lifetime,
        }
    }

    /// Trả về tên ứng dụng đã chuẩn hóa.
    #[must_use]
    pub fn application_name(&self) -> &str {
        &self.application_name
    }

    /// Tạo [`PgConnectOptions`] phục vụ runtime kết nối thông qua builder.
    #[must_use]
    pub fn connect_options(&self) -> PgConnectOptions {
        let cfg = &self.raw;
        let mut opts = PgConnectOptions::new_without_pgpass()
            .host(&cfg.host)
            .port(cfg.port)
            .username(&cfg.username)
            .password(cfg.password.expose_secret())
            .database(&cfg.database_name)
            .ssl_mode(map_ssl_mode(cfg.ssl_mode))
            .application_name(&self.application_name)
            .statement_cache_capacity(cfg.statement_cache_capacity)
            .log_statements(log::LevelFilter::Debug)
            .log_slow_statements(
                log::LevelFilter::Warn,
                Duration::from_millis(cfg.slow_query_threshold_ms),
            );

        if !cfg.pgbouncer_mode {
            opts = opts.options([
                ("statement_timeout", cfg.statement_timeout_ms.to_string()),
                ("lock_timeout", cfg.lock_timeout_ms.to_string()),
                (
                    "idle_in_transaction_session_timeout",
                    cfg.idle_in_transaction_timeout_ms.to_string(),
                ),
                ("TimeZone", "UTC".to_string()),
            ]);
        }

        if let Some(ca) = &cfg.ssl_root_cert_path {
            opts = opts.ssl_root_cert(ca);
        }

        opts
    }

    /// Tạo [`PgPoolOptions`] với chiến lược kiểm tra nhịp tim keepalive 3 lớp.
    #[must_use]
    pub fn pool_options(&self) -> PgPoolOptions {
        let cfg = &self.raw;
        let ping_after = Duration::from_secs(cfg.ping_idle_threshold_secs);

        PgPoolOptions::new()
            .max_connections(cfg.max_connections)
            .min_connections(cfg.min_connections)
            .acquire_timeout(Duration::from_secs(cfg.acquire_timeout_secs))
            .acquire_slow_threshold(Duration::from_millis(cfg.slow_acquire_threshold_ms))
            .acquire_slow_level(log::LevelFilter::Warn)
            .idle_timeout(Duration::from_secs(cfg.idle_timeout_secs))
            .max_lifetime(self.max_lifetime)
            .test_before_acquire(false)
            .before_acquire(move |conn, meta| {
                Box::pin(async move {
                    if meta.idle_for >= ping_after {
                        conn.ping().await?;
                    }
                    Ok(true)
                })
            })
    }

    /// Tạo [`PgConnectOptions`] phục vụ thực thi Database Migrations trực tiếp.
    #[must_use]
    pub fn migration_connect_options(&self) -> PgConnectOptions {
        let cfg = &self.raw;
        let host = cfg.migration.host.as_deref().unwrap_or(&cfg.host);
        let port = cfg.migration.port.unwrap_or(cfg.port);
        let username = cfg.migration.username.as_deref().unwrap_or(&cfg.username);
        let password = cfg
            .migration
            .password
            .as_ref()
            .map_or_else(|| cfg.password.expose_secret(), ExposeSecret::expose_secret);

        let mut opts = PgConnectOptions::new_without_pgpass()
            .host(host)
            .port(port)
            .username(username)
            .password(password)
            .database(&cfg.database_name)
            .ssl_mode(map_ssl_mode(cfg.ssl_mode))
            .application_name(&format!("{}-migration", self.application_name))
            .statement_cache_capacity(0);

        if !cfg.pgbouncer_mode {
            let mut options = vec![
                ("lock_timeout", cfg.migration.lock_timeout_ms.to_string()),
                ("TimeZone", "UTC".to_string()),
            ];
            if cfg.migration.statement_timeout_ms > 0 {
                options.push((
                    "statement_timeout",
                    cfg.migration.statement_timeout_ms.to_string(),
                ));
            }
            opts = opts.options(options);
        }

        if let Some(ca) = &cfg.ssl_root_cert_path {
            opts = opts.ssl_root_cert(ca);
        }

        opts
    }
}
