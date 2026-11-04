//! Refresh Token Rotation (RTR) with Token Family reuse detection.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::AuthError;

/// Bản ghi chi tiết của Refresh Token trong một Token Family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshTokenRecord {
    pub token_id: Uuid,
    pub family_id: Uuid,
    pub user_id: Uuid,
    pub generation: u32,
    pub is_used: bool,
    pub expires_at: i64,
}

/// Trait quản lý vòng đời Refresh Token Rotation và Token Family.
#[async_trait]
pub trait TokenFamilyStore: Send + Sync + 'static {
    /// Đăng ký một refresh token mới vào family.
    async fn record_token(&self, record: &RefreshTokenRecord) -> Result<(), AuthError>;

    /// Luân chuyển refresh token: Đánh dấu token cũ là đã sử dụng (`is_used = true`),
    /// cấp phát bản ghi mới cho `new_token_id` với `generation + 1`.
    ///
    /// NẾU token cũ ĐÃ ĐƯỢC SỬ DỤNG TRƯỚC ĐÓ: Kích hoạt cảnh báo ĐÁNH CẮP TOKEN,
    /// lập tức vô hiệu hóa toàn bộ family và trả về [`AuthError::TokenTheftDetected`].
    async fn rotate_token(
        &self,
        old_token_id: Uuid,
        new_token_id: Uuid,
        new_expires_at: i64,
    ) -> Result<RefreshTokenRecord, AuthError>;

    /// Vô hiệu hóa toàn bộ tokens thuộc family.
    async fn invalidate_family(&self, family_id: Uuid) -> Result<(), AuthError>;

    /// Kiểm tra xem family có hợp lệ hay đã bị thu hồi/vô hiệu hóa.
    async fn is_family_valid(&self, family_id: Uuid) -> Result<bool, AuthError>;
}

/// Bộ nhớ in-memory quản lý Token Family phục vụ kiểm thử và offline development.
#[derive(Debug, Default, Clone)]
pub struct MemoryTokenFamilyStore {
    tokens: Arc<RwLock<HashMap<Uuid, RefreshTokenRecord>>>,
    invalidated_families: Arc<RwLock<HashSet<Uuid>>>,
}

impl MemoryTokenFamilyStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            invalidated_families: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

#[async_trait]
impl TokenFamilyStore for MemoryTokenFamilyStore {
    async fn record_token(&self, record: &RefreshTokenRecord) -> Result<(), AuthError> {
        let mut tokens = self.tokens.write().await;
        tokens.insert(record.token_id, record.clone());
        Ok(())
    }

    async fn rotate_token(
        &self,
        old_token_id: Uuid,
        new_token_id: Uuid,
        new_expires_at: i64,
    ) -> Result<RefreshTokenRecord, AuthError> {
        let mut tokens = self.tokens.write().await;

        let Some(old_record) = tokens.get_mut(&old_token_id) else {
            return Err(AuthError::TokenRevoked);
        };

        let family_id = old_record.family_id;

        // Kiểm tra family đã bị thu hồi trước đó chưa
        {
            let invalidated = self.invalidated_families.read().await;
            if invalidated.contains(&family_id) {
                return Err(AuthError::TokenRevoked);
            }
        }

        // PHÁT HIỆN TÁI SỬ DỤNG TOKEN (TOKEN THEFT DETECTION)
        if old_record.is_used {
            tracing::warn!(
                family_id = %family_id,
                token_id = %old_token_id,
                "TOKEN THEFT DETECTED: Refresh token was already used! Invalidating entire family."
            );
            let mut invalidated = self.invalidated_families.write().await;
            invalidated.insert(family_id);
            return Err(AuthError::TokenTheftDetected);
        }

        // Đánh dấu token cũ đã dùng
        old_record.is_used = true;
        let user_id = old_record.user_id;
        let next_generation = old_record.generation + 1;

        let new_record = RefreshTokenRecord {
            token_id: new_token_id,
            family_id,
            user_id,
            generation: next_generation,
            is_used: false,
            expires_at: new_expires_at,
        };

        tokens.insert(new_token_id, new_record.clone());
        Ok(new_record)
    }

    async fn invalidate_family(&self, family_id: Uuid) -> Result<(), AuthError> {
        let mut invalidated = self.invalidated_families.write().await;
        invalidated.insert(family_id);
        Ok(())
    }

