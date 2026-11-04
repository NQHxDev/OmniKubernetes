//! Unified Message Publisher Abstraction (`publisher.rs`).
//!
//! Provides the broker-agnostic [`MessagePublisher`] trait and batch dispatch results.

use async_trait::async_trait;
use uuid::Uuid;

use crate::{broker::RawBrokerMessage, error::MessagingError};

/// Kết quả xuất bản thông điệp theo lô.
#[derive(Debug, Clone, Default)]
pub struct BatchPublishResult {
    /// Danh sách ID các bản tin đã xuất bản thành công.
    pub successful_ids: Vec<Uuid>,
    /// Danh sách ID và thông điệp lỗi cho các bản tin bị thất bại.
    pub failed_items: Vec<(Uuid, String)>,
}

impl BatchPublishResult {
    /// Khởi tạo [`BatchPublishResult`] mới.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Kiểm tra xem toàn bộ các bản tin trong mẻ có xuất bản thành công hay không.
    #[must_use]
    pub fn is_all_successful(&self) -> bool {
        self.failed_items.is_empty()
    }
}

/// Trait xuất bản thông điệp ra Message Broker.
#[async_trait]
pub trait MessagePublisher: Send + Sync {
    /// Xuất bản một thông điệp đơn lẻ sang Broker.
    async fn publish(&self, message: RawBrokerMessage) -> Result<(), MessagingError>;

    /// Xuất bản một mẻ thông điệp (Batch Publishing) với xác nhận hàng loạt.
    async fn publish_batch(
        &self,
        messages: &[RawBrokerMessage],
    ) -> Result<BatchPublishResult, MessagingError>;
}
