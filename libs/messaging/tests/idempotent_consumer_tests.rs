use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use omni_cache::MemoryBackend;
use omni_messaging::{
    CloudEvent, CloudEventBuilder, ConsumerConfig, ConsumerPipeline, DeadLetterHeaders,
    DualLayerDeduplicator, InMemoryBroker, MessageContext, MessageHandler, MessageProcessingError,
    RawBrokerMessage, RawMessageHandler, to_binary_message,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TestEventPayload {
    item_id: Uuid,
    name: String,
}

struct TestHandler {
    call_count: Arc<AtomicUsize>,
    should_fail_transient: Arc<AtomicUsize>,
    should_fail_terminal: bool,
}

#[async_trait]
impl MessageHandler<TestEventPayload> for TestHandler {
    async fn handle(
        &self,
        _ctx: MessageContext,
        _event: CloudEvent<TestEventPayload>,
    ) -> Result<(), MessageProcessingError> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail_terminal {
            return Err(MessageProcessingError::Terminal(
                "Bad logic unrecoverable".into(),
            ));
        }

        let transient_remaining = self.should_fail_transient.load(Ordering::SeqCst);
        if transient_remaining > 0 {
            self.should_fail_transient.fetch_sub(1, Ordering::SeqCst);
            return Err(MessageProcessingError::Transient(
                "Transient db connection error".into(),
            ));
        }

        let _ = count;
        Ok(())
    }
}

#[tokio::test]
async fn test_idempotent_consumer_fast_path_suppresses_duplicates() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(TestHandler {
        call_count: call_count.clone(),
        should_fail_transient: Arc::new(AtomicUsize::new(0)),
        should_fail_terminal: false,
    });

    let config = ConsumerConfig {
        concurrency: 4,
        max_retries: 3,
        ..Default::default()
    };

    let cache_backend = Arc::new(MemoryBackend::new());
    let dedup = Arc::new(DualLayerDeduplicator::new().with_cache_backend(cache_backend));

    let pipeline = ConsumerPipeline::new(handler, "test-group", config).with_deduplicator(dedup);

    let event_id = Uuid::now_v7();
    let event = CloudEventBuilder::new(
        "urn:omni:service:test",
        "com.omni.test.v1",
        TestEventPayload {
            item_id: event_id,
            name: "item-1".to_string(),
        },
    )
    .id(event_id)
    .idempotency_key(event_id.to_string())
    .build();

    let raw_msg = to_binary_message(&event, "test.topic").unwrap();

    // 1. Gửi lần 1: thành công
    pipeline.handle_raw(raw_msg.clone()).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // 2. Gửi lần 2: bị deduplicator chặn lại ở Fast Path, handler không bị gọi lần 2
    pipeline.handle_raw(raw_msg.clone()).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // 3. Gửi lần 3: vẫn bị chặn
    pipeline.handle_raw(raw_msg).await.unwrap();
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_poison_pill_routing_to_dlq() {
    let broker = Arc::new(InMemoryBroker::new());
    let call_count = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(TestHandler {
        call_count: call_count.clone(),
        should_fail_transient: Arc::new(AtomicUsize::new(0)),
        should_fail_terminal: false,
    });

    let config = ConsumerConfig {
        concurrency: 2,
        max_retries: 3,
        dlq_topic_suffix: ".dlq".to_string(),
        ..Default::default()
    };

    let pipeline =
        ConsumerPipeline::new(handler, "test-group", config).with_publisher(broker.clone());

    // Tạo bản tin độc với payload rác không thể parse thành JSON
    let mut malformed_headers = std::collections::HashMap::new();
    malformed_headers.insert("ce-id".to_string(), Uuid::now_v7().to_string());
    malformed_headers.insert("ce-type".to_string(), "com.omni.test.v1".to_string());
    malformed_headers.insert("ce-source".to_string(), "urn:omni:bad".to_string());

    let poison_msg = RawBrokerMessage {
        topic: "order.events".to_string(),
        partition_key: None,
        payload: b"{ malformed not valid json ---".to_vec(),
        headers: malformed_headers,
    };

    // Khi xử lý bản tin độc, pipeline không sập mà tự điều hướng sang DLQ và trả về Ok(())
    let result = pipeline.handle_raw(poison_msg).await;
    assert!(result.is_ok());

    // Handler nghiệp vụ hoàn toàn không bị gọi
    assert_eq!(call_count.load(Ordering::SeqCst), 0);

    // Kiểm tra Broker nhận được bản tin trên topic DLQ
    let dlq_messages = broker.get_messages_for_topic("order.events.dlq");
    assert_eq!(dlq_messages.len(), 1);
    let dlq_msg = &dlq_messages[0];
    assert!(dlq_msg.header(DeadLetterHeaders::REASON).is_some());
    assert_eq!(
        dlq_msg.header(DeadLetterHeaders::ORIGINAL_TOPIC),
        Some("order.events")
    );
}

#[tokio::test]
async fn test_terminal_error_immediately_routes_to_dlq() {
    let broker = Arc::new(InMemoryBroker::new());
    let call_count = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(TestHandler {
        call_count: call_count.clone(),
        should_fail_transient: Arc::new(AtomicUsize::new(0)),
        should_fail_terminal: true, // Lỗi vĩnh viễn
    });

    let config = ConsumerConfig {
        concurrency: 2,
        max_retries: 5,
        ..Default::default()
    };

    let pipeline =
        ConsumerPipeline::new(handler, "test-group", config).with_publisher(broker.clone());

    let event = CloudEventBuilder::new(
        "urn:omni:service:test",
        "com.omni.test.v1",
        TestEventPayload {
            item_id: Uuid::now_v7(),
            name: "terminal-item".to_string(),
        },
    )
    .build();

    let raw_msg = to_binary_message(&event, "payment.events").unwrap();

    let result = pipeline.handle_raw(raw_msg).await;
    assert!(result.is_ok());

    // Handler chỉ được gọi đúng 1 lần, không hề retry
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    let dlq_messages = broker.get_messages_for_topic("payment.events.dlq");
    assert_eq!(dlq_messages.len(), 1);
    let reason = dlq_messages[0].header(DeadLetterHeaders::REASON).unwrap();
    assert!(reason.contains("Bad logic unrecoverable"));
}

#[tokio::test]
async fn test_transient_retry_and_recovery() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(TestHandler {
        call_count: call_count.clone(),
        should_fail_transient: Arc::new(AtomicUsize::new(2)), // Thất bại 2 lần đầu, thành công ở lần 3
        should_fail_terminal: false,
    });

    let config = ConsumerConfig {
        concurrency: 2,
        max_retries: 5,
        initial_retry_backoff: std::time::Duration::from_millis(10),
        max_retry_backoff: std::time::Duration::from_millis(50),
        backoff_jitter: false,
        ..Default::default()
    };

    let pipeline = ConsumerPipeline::new(handler, "test-group", config);

    let event = CloudEventBuilder::new(
        "urn:omni:service:test",
        "com.omni.test.v1",
        TestEventPayload {
            item_id: Uuid::now_v7(),
            name: "transient-item".to_string(),
        },
    )
    .build();

    let raw_msg = to_binary_message(&event, "retry.events").unwrap();

    let result = pipeline.handle_raw(raw_msg).await;
    assert!(result.is_ok());

    // Đã gọi 3 lần (2 lần lỗi transient + 1 lần thành công)
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}