    async fn is_family_valid(&self, family_id: Uuid) -> Result<bool, AuthError> {
        let invalidated = self.invalidated_families.read().await;
        Ok(!invalidated.contains(&family_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_token_record_serde() {
        let record = RefreshTokenRecord {
            token_id: Uuid::now_v7(),
            family_id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            generation: 1,
            is_used: false,
            expires_at: 1_700_000_000,
        };

        let json = serde_json::to_string(&record).unwrap();
        let de: RefreshTokenRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, de);
    }

    #[tokio::test]
    async fn test_memory_token_family_rotation_flow() {
        let store = MemoryTokenFamilyStore::new();
        let family_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let token_1 = Uuid::now_v7();
        let token_2 = Uuid::now_v7();
        let token_3 = Uuid::now_v7();

        // 1. Record initial token gen 0
        let record_1 = RefreshTokenRecord {
            token_id: token_1,
            family_id,
            user_id,
            generation: 0,
            is_used: false,
            expires_at: 1_700_001_000,
        };
        store.record_token(&record_1).await.unwrap();
        assert!(store.is_family_valid(family_id).await.unwrap());

        // 2. Rotate token_1 -> token_2
        let record_2 = store
            .rotate_token(token_1, token_2, 1_700_002_000)
            .await
            .expect("First rotation should succeed");
        assert_eq!(record_2.token_id, token_2);
        assert_eq!(record_2.generation, 1);
        assert!(!record_2.is_used);

        // 3. Rotate token_2 -> token_3
        let record_3 = store
            .rotate_token(token_2, token_3, 1_700_003_000)
            .await
            .expect("Second rotation should succeed");
        assert_eq!(record_3.token_id, token_3);
        assert_eq!(record_3.generation, 2);
        assert!(store.is_family_valid(family_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_token_theft_detection_and_invalidation() {
        let store = MemoryTokenFamilyStore::new();
        let family_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let token_1 = Uuid::now_v7();
        let token_2 = Uuid::now_v7();
        let attacker_token = Uuid::now_v7();

        let initial = RefreshTokenRecord {
            token_id: token_1,
            family_id,
            user_id,
            generation: 0,
            is_used: false,
            expires_at: 1_700_001_000,
        };
        store.record_token(&initial).await.unwrap();

        // Legitimate user rotates token_1 -> token_2
        store
            .rotate_token(token_1, token_2, 1_700_002_000)
            .await
            .unwrap();

        // Attacker attempts to rotate the already used token_1 -> triggers TokenTheftDetected
        let theft_res = store
            .rotate_token(token_1, attacker_token, 1_700_003_000)
            .await;
        assert!(matches!(theft_res, Err(AuthError::TokenTheftDetected)));

        // Family is now invalidated
        assert!(!store.is_family_valid(family_id).await.unwrap());

        // Even legitimate user with token_2 is now rejected
        let victim_token = Uuid::now_v7();
        let next_res = store
            .rotate_token(token_2, victim_token, 1_700_004_000)
            .await;
        assert!(matches!(next_res, Err(AuthError::TokenRevoked)));
    }

    #[tokio::test]
    async fn test_invalidate_family_manually() {
        let store = MemoryTokenFamilyStore::new();
        let family_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let token_1 = Uuid::now_v7();
        let token_2 = Uuid::now_v7();

        let record = RefreshTokenRecord {
            token_id: token_1,
            family_id,
            user_id,
            generation: 0,
            is_used: false,
            expires_at: 1_700_001_000,
        };
        store.record_token(&record).await.unwrap();
        assert!(store.is_family_valid(family_id).await.unwrap());

        // Manually invalidate family
        store.invalidate_family(family_id).await.unwrap();
        assert!(!store.is_family_valid(family_id).await.unwrap());

        // Any rotation should now fail
        let res = store.rotate_token(token_1, token_2, 1_700_002_000).await;
        assert!(matches!(res, Err(AuthError::TokenRevoked)));
    }

    #[tokio::test]
    async fn test_rotate_nonexistent_token() {
        let store = MemoryTokenFamilyStore::new();
        let res = store
            .rotate_token(Uuid::now_v7(), Uuid::now_v7(), 1_700_002_000)
            .await;
        assert!(matches!(res, Err(AuthError::TokenRevoked)));
    }
}
