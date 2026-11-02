//! OpenTelemetry Semantic Conventions and Prometheus Telemetry Instrumentation.
//!
//! Provides the [`DbFutureExt`] extension trait for measuring database operation
//! durations, exporting OpenTelemetry tracing spans, and logging slow queries.

use std::future::Future;
use std::time::{Duration, Instant};

use tracing::Instrument;

use crate::error::DbError;

/// Tên hệ thống cơ sở dữ liệu tuân thủ `OTel` Semantic Conventions (stable).
pub const DB_SYSTEM_NAME: &str = "postgresql";

/// Tên chỉ số độ trễ từng thao tác truy vấn (giây).
pub const METRIC_DB_CLIENT_OPERATION_DURATION_SECONDS: &str =
    "db_client_operation_duration_seconds";

/// Tên chỉ số tổng số lỗi phát sinh trong tương tác cơ sở dữ liệu.
pub const METRIC_DB_CLIENT_ERRORS_TOTAL: &str = "db_client_errors_total";

/// Tên chỉ số tổng số giao dịch đã thực hiện theo kết quả (committed, `rolled_back`, aborted).
pub const METRIC_DB_CLIENT_TRANSACTIONS_TOTAL: &str = "db_client_transactions_total";

/// Tên chỉ số độ trễ toàn bộ chu trình sống của một giao dịch (giây).
pub const METRIC_DB_CLIENT_TRANSACTION_DURATION_SECONDS: &str =
    "db_client_transaction_duration_seconds";

/// Tên chỉ số tổng số lần thử lại giao dịch thành công hoặc thất bại.
pub const METRIC_DB_CLIENT_TRANSACTION_RETRIES_TOTAL: &str = "db_client_transaction_retries_total";

/// Tên chỉ số mức độ sử dụng kết nối (active/idle).
pub const METRIC_DB_CLIENT_CONNECTIONS_USAGE: &str = "db_client_connections_usage";

/// Tên chỉ số giới hạn kết nối tối đa cấu hình.
pub const METRIC_DB_CLIENT_CONNECTIONS_MAX: &str = "db_client_connections_max";

/// Tên chỉ số duy trì kết nối tối thiểu cấu hình.
pub const METRIC_DB_CLIENT_CONNECTIONS_MIN: &str = "db_client_connections_min";

/// Ngưỡng thời gian cảnh báo truy vấn chậm mặc định (200 mili-giây).
pub const DEFAULT_SLOW_QUERY_THRESHOLD_MS: u64 = 200;

/// Mô tả loại thao tác và bảng dữ liệu mục tiêu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbOperation {
    /// Loại thao tác SQL chuẩn hóa (SELECT, INSERT, UPDATE, DELETE, RAW).
    pub op: &'static str,
    /// Tên bảng dữ liệu (collection).
    pub collection: &'static str,
}

impl DbOperation {
    /// Khởi tạo thao tác SELECT trên bảng chỉ định.
    #[must_use]
    pub const fn select(collection: &'static str) -> Self {
        Self {
            op: "SELECT",
            collection,
        }
    }

    /// Khởi tạo thao tác INSERT trên bảng chỉ định.
    #[must_use]
    pub const fn insert(collection: &'static str) -> Self {
        Self {
            op: "INSERT",
            collection,
        }
    }

    /// Khởi tạo thao tác UPDATE trên bảng chỉ định.
    #[must_use]
    pub const fn update(collection: &'static str) -> Self {
        Self {
            op: "UPDATE",
            collection,
        }
    }

    /// Khởi tạo thao tác DELETE trên bảng chỉ định.
    #[must_use]
    pub const fn delete(collection: &'static str) -> Self {
        Self {
            op: "DELETE",
            collection,
        }
    }

    /// Khởi tạo thao tác tùy biến.
    #[must_use]
    pub const fn raw(op: &'static str, collection: &'static str) -> Self {
        Self { op, collection }
    }
}

/// Extension trait cung cấp phương thức `.instrument_db()` cho các Future trả về [`Result<T, sqlx::Error>`].
pub trait DbFutureExt: Sized {
    /// Kiểu dữ liệu thành công trả về từ truy vấn.
    type Output;

    /// Đo lường độ trễ truy vấn, xuất `OTel` span, ghi nhận metrics Prometheus,
    /// phát cảnh báo truy vấn chậm và chuyển đổi [`sqlx::Error`] sang [`DbError`].
    fn instrument_db(
        self,
        op: DbOperation,
    ) -> impl Future<Output = Result<Self::Output, DbError>> + Send;
}

impl<F, T> DbFutureExt for F
where
    F: Future<Output = Result<T, sqlx::Error>> + Send,
    T: Send,
{
    type Output = T;

    async fn instrument_db(self, op: DbOperation) -> Result<T, DbError> {
        let start = Instant::now();
        let span = tracing::info_span!(
            "db.query",
            db_system = DB_SYSTEM_NAME,
            db_operation = op.op,
            db_collection = op.collection,
            otel_kind = "client"
        );

        let result = self.instrument(span).await;
        let duration = start.elapsed();

        metrics::histogram!(
            METRIC_DB_CLIENT_OPERATION_DURATION_SECONDS,
            "db_system" => DB_SYSTEM_NAME,
            "db_operation" => op.op,
            "db_collection" => op.collection
        )
        .record(duration.as_secs_f64());

        if duration >= Duration::from_millis(DEFAULT_SLOW_QUERY_THRESHOLD_MS) {
            tracing::warn!(
                target: "omni_database::slow_query",
                db_operation = op.op,
                db_collection = op.collection,
                duration_ms = duration.as_millis(),
                "slow database query detected"
            );
        }

        result.map_err(DbError::from)
    }
}
