//! Standardized Cache & Concurrency Control Error Taxonomy (`CacheError`).
//!
//! Provides granular error classification from Redis errors, codec errors,
//! decompression bomb defenses, and lock acquisition contention, with mapping to
//! RFC 9457 [`AppError`] problem details.

use std::borrow::Cow;

use omni_errors::AppError;

/// Lý do không chiếm được khóa phân tán.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockAcquireReason {
    /// Khóa đang do tiến trình khác sở hữu (tranh chấp bình thường).
    Contended,
    /// Hết thời gian chờ (`acquire_wait_ms` elapsed).
    Timeout,
    /// Sự cố kết nối Redis khi thử chiếm khóa.
    BackendFailure,
}

/// Lý do mất khóa phân tán trong quá trình thực thi nghiệp vụ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockLostReason {
    /// Khóa bị tiến trình khác chiếm đoạt do quá hạn TTL hoặc bị xóa thủ công.
    Preempted,
    /// Watchdog gia hạn nhịp tim thất bại liên tiếp do đứt mạng.
    RenewalFailed(String),
}

/// Định nghĩa các loại lỗi bộ nhớ đệm và khóa phân tán (`omni-cache`).
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Lệnh Redis bị quá thời gian chờ (`command_timeout_ms`).
    #[error("Redis command '{op}' timed out")]
    Timeout { op: &'static str },

    /// Kết nối Redis không khả dụng (mất mạng, server down, DNS failover).
    #[error("Redis connection unavailable: {0}")]
    Unavailable(String),

    /// Phát hiện Decompression Bomb: dung lượng khai báo vượt trần an toàn `max_value_bytes`.
    #[error(
        "Decompression bomb detected: claimed {claimed_bytes} bytes exceeds ceiling {max_allowed_bytes}"
    )]
    DecompressionBomb {
        claimed_bytes: usize,
        max_allowed_bytes: usize,
    },

    /// Thất bại khi mã hóa/giải mã envelope hoặc JSON payload.
    #[error("Cache codec failure: {0}")]
    Codec(String),

    /// Phiên bản cấu trúc dữ liệu trong cache không khớp với `KeySpace::schema_version`.
    #[error("Schema version mismatch: expected {expected}, found {actual}")]
    SchemaVersionMismatch { expected: u8, actual: u8 },

    /// Key không hợp lệ: chứa ký tự cấm, rỗng hoặc vượt quá 512 bytes.
    #[error("Invalid cache key: {0}")]
    InvalidKey(String),

    /// Không thể chiếm khóa phân tán.
    #[error("Distributed lock '{resource}' acquisition failed: {reason:?}")]
    LockNotAcquired {
        resource: String,
        reason: LockAcquireReason,
    },

    /// Khóa phân tán bị mất trong khi worker đang xử lý tác vụ.
    #[error("Distributed lock '{resource}' was lost during processing")]
    LockLost { resource: String },

    /// Thao tác bị bỏ qua do Sentinel đang kích hoạt chế độ Bypass (Redis Unhealthy).
    #[error("Operation bypassed due to unhealthy cache state")]
    Bypassed,

    /// Lỗi nội bộ phát sinh từ driver `redis-rs`.
    #[error("Redis backend internal error during '{op}': {source}")]
    Backend {
        op: &'static str,
        #[source]
        source: redis::RedisError,
    },
}

impl CacheError {
    /// Phân loại lỗi từ `redis::RedisError` driver.
    #[must_use]
    pub fn from_redis(op: &'static str, err: redis::RedisError) -> Self {
        if err.is_timeout() {
            return Self::Timeout { op };
        }

        if err.is_connection_dropped() || err.is_connection_refusal() {
            return Self::Unavailable(err.to_string());
        }

        Self::Backend { op, source: err }
    }

    /// Ghi log chẩn đoán theo mức độ nghiêm trọng phù hợp.
    pub fn log(&self) {
        match self {
            Self::Timeout { op } => {
                tracing::warn!(op = *op, "Redis command timed out (fail-open path)");
            }
            Self::Unavailable(msg) => {
                tracing::warn!(error = %msg, "Redis backend unavailable (fail-open path)");
            }
            Self::DecompressionBomb {
                claimed_bytes,
                max_allowed_bytes,
            } => {
                tracing::error!(
                    claimed_bytes = *claimed_bytes,
                    max_allowed_bytes = *max_allowed_bytes,
                    "Decompression bomb blocked: potentially malicious or corrupted cache payload"
                );
            }
            Self::Codec(msg) => {
                tracing::warn!(error = %msg, "Cache codec decode error");
            }
            Self::SchemaVersionMismatch { expected, actual } => {
                tracing::debug!(
                    expected = *expected,
                    actual = *actual,
                    "Cache schema version mismatch, treating as cache miss"
                );
            }
            Self::InvalidKey(msg) => {
                tracing::error!(error = %msg, "Invalid cache key format");
            }
            Self::LockNotAcquired { resource, reason } => {
                tracing::info!(
                    resource = %resource,
                    reason = ?reason,
                    "Distributed lock acquisition failed"
                );
            }
            Self::LockLost { resource } => {
                tracing::error!(
                    resource = %resource,
                    "CRITICAL: Distributed lock was lost during processing"
                );
            }
            Self::Bypassed => {
                tracing::trace!("Cache operation bypassed via Sentinel health state");
            }
            Self::Backend { op, source } => {
                tracing::warn!(op = *op, error = %source, "Redis backend returned error");
            }
        }
    }
}

impl From<serde_json::Error> for CacheError {
    fn from(err: serde_json::Error) -> Self {
        Self::Codec(format!("JSON serialization error: {err}"))
    }
}

impl From<CacheError> for AppError {
    fn from(err: CacheError) -> Self {
        err.log();
        match err {
            CacheError::LockNotAcquired { resource, reason } => Self::Conflict {
                detail: Some(Cow::Owned(format!(
                    "Resource '{resource}' is currently locked by another worker (reason: {reason:?})"
                ))),
            },
            CacheError::LockLost { .. }
            | CacheError::Timeout { .. }
            | CacheError::Unavailable(_)
            | CacheError::Bypassed
            | CacheError::Backend { .. } => Self::ServiceUnavailable,
            CacheError::InvalidKey(k) => Self::Internal(format!("Invalid cache key: {k}")),
            CacheError::DecompressionBomb { claimed_bytes, .. } => Self::Internal(format!(
                "Decompression bomb detected ({claimed_bytes} bytes)"
            )),
            CacheError::Codec(msg) => Self::Internal(format!("Cache codec failure: {msg}")),
            CacheError::SchemaVersionMismatch { expected, actual } => Self::Internal(format!(
                "Cache schema version mismatch: expected {expected}, actual {actual}"
            )),
        }
    }
}
