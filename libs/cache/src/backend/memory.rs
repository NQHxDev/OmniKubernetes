//! In-Memory Storage Backend for Offline Standalone Testing (`backend/memory.rs`).
//!
//! Emulates Redis commands and atomic Lua script semantics in memory,
//! allowing unit tests to run fast without requiring a live Redis server.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use parking_lot::Mutex;

use crate::{backend::CacheBackend, error::CacheError};

#[derive(Debug, Clone)]
struct Entry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

/// Tầng lưu trữ in-memory giả lập chính xác ngữ nghĩa của Redis và bộ 4 Lua Scripts.
#[derive(Debug, Default)]
pub struct MemoryBackend {
    store: Mutex<HashMap<String, Entry>>,
    fences: Mutex<HashMap<String, u64>>,
    unhealthy: AtomicBool,
}

impl MemoryBackend {
    /// Khởi tạo [`MemoryBackend`] mới.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            fences: Mutex::new(HashMap::new()),
            unhealthy: AtomicBool::new(false),
        }
    }

    /// Giả lập lỗi backend (để kiểm thử chế độ Fail-Open hoặc Sentinel Bypass).
    pub fn set_unhealthy(&self, unhealthy: bool) {
        self.unhealthy.store(unhealthy, Ordering::SeqCst);
    }

    fn check_health(&self) -> Result<(), CacheError> {
        if self.unhealthy.load(Ordering::SeqCst) {
            Err(CacheError::Unavailable(
                "MemoryBackend simulated network partition".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl CacheBackend for MemoryBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        if let Some(entry) = map.get(key) {
            if let Some(exp) = entry.expires_at
                && Instant::now() >= exp
            {
                map.remove(key);
                return Ok(None);
            }
            return Ok(Some(entry.value.clone()));
        }
        Ok(None)
    }

    async fn get_many(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, CacheError> {
        self.check_health()?;
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            results.push(self.get(key).await?);
        }
        Ok(results)
    }

    async fn set(&self, key: &str, value: &[u8], ttl_ms: u64) -> Result<(), CacheError> {
        self.check_health()?;
        let expires_at = if ttl_ms > 0 {
            Some(Instant::now() + Duration::from_millis(ttl_ms))
        } else {
            None
        };
        let mut map = self.store.lock();
        map.insert(
            key.to_string(),
            Entry {
                value: value.to_vec(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn set_nx(&self, key: &str, value: &[u8], ttl_ms: u64) -> Result<bool, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        let is_expired = if let Some(entry) = map.get(key) {
            entry.expires_at.is_some_and(|exp| Instant::now() >= exp)
        } else {
            true
        };

        if is_expired {
            let expires_at = if ttl_ms > 0 {
                Some(Instant::now() + Duration::from_millis(ttl_ms))
            } else {
                None
            };
            map.insert(
                key.to_string(),
                Entry {
                    value: value.to_vec(),
                    expires_at,
                },
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn del(&self, key: &str) -> Result<bool, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        Ok(map.remove(key).is_some())
    }

    async fn del_many(&self, keys: &[&str]) -> Result<usize, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        let mut count = 0;
        for key in keys {
            if map.remove(*key).is_some() {
                count += 1;
            }
        }
        Ok(count)
    }

    async fn ping(&self) -> Result<(), CacheError> {
        self.check_health()
    }

    async fn acquire_with_fence(
        &self,
        lock_key: &str,
        fence_key: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<(bool, u64), CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        let is_available = if let Some(entry) = map.get(lock_key) {
            entry.expires_at.is_some_and(|exp| Instant::now() >= exp)
        } else {
            true
        };

        if is_available {
            let expires_at = Some(Instant::now() + Duration::from_millis(ttl_ms));
            map.insert(
                lock_key.to_string(),
                Entry {
                    value: owner_token.as_bytes().to_vec(),
                    expires_at,
                },
            );

            let mut fences = self.fences.lock();
            let counter = fences.entry(fence_key.to_string()).or_insert(0);
            *counter += 1;
            Ok((true, *counter))
        } else {
            Ok((false, 0))
        }
    }

    async fn release_lock(&self, lock_key: &str, owner_token: &str) -> Result<bool, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        if let Some(entry) = map.get(lock_key)
            && entry.expires_at.is_none_or(|exp| Instant::now() < exp)
            && entry.value == owner_token.as_bytes()
        {
            map.remove(lock_key);
            return Ok(true);
        }
        Ok(false)
    }

    async fn extend_lock(
        &self,
        lock_key: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<bool, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        if let Some(entry) = map.get_mut(lock_key)
            && entry.expires_at.is_none_or(|exp| Instant::now() < exp)
            && entry.value == owner_token.as_bytes()
        {
            entry.expires_at = Some(Instant::now() + Duration::from_millis(ttl_ms));
            return Ok(true);
        }
        Ok(false)
    }

    async fn write_with_lease(
        &self,
        data_key: &str,
        lease_key: &str,
        token: &str,
        payload: &[u8],
        ttl_ms: u64,
    ) -> Result<bool, CacheError> {
        self.check_health()?;
        let mut map = self.store.lock();
        let valid_lease = if let Some(lease_entry) = map.get(lease_key) {
            lease_entry
                .expires_at
                .is_none_or(|exp| Instant::now() < exp)
                && lease_entry.value == token.as_bytes()
        } else {
            false
        };

        if valid_lease {
            let expires_at = if ttl_ms > 0 {
                Some(Instant::now() + Duration::from_millis(ttl_ms))
            } else {
                None
            };
            map.insert(
                data_key.to_string(),
                Entry {
                    value: payload.to_vec(),
                    expires_at,
                },
            );
            map.remove(lease_key);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
