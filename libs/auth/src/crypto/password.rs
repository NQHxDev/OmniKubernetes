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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_security_new_and_from_config() {
        let sec = PasswordSecurity::new(4096, 1, 1).expect("Init should succeed");
        assert!(format!("{sec:?}").contains("PasswordSecurity"));

        let cfg = Argon2Config {
            memory_kib: 4096,
            iterations: 1,
            parallelism: 1,
            ..Default::default()
        };
        assert!(PasswordSecurity::from_config(&cfg).is_ok());

        // Invalid parameters
        let invalid = PasswordSecurity::new(0, 0, 0);
        assert!(matches!(invalid, Err(AuthError::CryptoConfiguration(_))));
    }

    #[tokio::test]
    async fn test_hash_and_verify_password_lifecycle() {
        // Use minimal memory/iterations for fast unit testing
        let sec = PasswordSecurity::new(4096, 1, 1).unwrap();
        let password = SecretString::from("MySecureP@ssw0rd123!");

        let hash = sec
            .hash_password(password.clone())
            .await
            .expect("Hashing should succeed");
        assert!(hash.starts_with("$argon2id$"));

        // Matching password
        let match_res = sec
            .verify_password(password, hash.clone())
            .await
            .expect("Verification should run cleanly");
        assert!(match_res);

        // Incorrect password
        let wrong_password = SecretString::from("WrongPassword123!");
        let wrong_res = sec
            .verify_password(wrong_password, hash)
            .await
            .expect("Verification should run cleanly");
        assert!(!wrong_res);

        // Malformed hash
        let malformed_res = sec
            .verify_password(
                SecretString::from("any"),
                "invalid-hash-phc-format".to_string(),
            )
            .await;
        assert!(matches!(
            malformed_res,
            Err(AuthError::InvalidHashFormat(_))
        ));
    }
}
