//! Enterprise Transaction Management & Unit of Work pattern.
//!
//! Provides RAII transaction guards, zero-round-trip session context,
//! automatic commit/rollback semantics, and retry loops for serialization failures.

use std::hash::{BuildHasher, RandomState};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;

use crate::error::{DbError, Retryable};
use crate::instrument::{
    METRIC_DB_CLIENT_TRANSACTION_DURATION_SECONDS, METRIC_DB_CLIENT_TRANSACTION_RETRIES_TOTAL,
    METRIC_DB_CLIENT_TRANSACTIONS_TOTAL,
};
use crate::pool::DbPool;
use crate::session::SessionContext;

/// Mức độ cô lập giao dịch (Transaction Isolation Level) của PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IsolationLevel {
    /// Đọc các bản ghi đã commit (mặc định của PostgreSQL).
    #[default]
    ReadCommitted,
    /// Đọc dữ liệu nhất quán theo snapshot từ thời điểm bắt đầu giao dịch.
    RepeatableRead,
    /// Cách ly hoàn toàn, mô phỏng như thể các giao dịch chạy tuần tự.
    Serializable,
}

impl IsolationLevel {
    /// Trả về chuỗi SQL đại diện cho mức cô lập.
    #[must_use]
    pub const fn as_sql(&self) -> &'static str {
        match self {
            Self::ReadCommitted => "READ COMMITTED",
            Self::RepeatableRead => "REPEATABLE READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }
}

/// Chính sách thử lại giao dịch tự động khi gặp lỗi tạm thời (Serialization Failure / Deadlock).
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Số lần thực thi tối đa (bao gồm lần chạy ban đầu).
    pub max_attempts: u32,
    /// Thời gian chờ ban đầu trước lần thử lại đầu tiên.
    pub initial_backoff: Duration,
    /// Trần thời gian chờ tối đa giữa các lần thử lại.
    pub max_backoff: Duration,
    /// Cho phép thử lại khi gặp lỗi `lock_not_available` (`55P03`).
    pub retry_on_lock_timeout: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::none()
    }
}

impl RetryPolicy {
    /// Chính sách không thử lại (chạy đúng 1 lần).
    #[must_use]
    pub const fn none() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_millis(1000),
            retry_on_lock_timeout: false,
        }
    }

    /// Chính sách tiêu chuẩn cho các giao dịch quan trọng (Serializable).
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_millis(1000),
            retry_on_lock_timeout: false,
        }
    }

    /// Tính toán thời gian chờ backoff exponential kèm jitter ngẫu nhiên.
    #[must_use]
    pub fn backoff(&self, attempt: u32) -> Duration {
        let factor = 2u64.saturating_pow(attempt.saturating_sub(1));
        let millis = u64::try_from(self.initial_backoff.as_millis())
            .unwrap_or(u64::MAX)
            .saturating_mul(factor);
        let bounded = Duration::from_millis(millis).min(self.max_backoff);

        // Jitter 0-20%
        let r = RandomState::new().hash_one(attempt);
        let jitter_pct = f64::from(u32::try_from(r % 20).unwrap_or(0)) / 100.0;
        bounded.mul_f64(1.0 + jitter_pct)
    }
}

/// Tùy chọn cấu hình chi tiết cho giao dịch.
#[derive(Debug, Clone, Default)]
pub struct TxOptions {
    /// Mức độ cô lập giao dịch.
    pub isolation: IsolationLevel,
    /// Chế độ chỉ đọc (Read-Only) giúp PostgreSQL tối ưu tài nguyên.
    pub read_only: bool,
    /// Chế độ Deferrable (chỉ áp dụng cho Serializable Read-Only).
    pub deferrable: bool,
    /// Thời gian tối đa cho toàn bộ câu lệnh trong giao dịch (ghi đè cấu hình pool).
    pub statement_timeout: Option<Duration>,
    /// Thời gian tối đa chờ lock trong giao dịch.
    pub lock_timeout: Option<Duration>,
    /// Chính sách thử lại.
    pub retry: RetryPolicy,
}

impl TxOptions {
    /// Cấu hình mặc định cho các thao tác ghi dữ liệu nghiệp vụ.
    #[must_use]
    pub fn read_write() -> Self {
        Self::default()
    }

    /// Cấu hình đọc snapshot nhất quán cho báo cáo, xuất dữ liệu.
    #[must_use]
    pub fn read_only_snapshot() -> Self {
        Self {
            isolation: IsolationLevel::RepeatableRead,
            read_only: true,
            ..Self::default()
        }
    }

    /// Cấu hình tuần tự hóa nghiêm ngặt kèm chính sách thử lại tiêu chuẩn.
    #[must_use]
    pub fn serializable() -> Self {
        Self {
            isolation: IsolationLevel::Serializable,
            retry: RetryPolicy::standard(),
            ..Self::default()
        }
    }
}

/// Kết quả kết thúc của một giao dịch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxOutcome {
    /// Giao dịch commit thành công.
    Committed,
    /// Giao dịch chủ động rollback do lỗi logic hoặc yêu cầu.
    RolledBack,
    /// Lệnh commit thất bại (ví dụ vi phạm deferred constraint).
    CommitFailed,
    /// Bị hủy giữa chừng do drop/panic/timeout mà không gọi commit/rollback.
    Aborted,
}

