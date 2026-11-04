//! Resilient Consumer Pipeline & Worker Pool (`pipeline.rs`).
//!
//! Provides the complete processing pipeline coordinating bounded concurrency,
//! dual-layer deduplication, poison pill isolation, exponential backoff with jitter,
//! and automatic Dead Letter Queue routing.

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;

use crate::{
    broker::{consumer::RawMessageHandler, message::RawBrokerMessage, publisher::MessagePublisher},
    cloudevent::{event::CloudEvent, serde_mode::from_raw_message},
    config::ConsumerConfig,
    consumer::{
        backoff::BackoffPolicy,
        dlq::route_to_dlq,
        handler::{MessageContext, MessageHandler},
    },
    error::{MessageProcessingError, MessagingError},
    inbox::deduplicator::DualLayerDeduplicator,
    instrument::{
        record_consumer_dedup_hit, record_consumer_duration, record_consumer_error,
        record_consumer_received, record_consumer_retry,
    },
    tracing::extractor::create_consumer_span,
};

/// Pipeline xử lý bản tin chịu lỗi kết nối giữa [`RawMessageHandler`] và [`MessageHandler<T>`].
pub struct ConsumerPipeline<T: Clone + Send + Sync + 'static> {
    handler: Arc<dyn MessageHandler<T>>,
    config: ConsumerConfig,
    backoff: BackoffPolicy,
    publisher: Option<Arc<dyn MessagePublisher>>,
    deduplicator: Option<Arc<DualLayerDeduplicator>>,
    consumer_group: String,
    semaphore: Arc<Semaphore>,
}

impl<T: Clone + Send + Sync + 'static> std::fmt::Debug for ConsumerPipeline<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsumerPipeline")
            .field("consumer_group", &self.consumer_group)
            .field("concurrency", &self.config.concurrency)
            .finish_non_exhaustive()
    }
}

impl<T: DeserializeOwned + Clone + Send + Sync + 'static> ConsumerPipeline<T> {
    /// Khởi tạo một [`ConsumerPipeline`] mới.
    pub fn new(
        handler: Arc<dyn MessageHandler<T>>,
        consumer_group: impl Into<String>,
        config: ConsumerConfig,
    ) -> Self {
        let concurrency = config.concurrency.max(1);
        let backoff = BackoffPolicy::from(&config);
        Self {
            handler,
            consumer_group: consumer_group.into(),
            backoff,
            semaphore: Arc::new(Semaphore::new(concurrency)),
            config,
            publisher: None,
            deduplicator: None,
        }
    }

    /// Đính kèm Message Publisher phục vụ việc xuất bản sang Dead Letter Queue (DLQ).
    #[must_use]
    pub fn with_publisher(mut self, publisher: Arc<dyn MessagePublisher>) -> Self {
        self.publisher = Some(publisher);
        self
    }

    /// Đính kèm DualLayerDeduplicator để khử trùng lặp 2 lớp.
    #[must_use]
    pub fn with_deduplicator(mut self, deduplicator: Arc<DualLayerDeduplicator>) -> Self {
        self.deduplicator = Some(deduplicator);
        self
    }
}

