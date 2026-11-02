//! Error taxonomy and mapping for `omni-database`.
//!
//! Maps PostgreSQL SQLSTATE error codes and `sqlx::Error` to typed [`DbError`],
//! which in turn maps cleanly to RFC 9457 [`omni_errors::AppError`] without leaking
//! sensitive database internals (table names, constraints, raw queries) to clients.

use std::fmt;

/// Mã trạng thái lỗi PostgreSQL gồm 5 ký tự chuẩn ASCII (SQLSTATE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SqlState(pub [u8; 5]);

impl SqlState {
    /// Class 23 — Ràng buộc toàn vẹn dữ liệu
    pub const UNIQUE_VIOLATION: Self = Self(*b"23505");
    pub const FOREIGN_KEY_VIOLATION: Self = Self(*b"23503");
    pub const NOT_NULL_VIOLATION: Self = Self(*b"23502");
    pub const CHECK_VIOLATION: Self = Self(*b"23514");
    pub const EXCLUSION_VIOLATION: Self = Self(*b"23P01");

    /// Class 40 — Lỗi hủy giao dịch
    pub const SERIALIZATION_FAILURE: Self = Self(*b"40001");
    pub const DEADLOCK_DETECTED: Self = Self(*b"40P01");

    /// Class 55 — Điều kiện tiên quyết đối tượng
    pub const LOCK_NOT_AVAILABLE: Self = Self(*b"55P03");

    /// Class 57 — Can thiệp của quản trị viên / Hủy câu lệnh
    pub const QUERY_CANCELED: Self = Self(*b"57014");
    pub const ADMIN_SHUTDOWN: Self = Self(*b"57P01");
    pub const CRASH_SHUTDOWN: Self = Self(*b"57P02");
    pub const CANNOT_CONNECT_NOW: Self = Self(*b"57P03");

    /// Class 25 — Trạng thái giao dịch
    pub const READ_ONLY_SQL_TRANSACTION: Self = Self(*b"25006");

    /// Class 42 — Cú pháp hoặc quyền hạn
    pub const INSUFFICIENT_PRIVILEGE: Self = Self(*b"42501");

    /// Class 53 — Tài nguyên không đủ
    pub const TOO_MANY_CONNECTIONS: Self = Self(*b"53300");

    /// Class 28 — Xác thực người dùng
    pub const INVALID_PASSWORD: Self = Self(*b"28P01");
    pub const INVALID_AUTH_SPEC: Self = Self(*b"28000");

    /// Class 3D — Cơ sở dữ liệu không tồn tại
    pub const INVALID_CATALOG_NAME: Self = Self(*b"3D000");

    /// Phân tích chuỗi 5 ký tự ASCII thành [`SqlState`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() == 5 && bytes.iter().all(u8::is_ascii_alphanumeric) {
            let mut arr = [0u8; 5];
            arr.copy_from_slice(bytes);
            Some(Self(arr))
        } else {
            None
        }
    }

    /// Trả về 2 ký tự đầu đại diện cho lớp lỗi (Error Class).
    #[must_use]
    pub const fn class(&self) -> [u8; 2] {
        [self.0[0], self.0[1]]
    }

    /// Trả về chuỗi đại diện SQLSTATE.
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("?????")
    }
}

impl fmt::Display for SqlState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Trait đánh dấu các lỗi có tính chất tạm thời, có thể thử lại an toàn.
pub trait Retryable {
    /// Trả về `true` nếu lỗi là tạm thời và có thể thử lại.
    fn is_transient(&self) -> bool;

    /// Lý do thử lại phục vụ ghi nhãn metric.
    fn retry_reason(&self) -> &'static str {
        "transient"
    }
}

