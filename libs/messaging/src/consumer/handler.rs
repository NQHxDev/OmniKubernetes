//! Declarative Message Handler Trait (`handler.rs`).
//!
//! Provides the developer-facing [`MessageHandler<T>`] trait and execution context [`MessageContext`].

use std::collections::HashMap;

use async_trait::async_trait;

use crate::{cloudevent::CloudEvent, error::MessageProcessingError};

/// Ngữ cảnh cung cấp cho Handler trong quá trình xử lý bản tin.
#[derive(Debug, Clone)]
pub struct MessageContext {
    /// Topic nguồn nhận bản tin.
    pub topic: String,

    /// Khóa phân vùng của Aggregate Root.
    pub partition_key: Option<String>,

    /// Lần thử phân phối hiện tại (1-indexed).
    pub delivery_attempt: u32,

    /// W3C Distributed Trace ID trích xuất từ header.
    pub trace_id: String,

    /// Bảng metadata headers đính kèm bản tin.
    pub headers: HashMap<String, String>,
}

/// Trait nghiệp vụ khai báo cho Consumer.
///
/// Developer triển khai trait này cho struct nghiệp vụ của microservice:
/// - Trả về `Ok(())`: Khung tự động ACK và ghi nhận chỉ số thành công.
/// - Trả về `Err(MessageProcessingError::Transient)`: Khung kích hoạt Exponential Backoff with Full Jitter.
/// - Trả về `Err(MessageProcessingError::Terminal)`: Khung lập tức đóng gói và chuyển bản tin sang DLQ.
#[async_trait]
pub trait MessageHandler<T: Clone + Send + Sync + 'static>: Send + Sync {
    /// Hàm xử lý nghiệp vụ chính.
    async fn handle(
        &self,
        ctx: MessageContext,
        event: CloudEvent<T>,
    ) -> Result<(), MessageProcessingError>;
}
