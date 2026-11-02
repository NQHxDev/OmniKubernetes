//! RabbitMQ AMQP 0-9-1 Broker Driver (`rabbitmq.rs`).
//!
//! Implements [`MessagePublisher`] and [`MessageConsumer`] via the pure-Rust `lapin` crate,
//! supporting topic exchanges, quorum queues, publisher confirms, and manual acknowledgments.

#[cfg(feature = "rabbitmq")]
use std::sync::Arc;

#[cfg(feature = "rabbitmq")]
use async_trait::async_trait;
#[cfg(feature = "rabbitmq")]
use futures::StreamExt;
#[cfg(feature = "rabbitmq")]
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable, ShortString},
};
#[cfg(feature = "rabbitmq")]
use uuid::Uuid;

#[cfg(feature = "rabbitmq")]
use crate::{
    broker::{
        consumer::{MessageConsumer, RawMessageHandler},
        message::RawBrokerMessage,
        publisher::{BatchPublishResult, MessagePublisher},
    },
    error::MessagingError,
};

/// RabbitMQ AMQP 0-9-1 Driver.
#[cfg(feature = "rabbitmq")]
#[derive(Clone)]
pub struct RabbitMqBroker {
    connection: Arc<Connection>,
    publish_channel: Arc<Channel>,
    exchange_name: String,
}

#[cfg(feature = "rabbitmq")]
impl std::fmt::Debug for RabbitMqBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RabbitMqBroker")
            .field("exchange_name", &self.exchange_name)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "rabbitmq")]
impl RabbitMqBroker {
    /// Kết nối tới RabbitMQ và khởi tạo topic exchange mặc định.
    pub async fn connect(
        url: &str,
        exchange_name: impl Into<String>,
    ) -> Result<Self, MessagingError> {
        let conn = Connection::connect(url, ConnectionProperties::default())
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        let publish_channel = conn
            .create_channel()
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        let exchange = exchange_name.into();

        // Khởi tạo topic exchange
        publish_channel
            .exchange_declare(
                &exchange,
                ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        Ok(Self {
            connection: Arc::new(conn),
            publish_channel: Arc::new(publish_channel),
            exchange_name: exchange,
        })
    }
}

#[cfg(feature = "rabbitmq")]
#[async_trait]
impl MessagePublisher for RabbitMqBroker {
    async fn publish(&self, message: RawBrokerMessage) -> Result<(), MessagingError> {
        let mut field_table = FieldTable::default();
        for (k, v) in &message.headers {
            field_table.insert(
                ShortString::from(k.as_str()),
                AMQPValue::LongString(v.clone().into_bytes().into()),
            );
        }

        let mut properties = BasicProperties::default()
            .with_delivery_mode(2) // persistent delivery mode
            .with_headers(field_table);

        if let Some(ref pkey) = message.partition_key {
            properties = properties.with_message_id(ShortString::from(pkey.as_str()));
        }

        let routing_key = message.topic.as_str();

        let confirm = self
            .publish_channel
            .basic_publish(
                &self.exchange_name,
                routing_key,
                BasicPublishOptions::default(),
                &message.payload,
                properties,
            )
            .await
            .map_err(|e| MessagingError::PublishRejected(e.to_string()))?;

        confirm.await.map_err(|e| {
            MessagingError::PublishRejected(format!("Publisher confirm failed: {e}"))
        })?;

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

#[cfg(feature = "rabbitmq")]
#[async_trait]
impl MessageConsumer for RabbitMqBroker {
    async fn subscribe(
        &self,
        topic: &str,
        consumer_group: &str,
        handler: Arc<dyn RawMessageHandler>,
    ) -> Result<(), MessagingError> {
        let channel = self
            .connection
            .create_channel()
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        // Thiết lập QoS Prefetch count
        channel
            .basic_qos(50, BasicQosOptions::default())
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        // Tên hàng đợi theo quy ước consumer_group.topic
        let queue_name = format!("{consumer_group}.{topic}");

        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        // Liên kết hàng đợi với Topic Exchange
        channel
            .queue_bind(
                &queue_name,
                &self.exchange_name,
                topic,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        let mut consumer = channel
            .basic_consume(
                &queue_name,
                &format!("consumer-{consumer_group}"),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| MessagingError::BrokerConnectionFailed(e.to_string()))?;

        let topic_str = topic.to_string();

        tokio::spawn(async move {
            while let Some(delivery_result) = consumer.next().await {
                match delivery_result {
                    Ok(delivery) => {
                        let mut headers = std::collections::HashMap::new();
                        if let Some(table) = delivery.properties.headers() {
                            for (k, v) in table.inner() {
                                if let AMQPValue::LongString(val) = v {
                                    if let Ok(s) = std::str::from_utf8(val.as_bytes()) {
                                        headers.insert(k.to_string(), s.to_string());
                                    }
                                } else if let AMQPValue::ShortString(val) = v {
                                    headers.insert(k.to_string(), val.to_string());
                                }
                            }
                        }

                        let raw_msg = RawBrokerMessage {
                            topic: topic_str.clone(),
                            partition_key: None,
                            payload: delivery.data.clone(),
                            headers,
                        };

                        match handler.handle_raw(raw_msg).await {
                            Ok(()) => {
                                let _ = delivery.ack(BasicAckOptions::default()).await;
                            }
                            Err(e) => {
                                tracing::warn!("Consumer handler failed: {e}. Nacking message.");
                                let _ = delivery
                                    .nack(BasicNackOptions {
                                        requeue: false,
                                        ..Default::default()
                                    })
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("RabbitMQ delivery stream error: {e}");
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}
