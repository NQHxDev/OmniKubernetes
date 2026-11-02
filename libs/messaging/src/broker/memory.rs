//! In-Memory Broker Implementation for Fast Unit & Integration Testing (`memory.rs`).
//!
//! Provides an in-process, zero-dependency broker implementing both [`MessagePublisher`]
//! and [`MessageConsumer`] with published message inspection and subscriber routing.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::{
    broker::{
        consumer::{MessageConsumer, RawMessageHandler},
        message::RawBrokerMessage,
        publisher::{BatchPublishResult, MessagePublisher},
    },
    error::MessagingError,
};

type SubscriberList = Arc<RwLock<HashMap<String, Vec<Arc<dyn RawMessageHandler>>>>>;

/// In-Memory Broker phục vụ kiểm thử đơn vị và tích hợp nội bộ.
#[derive(Clone, Default)]
pub struct InMemoryBroker {
    subscribers: SubscriberList,
    published_messages: Arc<RwLock<Vec<RawBrokerMessage>>>,
    should_fail: Arc<AtomicBool>,
}

impl std::fmt::Debug for InMemoryBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryBroker")
            .field("published_count", &self.published_messages.read().len())
            .field("should_fail", &self.should_fail.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl InMemoryBroker {
    /// Khởi tạo một [`InMemoryBroker`] mới.
    #[must_use]
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            published_messages: Arc::new(RwLock::new(Vec::new())),
            should_fail: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Giả lập lỗi publish (để kiểm thử retry / circuit breaker / poison pill).
    pub fn set_simulate_failure(&self, fail: bool) {
        self.should_fail.store(fail, Ordering::SeqCst);
    }

    /// Trả về bản sao danh sách tất cả các thông điệp đã xuất bản.
    #[must_use]
    pub fn get_published_messages(&self) -> Vec<RawBrokerMessage> {
        self.published_messages.read().clone()
    }

    /// Lọc các thông điệp đã xuất bản theo topic.
    #[must_use]
    pub fn get_messages_for_topic(&self, topic: &str) -> Vec<RawBrokerMessage> {
        self.published_messages
            .read()
            .iter()
            .filter(|m| m.topic == topic)
            .cloned()
            .collect()
    }

    /// Xóa toàn bộ lịch sử thông điệp đã lưu trong bộ nhớ.
    pub fn clear(&self) {
        self.published_messages.write().clear();
    }
}

#[async_trait]
impl MessagePublisher for InMemoryBroker {
    async fn publish(&self, message: RawBrokerMessage) -> Result<(), MessagingError> {
        if self.should_fail.load(Ordering::SeqCst) {
            return Err(MessagingError::PublishRejected(
                "Simulated in-memory broker rejection".into(),
            ));
        }

        let topic = message.topic.clone();
        self.published_messages.write().push(message.clone());

        // Định tuyến bản tin tới các handlers đã subscribe
        let handlers = {
            let subs = self.subscribers.read();
            subs.get(&topic).cloned().unwrap_or_default()
        };

        for handler in handlers {
            let msg = message.clone();
            tokio::spawn(async move {
                if let Err(e) = handler.handle_raw(msg).await {
                    tracing::warn!("In-memory handler returned error: {e}");
                }
            });
        }

        Ok(())
    }

    async fn publish_batch(
        &self,
        messages: &[RawBrokerMessage],
    ) -> Result<BatchPublishResult, MessagingError> {
        let mut result = BatchPublishResult::new();

        for msg in messages {
            let msg_id = msg
                .header("ce-id")
                .and_then(|id| Uuid::parse_str(id).ok())
                .unwrap_or_else(Uuid::now_v7);

            match self.publish(msg.clone()).await {
                Ok(()) => result.successful_ids.push(msg_id),
                Err(e) => result.failed_items.push((msg_id, e.to_string())),
            }
        }

        Ok(result)
    }
}

#[async_trait]
impl MessageConsumer for InMemoryBroker {
    async fn subscribe(
        &self,
        topic: &str,
        _consumer_group: &str,
        handler: Arc<dyn RawMessageHandler>,
    ) -> Result<(), MessagingError> {
        let mut subs = self.subscribers.write();
        subs.entry(topic.to_string()).or_default().push(handler);
        Ok(())
    }
}