/// Định nghĩa các loại lỗi cơ sở dữ liệu quan hệ được chuẩn hóa (`omni-database`).
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// Không tìm thấy bản ghi.
    #[error("record not found")]
    NotFound,

    /// Vi phạm ràng buộc duy nhất (Unique Constraint).
    #[error("unique constraint violated ({constraint:?})")]
    UniqueViolation {
        /// Tên ràng buộc trong cơ sở dữ liệu (chỉ dùng nội bộ, không lộ ra client).
        constraint: Option<String>,
    },

    /// Vi phạm khóa ngoại (Foreign Key Constraint).
    #[error("foreign key constraint violated ({constraint:?})")]
    ForeignKeyViolation {
        /// Tên ràng buộc trong cơ sở dữ liệu.
        constraint: Option<String>,
    },

    /// Vi phạm ràng buộc toàn vẹn (NOT NULL, CHECK, EXCLUSION, kiểu dữ liệu).
    #[error("integrity constraint violated ({sqlstate}, {constraint:?})")]
    IntegrityViolation {
        /// Tên ràng buộc vi phạm (nếu có).
        constraint: Option<String>,
        /// Mã SQLSTATE tương ứng.
        sqlstate: SqlState,
    },

    /// Xung đột phiên bản dữ liệu khi cập nhật đồng thời (Optimistic Locking).
    #[error("concurrent modification detected on `{entity}`")]
    ConcurrentModification {
        /// Tên thực thể dữ liệu bị xung đột.
        entity: &'static str,
    },

    /// Giao dịch bị máy chủ hủy do xung đột tuần tự hóa hoặc bế tắc (Deadlock).
    #[error("transaction aborted by server ({0})")]
    SerializationFailure(SqlState),

    /// Không thể chiếm khóa trong thời hạn `lock_timeout`.
    #[error("lock not available within lock_timeout")]
    LockNotAvailable,

    /// Truy vấn bị máy chủ hủy do vượt quá `statement_timeout`.
    #[error("statement canceled (statement_timeout or cancel request)")]
    QueryCanceled,

    /// Cạn kiệt kết nối trong pool sau thời gian `acquire_timeout`.
    #[error("connection pool exhausted (acquire_timeout elapsed)")]
    PoolExhausted,

    /// Cơ sở dữ liệu tạm thời không thể phục vụ (mất mạng, đóng pool, máy chủ khởi động lại).
    #[error("database unavailable: {0}")]
    Unavailable(#[source] sqlx::Error),

    /// Thất bại khi thực thi Database Migration.
    #[error("migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Cấu hình cơ sở dữ liệu không hợp lệ.
    #[error("invalid database configuration: {0}")]
    Configuration(String),

    /// Lỗi nội bộ không xác định từ driver hoặc câu lệnh SQL.
    #[error("unexpected database error: {0}")]
    Internal(#[source] sqlx::Error),
}

impl DbError {
    /// Chuỗi định danh ngắn gọn, ít chiều (low-cardinality) cho nhãn metrics và span attributes.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::UniqueViolation { .. } => "unique_violation",
            Self::ForeignKeyViolation { .. } => "foreign_key_violation",
            Self::IntegrityViolation { .. } => "integrity_violation",
            Self::ConcurrentModification { .. } => "concurrent_modification",
            Self::SerializationFailure(_) => "serialization_failure",
            Self::LockNotAvailable => "lock_not_available",
            Self::QueryCanceled => "query_canceled",
            Self::PoolExhausted => "pool_exhausted",
            Self::Unavailable(_) => "unavailable",
            Self::Migration(_) => "migration_failed",
            Self::Configuration(_) => "configuration_error",
            Self::Internal(_) => "internal_error",
        }
    }

    /// Phân loại lỗi từ mã SQLSTATE và thông tin ràng buộc.
    #[must_use]
    pub fn from_sqlstate(
        sqlstate: Option<SqlState>,
        constraint: Option<String>,
        source: sqlx::Error,
    ) -> Self {
        match sqlstate {
            Some(SqlState::UNIQUE_VIOLATION) => Self::UniqueViolation { constraint },
            Some(SqlState::FOREIGN_KEY_VIOLATION) => Self::ForeignKeyViolation { constraint },
            Some(SqlState::SERIALIZATION_FAILURE | SqlState::DEADLOCK_DETECTED) => {
                Self::SerializationFailure(sqlstate.unwrap())
            }
            Some(SqlState::LOCK_NOT_AVAILABLE) => Self::LockNotAvailable,
            Some(SqlState::QUERY_CANCELED) => Self::QueryCanceled,
            Some(SqlState::TOO_MANY_CONNECTIONS | SqlState::READ_ONLY_SQL_TRANSACTION) => {
                Self::Unavailable(source)
            }
            Some(state) => {
                let class = state.class();
                match &class {
                    b"23" | b"22" => Self::IntegrityViolation {
                        constraint,
                        sqlstate: state,
                    },
                    b"08" | b"53" | b"57" => Self::Unavailable(source),
                    _ => Self::Internal(source),
                }
            }
            None => Self::Internal(source),
        }
    }

    /// Ghi log cấu trúc tương ứng với mức độ nghiêm trọng của lỗi.
    pub fn log(&self) {
        match self {
            Self::NotFound => {
                tracing::debug!(error_type = self.kind_str(), "database record not found");
            }
            Self::UniqueViolation { constraint } => {
                tracing::info!(
                    error_type = self.kind_str(),
                    db_constraint = constraint.as_deref().unwrap_or("unknown"),
                    "unique constraint violated"
                );
            }
            Self::ForeignKeyViolation { constraint } => {
                tracing::info!(
                    error_type = self.kind_str(),
                    db_constraint = constraint.as_deref().unwrap_or("unknown"),
                    "foreign key constraint violated"
                );
            }
            Self::ConcurrentModification { entity } => {
                tracing::info!(
                    error_type = self.kind_str(),
                    entity = entity,
                    "optimistic lock concurrent modification detected"
                );
            }
            Self::IntegrityViolation {
                constraint,
                sqlstate,
            } => {
                tracing::warn!(
                    error_type = self.kind_str(),
                    db_response_status_code = sqlstate.as_str(),
                    db_constraint = constraint.as_deref().unwrap_or("unknown"),
                    "data integrity constraint violation"
                );
            }
            Self::SerializationFailure(sqlstate) => {
                tracing::warn!(
                    error_type = self.kind_str(),
                    db_response_status_code = sqlstate.as_str(),
                    "serialization failure or deadlock detected"
                );
            }
            Self::LockNotAvailable => {
                tracing::warn!(
                    error_type = self.kind_str(),
                    "lock not available within lock_timeout"
                );
            }
            Self::QueryCanceled => {
                tracing::warn!(
                    error_type = self.kind_str(),
                    "statement canceled due to statement_timeout"
                );
            }
            Self::PoolExhausted => {
                tracing::warn!(
                    error_type = self.kind_str(),
                    "connection pool acquire timed out"
                );
            }
            Self::Unavailable(source) => {
                tracing::error!(
                    error_type = self.kind_str(),
                    error = %source,
                    "database service unavailable"
                );
            }
            Self::Migration(source) => {
                tracing::error!(
                    error_type = self.kind_str(),
                    error = %source,
                    "database migration execution failed"
                );
            }
            Self::Configuration(msg) => {
                tracing::error!(
                    error_type = self.kind_str(),
                    message = %msg,
                    "invalid database configuration"
                );
            }
            Self::Internal(source) => {
                tracing::error!(
                    error_type = self.kind_str(),
                    error = %source,
                    "internal database unexpected error"
                );
            }
        }
    }
}