impl TxOutcome {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::RolledBack => "rolled_back",
            Self::CommitFailed => "commit_failed",
            Self::Aborted => "aborted",
        }
    }
}

/// RAII Drop Guard ghi nhận thời gian thực thi và chỉ số Prometheus cho giao dịch.
#[derive(Debug)]
pub(crate) struct TxMetricsGuard {
    start: Instant,
    outcome: Option<TxOutcome>,
}

impl TxMetricsGuard {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            outcome: None,
        }
    }

    pub(crate) fn set_outcome(&mut self, outcome: TxOutcome) {
        self.outcome = Some(outcome);
    }
}

impl Drop for TxMetricsGuard {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        let outcome = self.outcome.unwrap_or(TxOutcome::Aborted);

        metrics::histogram!(
            METRIC_DB_CLIENT_TRANSACTION_DURATION_SECONDS,
            "outcome" => outcome.as_str()
        )
        .record(duration);

        metrics::counter!(
            METRIC_DB_CLIENT_TRANSACTIONS_TOTAL,
            "outcome" => outcome.as_str()
        )
        .increment(1);
    }
}

/// Đơn vị công việc (Unit of Work) đại diện cho một transaction PostgreSQL đang hoạt động.
pub struct UnitOfWork {
    tx: sqlx::Transaction<'static, sqlx::Postgres>,
    guard: TxMetricsGuard,
}

impl std::fmt::Debug for UnitOfWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnitOfWork").finish_non_exhaustive()
    }
}

impl UnitOfWork {
    pub(crate) fn new(tx: sqlx::Transaction<'static, sqlx::Postgres>) -> Self {
        Self {
            tx,
            guard: TxMetricsGuard::new(),
        }
    }

    /// Mượn kết nối cơ sở dữ liệu để thực thi các truy vấn SQL.
    pub fn conn(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    /// Hoàn tất và ghi nhận các thay đổi vào cơ sở dữ liệu (`COMMIT`).
    pub async fn commit(mut self) -> Result<(), DbError> {
        match self.tx.commit().await {
            Ok(()) => {
                self.guard.set_outcome(TxOutcome::Committed);
                Ok(())
            }
            Err(e) => {
                self.guard.set_outcome(TxOutcome::CommitFailed);
                Err(DbError::from(e))
            }
        }
    }

    /// Hủy bỏ toàn bộ các thay đổi trong giao dịch (`ROLLBACK`).
    pub async fn rollback(mut self) -> Result<(), DbError> {
        self.guard.set_outcome(TxOutcome::RolledBack);
        self.tx.rollback().await.map_err(DbError::from)
    }
}

/// Thực thi một closure nghiệp vụ trong phạm vi Transaction tự động.
///
/// Tự động gọi `commit()` khi closure trả về `Ok(T)` và tự động `rollback()` khi trả về `Err(E)`.
pub async fn with_transaction<T, E, F>(
    db: &DbPool,
    session: &SessionContext,
    opts: &TxOptions,
    f: F,
) -> Result<T, E>
where
    F: for<'c> FnOnce(&'c mut sqlx::PgConnection) -> BoxFuture<'c, Result<T, E>> + Send,
    E: From<DbError> + Send,
    T: Send,
{
    let mut uow = db.begin(session, opts).await?;
    match f(uow.conn()).await {
        Ok(value) => {
            uow.commit().await?;
            Ok(value)
        }
        Err(err) => {
            if let Err(rb) = uow.rollback().await {
                tracing::warn!(error.type = rb.kind_str(), "rollback failed after business error");
            }
            Err(err)
        }
    }
}

/// Thực thi một closure nghiệp vụ trong phạm vi Transaction có hỗ trợ thử lại tự động.
///
/// Thích hợp cho các giao dịch mức Serializable hoặc có thể phát sinh bế tắc (Deadlock).
pub async fn with_retryable_transaction<T, E, F>(
    db: &DbPool,
    session: &SessionContext,
    opts: &TxOptions,
    mut f: F,
) -> Result<T, E>
where
    F: for<'c> FnMut(&'c mut sqlx::PgConnection) -> BoxFuture<'c, Result<T, E>> + Send,
    E: From<DbError> + Retryable + Send,
    T: Send,
{
    let mut attempt: u32 = 1;
    loop {
        let mut uow = db.begin(session, opts).await?;
        let err = match f(uow.conn()).await {
            Ok(value) => match uow.commit().await {
                Ok(()) => return Ok(value),
                Err(commit_err) => E::from(commit_err),
            },
            Err(e) => {
                let _ = uow.rollback().await;
                e
            }
        };

        if attempt >= opts.retry.max_attempts || !err.is_transient() {
            return Err(err);
        }

        metrics::counter!(
            METRIC_DB_CLIENT_TRANSACTION_RETRIES_TOTAL,
            "reason" => err.retry_reason()
        )
        .increment(1);

        tokio::time::sleep(opts.retry.backoff(attempt)).await;
        attempt += 1;
    }
}