#[async_trait]
impl<T: DeserializeOwned + Clone + Send + Sync + 'static> RawMessageHandler
    for ConsumerPipeline<T>
{
    async fn handle_raw(&self, message: RawBrokerMessage) -> Result<(), MessagingError> {
        // 1. Quản lý đồng thời bằng Semaphore theo concurrency
        let _permit =
            self.semaphore.acquire().await.map_err(|e| {
                MessagingError::Internal(format!("Semaphore acquisition failed: {e}"))
            })?;

        let start_time = Instant::now();
        let topic = &message.topic;
        let consumer_group = &self.consumer_group;

        record_consumer_received(topic, consumer_group);

        let msg_id = message
            .header("ce-id")
            .or_else(|| message.header("id"))
            .unwrap_or("unknown");

        // 2. Kích hoạt Span tracing kế thừa vết từ W3C headers
        let span = create_consumer_span("messaging", topic, msg_id, &message.headers);
        let _span_guard = span.enter();

        let idempotency_key = message
            .header("ce-idempotency_key")
            .or_else(|| message.header("idempotency_key"))
            .or_else(|| message.header("ce-id"))
            .or_else(|| message.header("id"))
            .unwrap_or(msg_id)
            .to_string();

        // 3. Tầng 1: Lọc trùng lặp nhanh qua Cache (Fast Path)
        if let Some(ref dedup) = self.deduplicator {
            match dedup
                .check_and_lock_fast(consumer_group, &idempotency_key, self.config.dedup_ttl)
                .await
            {
                Ok(false) => {
                    // Bản tin đã được xử lý trước đó -> Bỏ qua & Ack
                    tracing::info!(
                        idempotency_key = %idempotency_key,
                        consumer_group = %consumer_group,
                        "Duplicate message detected via fast-path cache. Skipping execution."
                    );
                    record_consumer_dedup_hit(topic, consumer_group);
                    return Ok(());
                }
                Ok(true) => {}
                Err(e) => {
                    tracing::warn!("Fast-path deduplication error: {e}. Continuing.");
                }
            }
        }

        // 4. Giải tuần tự hóa CloudEvent (Tự động nhận diện Binary / Structured)
        let event: CloudEvent<T> = match from_raw_message(&message) {
            Ok(e) => e,
            Err(deser_err) => {
                // Lỗi định dạng payload -> Poison Pill! Cách ly ngay lập tức vào DLQ
                tracing::error!(
                    error = %deser_err,
                    topic = %topic,
                    "Poison pill detected: Failed to parse CloudEvent payload. Routing to DLQ."
                );
                record_consumer_error(topic, consumer_group, "poison_pill");

                if let Some(ref publ) = self.publisher {
                    let dlq_topic = format!("{}{}", topic, self.config.dlq_topic_suffix);
                    route_to_dlq(
                        publ.as_ref(),
                        &message,
                        &dlq_topic,
                        &deser_err.to_string(),
                        1,
                    )
                    .await?;
                    // Ack hàng đợi chính để giải phóng worker
                    return Ok(());
                }

                return Err(deser_err);
            }
        };

        // 5. Tầng 2: Lọc trùng lặp trên cơ sở dữ liệu nếu có
        #[cfg(feature = "postgres")]
        if let Some(ref dedup) = self.deduplicator {
            match dedup
                .check_and_record_db(event.id, &event.event_type, &event.source, consumer_group)
                .await
            {
                Ok(false) => {
                    tracing::info!(
                        event_id = %event.id,
                        consumer_group = %consumer_group,
                        "Duplicate message detected via PostgreSQL inbox. Skipping execution."
                    );
                    record_consumer_dedup_hit(topic, consumer_group);
                    return Ok(());
                }
                Ok(true) => {}
                Err(e) => {
                    tracing::warn!("Database deduplication error: {e}. Continuing.");
                }
            }
        }

        let trace_id = event
            .extensions
            .traceparent
            .as_deref()
            .and_then(omni_common::middleware::traceparent::parse_traceparent)
            .map_or_else(|| "unknown".to_string(), |tc| tc.trace_id);

        let mut ctx = MessageContext {
            topic: topic.clone(),
            partition_key: event.extensions.partition_key.clone(),
            delivery_attempt: 1,
            trace_id,
            headers: message.headers.clone(),
        };

        // 6. Resilient Execution Loop với Exponential Backoff & Full Jitter
        let mut attempt = 1;
        loop {
            match self.handler.handle(ctx.clone(), event.clone()).await {
                Ok(()) => {
                    record_consumer_duration(topic, consumer_group, start_time.elapsed());
                    return Ok(());
                }
                Err(MessageProcessingError::Terminal(reason)) => {
                    // Lỗi vĩnh viễn: Chuyển thẳng sang DLQ, không retry
                    record_consumer_error(topic, consumer_group, "terminal_error");
                    tracing::error!(
                        attempt = attempt,
                        reason = %reason,
                        "Handler returned unrecoverable Terminal error. Routing to DLQ."
                    );

                    if let Some(ref publ) = self.publisher {
                        let dlq_topic = format!("{}{}", topic, self.config.dlq_topic_suffix);
                        route_to_dlq(publ.as_ref(), &message, &dlq_topic, &reason, attempt).await?;
                        return Ok(());
                    }

                    return Err(MessagingError::PoisonPill(reason));
                }
                Err(MessageProcessingError::Transient(reason)) => {
                    record_consumer_error(topic, consumer_group, "transient_error");

                    if attempt >= self.config.max_retries {
                        tracing::error!(
                            attempt = attempt,
                            max_retries = self.config.max_retries,
                            reason = %reason,
                            "Max retries exceeded for transient error. Routing to DLQ."
                        );

                        if let Some(ref publ) = self.publisher {
                            let dlq_topic = format!("{}{}", topic, self.config.dlq_topic_suffix);
                            route_to_dlq(publ.as_ref(), &message, &dlq_topic, &reason, attempt)
                                .await?;
                            return Ok(());
                        }

                        // Giải phóng khóa cache để có thể xử lý lại sau
                        if let Some(ref dedup) = self.deduplicator {
                            let _ = dedup.release_fast(consumer_group, &idempotency_key).await;
                        }

                        return Err(MessagingError::DeadLetterThresholdExceeded {
                            max_retries: self.config.max_retries,
                            reason,
                        });
                    }

                    record_consumer_retry(topic, consumer_group);

                    // Giải phóng tạm thời khóa cache trong khi retry
                    if let Some(ref dedup) = self.deduplicator {
                        let _ = dedup.release_fast(consumer_group, &idempotency_key).await;
                    }

                    let delay = self.backoff.calculate_delay(attempt);
                    tracing::warn!(
                        attempt = attempt,
                        delay_ms = delay.as_millis(),
                        reason = %reason,
                        "Transient error encountered. Backing off before retry."
                    );

                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    ctx.delivery_attempt = attempt;
                }
            }
        }
    }
}
