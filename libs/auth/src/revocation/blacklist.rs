//! JTI Blacklist operations on distributed cache (omni-cache).

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::AuthError;

/// Bản ghi chi tiết lý do thu hồi mã thông báo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationEntry {
    pub jti: Uuid,
    pub revoked_at: i64,
    pub reason: String,
    pub expires_at: i64,
}

/// Trait trừu tượng hóa kho lưu trữ danh sách đen thu hồi JTI.
#[async_trait]
pub trait RevocationStore: Send + Sync + 'static {
    /// Đưa một `jti` vào danh sách đen thu hồi kèm thời hạn sống chính xác bằng thời gian còn lại của token.
    async fn revoke_token(&self, jti: Uuid, expires_at: i64, reason: &str)
    -> Result<(), AuthError>;

    /// Kiểm tra một `jti` đã bị thu hồi hay chưa.
    async fn is_token_revoked(&self, jti: Uuid) -> Result<bool, AuthError>;
}

/// Kho lưu trữ JTI Blacklist in-memory phục vụ kiểm thử và offline development.
#[derive(Debug, Default, Clone)]
pub struct MemoryRevocationStore {
    entries: Arc<RwLock<HashMap<Uuid, (RevocationEntry, Instant)>>>,
}

impl MemoryRevocationStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl RevocationStore for MemoryRevocationStore {
    async fn revoke_token(
        &self,
        jti: Uuid,
        expires_at: i64,
        reason: &str,
    ) -> Result<(), AuthError> {
        let now = Utc::now().timestamp();
        let remaining_secs = u64::try_from((expires_at - now).max(1)).unwrap_or(1);
        let expire_instant = Instant::now() + std::time::Duration::from_secs(remaining_secs);

        let entry = RevocationEntry {
            jti,
            revoked_at: now,
            reason: reason.to_string(),
            expires_at,
        };

        let mut lock = self.entries.write().await;
        lock.insert(jti, (entry, expire_instant));
        Ok(())
    }

    async fn is_token_revoked(&self, jti: Uuid) -> Result<bool, AuthError> {
        let mut lock = self.entries.write().await;
        if let Some((_, expire_instant)) = lock.get(&jti) {
            if Instant::now() >= *expire_instant {
                lock.remove(&jti);
                return Ok(false);
            }
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(feature = "revocation")]
pub mod cache_store {
    use super::{AuthError, RevocationEntry, RevocationStore};
    use async_trait::async_trait;
    use chrono::Utc;
    use omni_cache::CacheBackend;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Kho lưu trữ JTI Blacklist trên cụm Redis (`omni-cache`).
    #[derive(Clone, Debug)]
    pub struct CacheRevocationStore<B: CacheBackend> {
        backend: Arc<B>,
        key_prefix: String,
    }

    impl<B: CacheBackend> CacheRevocationStore<B> {
        pub fn new(backend: Arc<B>, key_prefix: impl Into<String>) -> Self {
            Self {
                backend,
                key_prefix: key_prefix.into(),
            }
        }

        fn key(&self, jti: Uuid) -> String {
            format!("{}:auth:revocation:v1:{{{}}}", self.key_prefix, jti)
        }
    }

    #[async_trait]
    impl<B: CacheBackend> RevocationStore for CacheRevocationStore<B> {
        async fn revoke_token(
            &self,
            jti: Uuid,
            expires_at: i64,
            reason: &str,
        ) -> Result<(), AuthError> {
            let now = Utc::now().timestamp();
            let remaining_secs = expires_at - now;
            let Ok(remaining_secs_u64) = u64::try_from(remaining_secs) else {
                return Ok(());
            };
            if remaining_secs_u64 == 0 {
                return Ok(());
            }

            let entry = RevocationEntry {
                jti,
                revoked_at: now,
                reason: reason.to_string(),
                expires_at,
            };

            let payload = serde_json::to_vec(&entry)
                .map_err(|e| AuthError::RevocationStoreError(e.to_string()))?;

            let ttl_ms = remaining_secs_u64.saturating_mul(1000);
            self.backend
                .set(&self.key(jti), &payload, ttl_ms)
                .await
                .map_err(|e| AuthError::RevocationStoreError(e.to_string()))?;

            Ok(())
        }

        async fn is_token_revoked(&self, jti: Uuid) -> Result<bool, AuthError> {
            match self.backend.get(&self.key(jti)).await {
                Ok(Some(_)) => Ok(true),
                Ok(None) => Ok(false),
                Err(e) => {
                    tracing::warn!(error = %e, "JTI Blacklist check failed; fail-open default");
                    Ok(false)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revocation_entry_serde() {
        let jti = Uuid::now_v7();
        let entry = RevocationEntry {
            jti,
            revoked_at: 1_700_000_000,
            reason: "admin_revocation".to_string(),
            expires_at: 1_700_000_900,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let de: RevocationEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, de);
    }

    #[tokio::test]
    async fn test_memory_revocation_store_lifecycle() {
        let store = MemoryRevocationStore::new();
        let jti1 = Uuid::now_v7();
        let jti2 = Uuid::now_v7();

        // 1. Initial state: not revoked
        assert!(!store.is_token_revoked(jti1).await.unwrap());
        assert!(!store.is_token_revoked(jti2).await.unwrap());

        // 2. Revoke jti1
        let now = Utc::now().timestamp();
        store
            .revoke_token(jti1, now + 3600, "logout")
            .await
            .expect("Revocation should succeed");

        // 3. jti1 is revoked, jti2 is still not revoked
        assert!(store.is_token_revoked(jti1).await.unwrap());
        assert!(!store.is_token_revoked(jti2).await.unwrap());

        // 4. Revoke jti2
        store
            .revoke_token(jti2, now + 3600, "suspicious_activity")
            .await
            .expect("Revocation should succeed");
        assert!(store.is_token_revoked(jti2).await.unwrap());
    }
}
