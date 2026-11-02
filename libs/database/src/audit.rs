//! Enterprise Entity Auditing and Optimistic Concurrency Control.
//!
//! Standardizes audit columns (`created_at/by`, `updated_at/by`, `deleted_at/by`, `version`)
//! across all microservices, automatically populated and stamped by PostgreSQL triggers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DbError;

/// Bộ cột kiểm toán (Auditing Columns) và khóa lạc quan chuẩn hóa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditColumns {
    /// Dấu thời gian tạo bản ghi (UTC).
    pub created_at: DateTime<Utc>,
    /// Định danh chủ thể tạo bản ghi.
    pub created_by: Option<Uuid>,
    /// Dấu thời gian cập nhật gần nhất (UTC).
    pub updated_at: DateTime<Utc>,
    /// Định danh chủ thể thực hiện cập nhật gần nhất.
    pub updated_by: Option<Uuid>,
    /// Dấu thời gian đánh dấu xóa mềm (Soft Delete).
    pub deleted_at: Option<DateTime<Utc>>,
    /// Định danh chủ thể thực hiện xóa mềm.
    pub deleted_by: Option<Uuid>,
    /// Phiên bản dữ liệu cho Optimistic Locking (bắt đầu từ 1, tăng dần sau mỗi UPDATE).
    pub version: i64,
}

impl AuditColumns {
    /// Khởi tạo các giá trị audit ban đầu cho một bản ghi mới.
    #[must_use]
    pub fn new_initial(actor_id: Option<Uuid>) -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            created_by: actor_id,
            updated_at: now,
            updated_by: actor_id,
            deleted_at: None,
            deleted_by: None,
            version: 1,
        }
    }

    /// Kiểm tra xem bản ghi đã bị xóa mềm hay chưa.
    #[must_use]
    pub const fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Cập nhật dấu thời gian và người cập nhật, tăng số phiên bản.
    pub fn touch(&mut self, actor_id: Option<Uuid>) {
        self.updated_at = Utc::now();
        self.updated_by = actor_id;
        self.version += 1;
    }

    /// Đánh dấu bản ghi bị xóa mềm (Soft Delete).
    pub fn mark_deleted(&mut self, actor_id: Option<Uuid>) {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.deleted_by = actor_id;
        self.updated_at = now;
        self.updated_by = actor_id;
        self.version += 1;
    }
}

/// Kiểm tra kết quả thực thi của câu lệnh UPDATE kiểm soát khóa lạc quan (Optimistic Concurrency Check).
///
/// # Errors
/// Trả về [`DbError::ConcurrentModification`] nếu số dòng bị ảnh hưởng (`rows_affected`) bằng 0,
/// chứng tỏ bản ghi đã bị thay đổi bởi giao dịch khác trong cùng thời gian.
pub fn check_optimistic_lock(rows_affected: u64, entity: &'static str) -> Result<(), DbError> {
    if rows_affected == 0 {
        Err(DbError::ConcurrentModification { entity })
    } else {
        Ok(())
    }
}
