use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use omni_messaging::{
    InMemoryBroker, MessageConsumer, MessagePublisher, RawBrokerMessage, RawMessageHandler,
};

struct CountingHandler {
    count: Arc<AtomicUsize>,
}

#[async_trait]
impl RawMessageHandler for CountingHandler {
    async fn handle_raw(
        &self,
        _message: RawBrokerMessage,
    ) -> Result<(), omni_messaging::MessagingError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn test_in_memory_broker_publish_subscribe_flow() {
    let broker = InMemoryBroker::new();
    let counter = Arc::new(AtomicUsize::new(0));

    let handler = Arc::new(CountingHandler {
        count: counter.clone(),
    });

    broker
        .subscribe("order.events", "test-group", handler)
        .await
        .expect("Subscribe must succeed");

    let msg1 = RawBrokerMessage::new("order.events", b"payload1".to_vec());
    let msg2 = RawBrokerMessage::new("order.events", b"payload2".to_vec());

    broker.publish(msg1).await.expect("Publish 1 must succeed");
    broker.publish(msg2).await.expect("Publish 2 must succeed");

    // Wait briefly for asynchronous background tasks
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    assert_eq!(broker.get_published_messages().len(), 2);
    assert_eq!(broker.get_messages_for_topic("order.events").len(), 2);
}

#[tokio::test]
async fn test_in_memory_broker_publish_batch() {
    let broker = InMemoryBroker::new();

    let messages = vec![
        RawBrokerMessage::new("batch.topic", b"m1".to_vec()),
        RawBrokerMessage::new("batch.topic", b"m2".to_vec()),
        RawBrokerMessage::new("batch.topic", b"m3".to_vec()),
    ];

    let result = broker
        .publish_batch(&messages)
        .await
        .expect("Publish batch must succeed");

    assert!(result.is_all_successful());
    assert_eq!(result.successful_ids.len(), 3);
    assert_eq!(broker.get_published_messages().len(), 3);
}

#[tokio::test]
async fn test_in_memory_broker_failure_simulation() {
    let broker = InMemoryBroker::new();
    broker.set_simulate_failure(true);

    let msg = RawBrokerMessage::new("fail.topic", b"payload".to_vec());
    let err = broker
        .publish(msg)
        .await
        .expect_err("Must fail when simulated");

    match err {
        omni_messaging::MessagingError::PublishRejected(msg) => {
            assert!(msg.contains("Simulated in-memory broker rejection"));
        }
        other => panic!("Unexpected error: {other:?}"),
    }
}
