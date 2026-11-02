//! Argon2id password hashing and constant-time verification with Tokio non-blocking delegation.

use argon2::{
    Argon2, Params, Version,
    password_hash::{Error as PhError, PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use secrecy::{ExposeSecret, SecretString};

use crate::{config::Argon2Config, error::AuthError, instrument::CryptoTimer};

/// Adapter băm và so khớp mật khẩu chuẩn hóa OWASP bằng Argon2id.
#[derive(Debug, Clone)]
pub struct PasswordSecurity {
    params: Params,
}

impl PasswordSecurity {
    /// Khởi tạo [`PasswordSecurity`] mới với tham số bộ nhớ, vòng lặp và luồng song song.
    pub fn new(memory_kib: u32, iterations: u32, parallelism: u32) -> Result<Self, AuthError> {
        let params = Params::new(memory_kib, iterations, parallelism, None)
            .map_err(|e| AuthError::CryptoConfiguration(e.to_string()))?;
        Ok(Self { params })
    }

    /// Khởi tạo [`PasswordSecurity`] từ [`Argon2Config`].
    pub fn from_config(cfg: &Argon2Config) -> Result<Self, AuthError> {
        Self::new(cfg.memory_kib, cfg.iterations, cfg.parallelism)
    }

    /// Băm mật khẩu bất đồng bộ, ủy quyền tính toán sang blocking thread pool của Tokio
    /// để chống nghẽn Event Loop (Worker Thread Starvation).
    pub async fn hash_password(&self, password: SecretString) -> Result<String, AuthError> {
        let params = self.params.clone();
        tokio::task::spawn_blocking(move || {
            let timer = CryptoTimer::start("argon2_hash");
            let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);
            let hash = argon2
                .hash_password(password.expose_secret().as_bytes())
                .map_err(|e| AuthError::PasswordHashingFailed(e.to_string()))?;
            timer.complete();
            Ok(hash.to_string())
        })
        .await
        .map_err(|join_err| AuthError::InternalTaskPanic(join_err.to_string()))?
    }

    /// So khớp mật khẩu với hash lưu trong database (Constant-time execution).
    pub async fn verify_password(
        &self,
        password: SecretString,
        password_hash: String,
    ) -> Result<bool, AuthError> {
        tokio::task::spawn_blocking(move || {
            let timer = CryptoTimer::start("argon2_verify");
            let parsed_hash = PasswordHash::new(&password_hash)
                .map_err(|e| AuthError::InvalidHashFormat(e.to_string()))?;
            let argon2 = Argon2::default();
            let res =
                match argon2.verify_password(password.expose_secret().as_bytes(), &parsed_hash) {
                    Ok(()) => Ok(true),
                    Err(PhError::PasswordInvalid) => Ok(false),
                    Err(e) => Err(AuthError::PasswordVerificationFailed(e.to_string())),
                };
            timer.complete();
            res
        })
        .await
        .map_err(|join_err| AuthError::InternalTaskPanic(join_err.to_string()))?
    }
}
