//! Static-Typed Keyspace Grammar, Hash Tags & PII-Safe Key Abstractions (`key.rs`).
//!
//! Enforces canonical Redis key patterns, cluster hash tags, and HMAC-SHA256 masking
//! for sensitive personal identifiers (PII).

use std::{fmt, sync::Arc};

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

use crate::error::CacheError;

type HmacSha256 = Hmac<Sha256>;

/// Kích thước trần tối đa cho một Redis Key chuẩn (512 bytes).
pub const MAX_KEY_LENGTH: usize = 512;

/// Khóa cache đã qua xác thực định dạng và an toàn khi clone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    raw: Arc<str>,
    resource: &'static str,
}

impl CacheKey {
    /// Trả về chuỗi tham chiếu thô của key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Tên resource tĩnh gắn với key này (dùng làm low-cardinality metric label).
    #[must_use]
    pub fn resource(&self) -> &'static str {
        self.resource
    }

    /// Tạo khóa Distributed Lease tương ứng cho key này (`<cache-key>:lease`).
    ///
    /// Nhờ hash tag `{id}` trong cache key, lease key tự động rơi vào cùng một Redis Cluster Hash Slot!
    #[must_use]
    pub fn lease_key(&self) -> Self {
        Self {
            raw: Arc::from(format!("{}:lease", self.raw)),
            resource: self.resource,
        }
    }
}

impl std::ops::Deref for CacheKey {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl redis::ToRedisArgs for CacheKey {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        out.write_arg(self.raw.as_bytes());
    }
}

/// Bộ băm HMAC-SHA256 phục vụ che giấu định danh nhạy cảm trong key.
#[derive(Clone)]
pub struct KeyHasher {
    secret: Arc<SecretString>,
}

impl fmt::Debug for KeyHasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyHasher([REDACTED])")
    }
}

impl KeyHasher {
    /// Khởi tạo [`KeyHasher`] mới từ bí mật HMAC (yêu cầu tối thiểu 32 bytes).
    pub fn new(secret: SecretString) -> Result<Self, CacheError> {
        if secret.expose_secret().len() < 32 {
            return Err(CacheError::InvalidKey(
                "key_hash_secret must be at least 32 bytes for HMAC-SHA256".to_string(),
            ));
        }
        Ok(Self {
            secret: Arc::new(secret),
        })
    }

    /// Băm giá trị định danh nhạy cảm (email, phone) thành `"h:<32-hex-chars>"`.
    pub fn hash_sensitive(&self, value: &str) -> Result<String, CacheError> {
        let mut mac = HmacSha256::new_from_slice(self.secret.expose_secret().as_bytes())
            .map_err(|e| CacheError::Codec(format!("HMAC init failed: {e}")))?;
        mac.update(value.as_bytes());
        let result = mac.finalize().into_bytes();
        let hex_str = hex::encode(result);
        Ok(format!("h:{}", &hex_str[..32]))
    }
}

/// Định nghĩa Keyspace chuẩn hóa cho một resource cụ thể.
///
/// Định dạng: `<prefix>:<service>:<resource>:v<schema>[:<segment>]:{<id>}`
#[derive(Debug, Clone)]
pub struct KeySpace {
    prefix: Arc<str>,
    service: Arc<str>,
    resource: &'static str,
    schema_version: u8,
    hasher: Option<KeyHasher>,
}

impl KeySpace {
    /// Khởi tạo [`KeySpace`] mới.
    ///
    /// - `resource` bắt buộc là `&'static str` để ngăn chặn việc đưa dữ liệu động vào tên resource.
    /// - `schema_version`: phiên bản schema DTO (từ 1 đến 255).
    pub fn new(
        prefix: impl Into<String>,
        service: impl Into<String>,
        resource: &'static str,
        schema_version: u8,
        hasher: Option<KeyHasher>,
    ) -> Result<Self, CacheError> {
        let p: String = prefix.into();
        let s: String = service.into();

        validate_segment(&p)?;
        validate_segment(&s)?;
        validate_segment(resource)?;

        if schema_version == 0 {
            return Err(CacheError::InvalidKey(
                "schema_version must be >= 1".to_string(),
            ));
        }

        Ok(Self {
            prefix: Arc::from(p),
            service: Arc::from(s),
            resource,
            schema_version,
            hasher,
        })
    }