impl From<sqlx::Error> for DbError {
    fn from(err: sqlx::Error) -> Self {
        let mapped = if let sqlx::Error::Database(db_err) = &err {
            let sqlstate = db_err.code().and_then(|c| SqlState::parse(&c));
            let constraint = db_err.constraint().map(ToOwned::to_owned);
            Self::from_sqlstate(sqlstate, constraint, err)
        } else {
            match err {
                sqlx::Error::RowNotFound => Self::NotFound,
                sqlx::Error::PoolTimedOut => Self::PoolExhausted,
                e @ (sqlx::Error::PoolClosed
                | sqlx::Error::Io(_)
                | sqlx::Error::Tls(_)
                | sqlx::Error::WorkerCrashed) => Self::Unavailable(e),
                other => Self::Internal(other),
            }
        };
        metrics::counter!(
            crate::instrument::METRIC_DB_CLIENT_ERRORS_TOTAL,
            "error_type" => mapped.kind_str()
        )
        .increment(1);
        mapped
    }
}

impl Retryable for DbError {
    fn is_transient(&self) -> bool {
        matches!(self, Self::SerializationFailure(_))
    }

    fn retry_reason(&self) -> &'static str {
        match self {
            Self::SerializationFailure(st) if *st == SqlState::DEADLOCK_DETECTED => {
                "deadlock_detected"
            }
            Self::SerializationFailure(_) => "serialization_failure",
            Self::LockNotAvailable => "lock_not_available",
            _ => "other",
        }
    }
}

impl From<DbError> for omni_errors::AppError {
    fn from(err: DbError) -> Self {
        err.log();
        match err {
            DbError::NotFound => Self::NotFound,
            DbError::UniqueViolation { .. }
            | DbError::ForeignKeyViolation { .. }
            | DbError::ConcurrentModification { .. }
            | DbError::SerializationFailure(_) => Self::Conflict { detail: None },
            DbError::IntegrityViolation { .. } => Self::BadRequest { detail: None },
            DbError::LockNotAvailable
            | DbError::QueryCanceled
            | DbError::PoolExhausted
            | DbError::Unavailable(_) => Self::ServiceUnavailable,
            other => Self::Internal(other.to_string()),
        }
    }
}