    /// Tên resource tĩnh gắn với keyspace này.
    #[must_use]
    pub fn resource(&self) -> &'static str {
        self.resource
    }

    /// Phiên bản cấu trúc dữ liệu của keyspace này.
    #[must_use]
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }

    /// Sinh [`CacheKey`] chuẩn cho định danh thực thể công khai (`id`).
    ///
    /// Ví dụ: `omni:product:product:v1:{018f4a2b-...}`
    pub fn key(&self, id: impl fmt::Display) -> Result<CacheKey, CacheError> {
        self.key_with(&[], id)
    }

    /// Sinh [`CacheKey`] có kèm các segment phụ (ví dụ tenant, locale) trước `id`.
    ///
    /// Ví dụ: `omni:product:category-tree:v1:tenant-1:vi-VN:{root}`
    pub fn key_with(
        &self,
        segments: &[&str],
        id: impl fmt::Display,
    ) -> Result<CacheKey, CacheError> {
        let id_str = id.to_string();
        validate_id(&id_str)?;

        let mut buf = String::with_capacity(128);
        buf.push_str(&self.prefix);
        buf.push(':');
        buf.push_str(&self.service);
        buf.push(':');
        buf.push_str(self.resource);
        buf.push(':');
        buf.push('v');
        buf.push_str(&self.schema_version.to_string());

        for seg in segments {
            validate_segment(seg)?;
            buf.push(':');
            buf.push_str(seg);
        }

        buf.push(':');
        buf.push('{');
        buf.push_str(&id_str);
        buf.push('}');

        if buf.len() > MAX_KEY_LENGTH {
            return Err(CacheError::InvalidKey(format!(
                "Constructed key length {} exceeds maximum allowed {}",
                buf.len(),
                MAX_KEY_LENGTH
            )));
        }

        Ok(CacheKey {
            raw: Arc::from(buf),
            resource: self.resource,
        })
    }

    /// Sinh [`CacheKey`] cho định danh nhạy cảm (email, phone, CCCD),
    /// tự động băm qua HMAC-SHA256: `omni:user:user-by-email:v1:{h:9f2c...e1}`.
    pub fn key_sensitive(&self, value: &str) -> Result<CacheKey, CacheError> {
        let hasher = self.hasher.as_ref().ok_or_else(|| {
            CacheError::InvalidKey(
                "key_hash_secret is required in RedisConfig to generate sensitive keys".to_string(),
            )
        })?;

        let hashed_id = hasher.hash_sensitive(value)?;
        self.key(hashed_id)
    }
}

/// Tạo cặp khóa chuẩn cho Distributed Lock và Fencing Counter theo cấu trúc tương thích Cluster Hash Tag:
/// - Lock key: `<prefix>:<service>:lock:{<lock-ns>}:r:<resource-id>`
/// - Fence key: `<prefix>:<service>:lock:{<lock-ns>}:fence`
pub fn make_lock_keys(
    prefix: &str,
    service: &str,
    lock_namespace: &str,
    resource_id: &str,
) -> Result<(String, String), CacheError> {
    validate_segment(prefix)?;
    validate_segment(service)?;
    validate_segment(lock_namespace)?;
    validate_id(resource_id)?;

    let lock_key = format!("{prefix}:{service}:lock:{{{lock_namespace}}}:r:{resource_id}");
    let fence_key = format!("{prefix}:{service}:lock:{{{lock_namespace}}}:fence");

    if lock_key.len() > MAX_KEY_LENGTH || fence_key.len() > MAX_KEY_LENGTH {
        return Err(CacheError::InvalidKey(format!(
            "Lock keys exceed maximum length {MAX_KEY_LENGTH}"
        )));
    }

    Ok((lock_key, fence_key))
}

fn validate_segment(s: &str) -> Result<(), CacheError> {
    if s.is_empty() {
        return Err(CacheError::InvalidKey(
            "Key segment must not be empty".to_string(),
        ));
    }
    if s.len() > 64 {
        return Err(CacheError::InvalidKey(format!(
            "Key segment '{s}' exceeds 64 characters"
        )));
    }
    if s.chars()
        .any(|c| c == ':' || c == '{' || c == '}' || c.is_whitespace())
    {
        return Err(CacheError::InvalidKey(format!(
            "Key segment '{s}' contains prohibited characters (colon, braces, or whitespace)"
        )));
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), CacheError> {
    if id.is_empty() {
        return Err(CacheError::InvalidKey(
            "Entity ID must not be empty".to_string(),
        ));
    }
    if id.len() > 128 {
        return Err(CacheError::InvalidKey(format!(
            "Entity ID exceeds 128 characters: '{id}'"
        )));
    }
    if id
        .chars()
        .any(|c| c == '{' || c == '}' || c.is_whitespace())
    {
        return Err(CacheError::InvalidKey(format!(
            "Entity ID '{id}' contains prohibited characters (braces or whitespace)"
        )));
    }
    Ok(())
}
