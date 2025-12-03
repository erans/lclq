//! Publisher service implementation.
//!
//! Implements the Google Cloud Pub/Sub Publisher service, including:
//! - CreateTopic
//! - UpdateTopic
//! - Publish
//! - GetTopic
//! - ListTopics
//! - ListTopicSubscriptions
//! - DeleteTopic
//! - DetachSubscription

use crate::error::{Error, Result};
use crate::pubsub::proto::publisher_server::Publisher;
use crate::pubsub::proto::*;
use crate::pubsub::push_queue::{DeliveryQueue, DeliveryTask};
use crate::pubsub::types::{ResourceName, validate_topic_id};
use crate::storage::StorageBackend;
use crate::types::{
    Message, MessageAttributeValue, MessageAttributes, MessageId, QueueConfig, QueueType,
};
use base64::Engine;
use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

/// Publisher service implementation.
pub struct PublisherService {
    backend: Arc<dyn StorageBackend>,
    delivery_queue: Option<DeliveryQueue>,
}

impl PublisherService {
    /// Create a new Publisher service.
    pub fn new(backend: Arc<dyn StorageBackend>, delivery_queue: Option<DeliveryQueue>) -> Self {
        Self {
            backend,
            delivery_queue,
        }
    }

    /// Convert proto Topic to our internal QueueConfig.
    fn topic_to_queue_config(topic: &Topic) -> Result<QueueConfig> {
        let resource_name = ResourceName::parse(&topic.name)?;
        let topic_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        Ok(QueueConfig {
            id: topic_id.clone(),
            name: topic.name.clone(),
            queue_type: QueueType::PubSubTopic,
            visibility_timeout: 60,
            message_retention_period: topic
                .message_retention_duration
                .as_ref()
                .map(|d| d.seconds as u32)
                .unwrap_or(604800), // 7 days default
            max_message_size: 10 * 1024 * 1024, // 10 MB for Pub/Sub
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: topic.labels.clone(),
            redrive_allow_policy: None,
        })
    }

    /// Convert our internal QueueConfig to proto Topic.
    fn queue_config_to_topic(config: &QueueConfig) -> Topic {
        Topic {
            name: config.name.clone(),
            labels: config.tags.clone(),
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
            message_retention_duration: Some(prost_types::Duration {
                seconds: config.message_retention_period as i64,
                nanos: 0,
            }),
        }
    }

    /// Convert proto PubsubMessage to our internal Message.
    fn pubsub_message_to_message(msg: &PubsubMessage, topic_id: &str) -> Message {
        let mut attributes = MessageAttributes::new();
        for (key, value) in &msg.attributes {
            attributes.insert(
                key.clone(),
                MessageAttributeValue {
                    data_type: "String".to_string(),
                    string_value: Some(value.clone()),
                    binary_value: None,
                },
            );
        }

        // Use base64 encoding for binary data
        let body = base64::engine::general_purpose::STANDARD.encode(&msg.data);

        Message {
            id: MessageId::new(),
            body,
            attributes,
            queue_id: topic_id.to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: if msg.ordering_key.is_empty() {
                None
            } else {
                Some(msg.ordering_key.clone())
            },
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        }
    }
}

#[tonic::async_trait]
impl Publisher for PublisherService {
    /// Creates the given topic with the given name.
    async fn create_topic(
        &self,
        request: Request<Topic>,
    ) -> std::result::Result<Response<Topic>, Status> {
        let topic = request.into_inner();
        info!("CreateTopic: {}", topic.name);

        // Validate topic name
        let resource_name = ResourceName::parse(&topic.name)
            .map_err(|e| Status::invalid_argument(format!("Invalid topic name: {}", e)))?;

        // Validate topic ID
        validate_topic_id(resource_name.resource_id())
            .map_err(|e| Status::invalid_argument(format!("Invalid topic ID: {}", e)))?;

        // Convert to queue config
        let queue_config = Self::topic_to_queue_config(&topic)
            .map_err(|e| Status::internal(format!("Failed to convert topic: {}", e)))?;

        // Create in backend
        let created_config =
            self.backend
                .create_queue(queue_config)
                .await
                .map_err(|e| match e {
                    Error::QueueAlreadyExists(_) => {
                        Status::already_exists(format!("Topic already exists: {}", topic.name))
                    }
                    _ => Status::internal(format!("Failed to create topic: {}", e)),
                })?;

        let response_topic = Self::queue_config_to_topic(&created_config);
        Ok(Response::new(response_topic))
    }

    /// Updates an existing topic.
    async fn update_topic(
        &self,
        request: Request<UpdateTopicRequest>,
    ) -> std::result::Result<Response<Topic>, Status> {
        let req = request.into_inner();
        let topic = req
            .topic
            .ok_or_else(|| Status::invalid_argument("Topic is required"))?;

        info!("UpdateTopic: {}", topic.name);

        // Convert to queue config
        let queue_config = Self::topic_to_queue_config(&topic)
            .map_err(|e| Status::internal(format!("Failed to convert topic: {}", e)))?;

        // Update in backend
        let updated_config = self
            .backend
            .update_queue(queue_config)
            .await
            .map_err(|e| Status::internal(format!("Failed to update topic: {}", e)))?;

        let response_topic = Self::queue_config_to_topic(&updated_config);
        Ok(Response::new(response_topic))
    }

    /// Adds one or more messages to the topic.
    async fn publish(
        &self,
        request: Request<PublishRequest>,
    ) -> std::result::Result<Response<PublishResponse>, Status> {
        let req = request.into_inner();
        info!("Publish: {} messages to {}", req.messages.len(), req.topic);

        // Parse topic name
        let resource_name = ResourceName::parse(&req.topic)
            .map_err(|e| Status::invalid_argument(format!("Invalid topic name: {}", e)))?;

        let topic_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Verify topic exists
        self.backend
            .get_queue(&topic_id)
            .await
            .map_err(|_| Status::not_found(format!("Topic not found: {}", req.topic)))?;

        // Convert messages
        let mut messages = Vec::new();
        for msg in &req.messages {
            let message = Self::pubsub_message_to_message(msg, &topic_id);
            debug!(
                "Converting message: id={}, ordering_key={}",
                message.id.0,
                message
                    .message_group_id
                    .as_ref()
                    .unwrap_or(&"<none>".to_string())
            );
            messages.push(message);
        }

        // Get all subscriptions for this topic
        let all_subscriptions = self
            .backend
            .list_subscriptions()
            .await
            .map_err(|e| Status::internal(format!("Failed to list subscriptions: {}", e)))?;

        let topic_subscriptions: Vec<_> = all_subscriptions
            .into_iter()
            .filter(|s| s.topic_id == topic_id)
            .collect();

        debug!(
            "Fanout: topic_id={}, subscriptions={}, messages={}",
            topic_id,
            topic_subscriptions.len(),
            messages.len()
        );

        // Fanout messages to all subscriptions
        let mut message_ids = Vec::new();
        if topic_subscriptions.is_empty() {
            // No subscriptions - just return the message IDs without storing
            message_ids = messages.iter().map(|m| m.id.0.clone()).collect();
        } else {
            // Publish to each subscription
            for sub in &topic_subscriptions {
                debug!("Fanning out to subscription: {}", sub.id);

                // Clone messages for this subscription (each subscription gets its own copy)
                let sub_messages: Vec<_> = messages
                    .iter()
                    .map(|m| {
                        let mut msg = m.clone();
                        msg.queue_id = sub.id.clone();
                        msg
                    })
                    .collect();

                debug!(
                    "Sending {} messages to subscription {}",
                    sub_messages.len(),
                    sub.id
                );

                let published = self
                    .backend
                    .send_messages(&sub.id, sub_messages)
                    .await
                    .map_err(|e| {
                        Status::internal(format!(
                            "Failed to publish messages to subscription {}: {}",
                            sub.id, e
                        ))
                    })?;

                debug!(
                    "Published {} messages to subscription {}, message_ids: {:?}",
                    published.len(),
                    sub.id,
                    published.iter().map(|m| &m.id.0).collect::<Vec<_>>()
                );

                // Use message IDs from first subscription
                if message_ids.is_empty() {
                    message_ids = published.iter().map(|m| m.id.0.clone()).collect();
                }
            }

            // Enqueue push subscriptions once for all messages and subscriptions
            self.enqueue_push_subscriptions(&topic_subscriptions, &messages)
                .await;
        }

        // Build response with message IDs
        let response = PublishResponse { message_ids };
        Ok(Response::new(response))
    }

    /// Gets the configuration of a topic.
    async fn get_topic(
        &self,
        request: Request<GetTopicRequest>,
    ) -> std::result::Result<Response<Topic>, Status> {
        let req = request.into_inner();
        debug!("GetTopic: {}", req.topic);

        // Parse topic name
        let resource_name = ResourceName::parse(&req.topic)
            .map_err(|e| Status::invalid_argument(format!("Invalid topic name: {}", e)))?;

        let topic_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get from backend
        let config = self
            .backend
            .get_queue(&topic_id)
            .await
            .map_err(|_| Status::not_found(format!("Topic not found: {}", req.topic)))?;

        let topic = Self::queue_config_to_topic(&config);
        Ok(Response::new(topic))
    }

    /// Lists matching topics.
    async fn list_topics(
        &self,
        request: Request<ListTopicsRequest>,
    ) -> std::result::Result<Response<ListTopicsResponse>, Status> {
        let req = request.into_inner();
        debug!("ListTopics: project={}", req.project);

        // Extract project ID from "projects/{project}" format
        let project_id = req
            .project
            .strip_prefix("projects/")
            .unwrap_or(&req.project);

        // List all queues and filter for Pub/Sub topics
        let configs = self
            .backend
            .list_queues(None)
            .await
            .map_err(|e| Status::internal(format!("Failed to list topics: {}", e)))?;

        let topics: Vec<Topic> = configs
            .iter()
            .filter(|c| c.queue_type == QueueType::PubSubTopic)
            .filter(|c| {
                // Filter by project
                if let Ok(resource_name) = ResourceName::parse(&c.name) {
                    resource_name.project() == project_id
                } else {
                    false
                }
            })
            .map(Self::queue_config_to_topic)
            .collect();

        let response = ListTopicsResponse {
            topics,
            next_page_token: String::new(), // TODO: Implement pagination
        };

        Ok(Response::new(response))
    }

    /// Lists the names of the attached subscriptions on this topic.
    async fn list_topic_subscriptions(
        &self,
        request: Request<ListTopicSubscriptionsRequest>,
    ) -> std::result::Result<Response<ListTopicSubscriptionsResponse>, Status> {
        let req = request.into_inner();
        debug!("ListTopicSubscriptions: {}", req.topic);

        // Parse topic name
        let resource_name = ResourceName::parse(&req.topic)
            .map_err(|e| Status::invalid_argument(format!("Invalid topic name: {}", e)))?;

        let topic_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Verify topic exists
        self.backend
            .get_queue(&topic_id)
            .await
            .map_err(|_| Status::not_found(format!("Topic not found: {}", req.topic)))?;

        // Get all subscriptions and filter by topic
        let all_subs = self
            .backend
            .list_subscriptions()
            .await
            .map_err(|e| Status::internal(format!("Failed to list subscriptions: {}", e)))?;

        let subscriptions: Vec<String> = all_subs
            .iter()
            .filter(|s| s.topic_id == topic_id)
            .map(|s| s.name.clone())
            .collect();

        let response = ListTopicSubscriptionsResponse {
            subscriptions,
            next_page_token: String::new(), // TODO: Implement pagination
        };

        Ok(Response::new(response))
    }

    /// Deletes the topic with the given name.
    async fn delete_topic(
        &self,
        request: Request<DeleteTopicRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("DeleteTopic: {}", req.topic);

        // Parse topic name
        let resource_name = ResourceName::parse(&req.topic)
            .map_err(|e| Status::invalid_argument(format!("Invalid topic name: {}", e)))?;

        let topic_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Delete from backend
        self.backend
            .delete_queue(&topic_id)
            .await
            .map_err(|_| Status::not_found(format!("Topic not found: {}", req.topic)))?;

        Ok(Response::new(()))
    }

    /// Detaches a subscription from this topic.
    async fn detach_subscription(
        &self,
        request: Request<DetachSubscriptionRequest>,
    ) -> std::result::Result<Response<DetachSubscriptionResponse>, Status> {
        let req = request.into_inner();
        info!("DetachSubscription: {}", req.subscription);

        // TODO: Implement subscription detachment
        // For now, return not implemented
        Err(Status::unimplemented(
            "DetachSubscription not yet implemented",
        ))
    }
}

impl PublisherService {
    /// Enqueue push subscriptions for published messages.
    async fn enqueue_push_subscriptions(
        &self,
        subscriptions: &[crate::types::SubscriptionConfig],
        messages: &[Message],
    ) {
        if let Some(ref queue) = self.delivery_queue {
            // Enqueue tasks for each push subscription and message combination
            for sub in subscriptions {
                if sub.push_config.is_some() {
                    for message in messages {
                        let task = DeliveryTask {
                            message: message.clone(),
                            subscription: Arc::new(sub.clone()),
                            attempt: 0,
                            scheduled_time: Instant::now(),
                        };
                        queue.enqueue(task).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::InMemoryBackend;
    use crate::types::SubscriptionConfig;
    use tonic::Request;

    /// Create a test publisher service with in-memory backend.
    fn create_test_service() -> PublisherService {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        PublisherService::new(backend, None)
    }

    // ========================================================================
    // Helper Function Tests
    // ========================================================================

    #[test]
    fn test_topic_to_queue_config_conversion() {
        let topic = Topic {
            name: "projects/test-project/topics/test-topic".to_string(),
            labels: [("env".to_string(), "test".to_string())]
                .into_iter()
                .collect(),
            message_retention_duration: Some(prost_types::Duration {
                seconds: 86400,
                nanos: 0,
            }),
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        let config = PublisherService::topic_to_queue_config(&topic).unwrap();

        assert_eq!(config.id, "test-project:test-topic");
        assert_eq!(config.name, "projects/test-project/topics/test-topic");
        assert_eq!(config.queue_type, QueueType::PubSubTopic);
        assert_eq!(config.message_retention_period, 86400);
        assert_eq!(config.tags.get("env").unwrap(), "test");
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_queue_config_to_topic_conversion() {
        let config = QueueConfig {
            id: "test-project:test-topic".to_string(),
            name: "projects/test-project/topics/test-topic".to_string(),
            queue_type: QueueType::PubSubTopic,
            visibility_timeout: 60,
            message_retention_period: 604800,
            max_message_size: 10 * 1024 * 1024,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: [("key".to_string(), "value".to_string())]
                .into_iter()
                .collect(),
            redrive_allow_policy: None,
        };

        let topic = PublisherService::queue_config_to_topic(&config);

        assert_eq!(topic.name, "projects/test-project/topics/test-topic");
        assert_eq!(topic.labels.get("key").unwrap(), "value");
        assert_eq!(topic.message_retention_duration.unwrap().seconds, 604800);
    }

    #[test]
    fn test_pubsub_message_to_message_conversion() {
        let pubsub_msg = PubsubMessage {
            data: b"Hello world".to_vec(),
            attributes: [("attr1".to_string(), "value1".to_string())]
                .into_iter()
                .collect(),
            message_id: String::new(),
            publish_time: None,
            ordering_key: "order-key-1".to_string(),
        };

        let message = PublisherService::pubsub_message_to_message(&pubsub_msg, "topic:test");

        // Body should be base64 encoded
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&message.body)
            .unwrap();
        assert_eq!(decoded, b"Hello world");

        assert_eq!(message.queue_id, "topic:test");
        assert_eq!(message.message_group_id, Some("order-key-1".to_string()));
        assert_eq!(
            message
                .attributes
                .get("attr1")
                .unwrap()
                .string_value
                .as_ref()
                .unwrap(),
            "value1"
        );
        assert_eq!(message.receive_count, 0);
    }

    #[test]
    fn test_pubsub_message_without_ordering_key() {
        let pubsub_msg = PubsubMessage {
            data: b"test".to_vec(),
            attributes: Default::default(),
            message_id: String::new(),
            publish_time: None,
            ordering_key: String::new(), // Empty ordering key
        };

        let message = PublisherService::pubsub_message_to_message(&pubsub_msg, "topic:test");

        assert!(message.message_group_id.is_none());
    }

    // ========================================================================
    // CreateTopic Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_topic_success() {
        let service = create_test_service();

        let topic = Topic {
            name: "projects/test-project/topics/my-topic".to_string(),
            labels: [("env".to_string(), "dev".to_string())]
                .into_iter()
                .collect(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        let request = Request::new(topic.clone());
        let response = service.create_topic(request).await.unwrap();
        let created_topic = response.into_inner();

        assert_eq!(created_topic.name, "projects/test-project/topics/my-topic");
        assert_eq!(created_topic.labels.get("env").unwrap(), "dev");
    }

    #[tokio::test]
    async fn test_create_topic_already_exists() {
        let service = create_test_service();

        let topic = Topic {
            name: "projects/test-project/topics/duplicate-topic".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        // Create first time
        let request = Request::new(topic.clone());
        service.create_topic(request).await.unwrap();

        // Create again (should fail)
        let request = Request::new(topic);
        let result = service.create_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::AlreadyExists);
    }

    #[tokio::test]
    async fn test_create_topic_invalid_name() {
        let service = create_test_service();

        let topic = Topic {
            name: "invalid-topic-name".to_string(), // Invalid format
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        let request = Request::new(topic);
        let result = service.create_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_create_topic_invalid_id() {
        let service = create_test_service();

        let topic = Topic {
            name: "projects/test-project/topics/123invalid".to_string(), // Topic ID starts with number
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        let request = Request::new(topic);
        let result = service.create_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    // ========================================================================
    // UpdateTopic Tests
    // ========================================================================

    #[tokio::test]
    async fn test_update_topic_success() {
        let service = create_test_service();

        // Create topic first
        let topic = Topic {
            name: "projects/test-project/topics/update-topic".to_string(),
            labels: [("version".to_string(), "v1".to_string())]
                .into_iter()
                .collect(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        let request = Request::new(topic);
        service.create_topic(request).await.unwrap();

        // Update with new labels
        let updated_topic = Topic {
            name: "projects/test-project/topics/update-topic".to_string(),
            labels: [("version".to_string(), "v2".to_string())]
                .into_iter()
                .collect(),
            message_retention_duration: Some(prost_types::Duration {
                seconds: 172800,
                nanos: 0,
            }),
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        let update_request = UpdateTopicRequest {
            topic: Some(updated_topic),
            update_mask: None,
        };

        let request = Request::new(update_request);
        let response = service.update_topic(request).await.unwrap();
        let result_topic = response.into_inner();

        assert_eq!(result_topic.labels.get("version").unwrap(), "v2");
        assert_eq!(
            result_topic.message_retention_duration.unwrap().seconds,
            172800
        );
    }

    #[tokio::test]
    async fn test_update_topic_missing_topic() {
        let service = create_test_service();

        let update_request = UpdateTopicRequest {
            topic: None, // Missing topic
            update_mask: None,
        };

        let request = Request::new(update_request);
        let result = service.update_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    // ========================================================================
    // GetTopic Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_topic_success() {
        let service = create_test_service();

        // Create topic first
        let topic = Topic {
            name: "projects/test-project/topics/get-topic".to_string(),
            labels: [("key".to_string(), "value".to_string())]
                .into_iter()
                .collect(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        let request = Request::new(topic);
        service.create_topic(request).await.unwrap();

        // Get topic
        let get_request = GetTopicRequest {
            topic: "projects/test-project/topics/get-topic".to_string(),
        };

        let request = Request::new(get_request);
        let response = service.get_topic(request).await.unwrap();
        let retrieved_topic = response.into_inner();

        assert_eq!(
            retrieved_topic.name,
            "projects/test-project/topics/get-topic"
        );
        assert_eq!(retrieved_topic.labels.get("key").unwrap(), "value");
    }

    #[tokio::test]
    async fn test_get_topic_not_found() {
        let service = create_test_service();

        let get_request = GetTopicRequest {
            topic: "projects/test-project/topics/nonexistent".to_string(),
        };

        let request = Request::new(get_request);
        let result = service.get_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_get_topic_invalid_name() {
        let service = create_test_service();

        let get_request = GetTopicRequest {
            topic: "invalid-format".to_string(),
        };

        let request = Request::new(get_request);
        let result = service.get_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    // ========================================================================
    // ListTopics Tests
    // ========================================================================

    #[tokio::test]
    async fn test_list_topics_empty() {
        let service = create_test_service();

        let list_request = ListTopicsRequest {
            project: "projects/test-project".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_topics(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.topics.len(), 0);
    }

    #[tokio::test]
    async fn test_list_topics_with_data() {
        let service = create_test_service();

        // Create two topics
        let topic1 = Topic {
            name: "projects/test-project/topics/topic1".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic1)).await.unwrap();

        let topic2 = Topic {
            name: "projects/test-project/topics/topic2".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic2)).await.unwrap();

        // List topics
        let list_request = ListTopicsRequest {
            project: "projects/test-project".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_topics(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.topics.len(), 2);
    }

    #[tokio::test]
    async fn test_list_topics_filters_by_project() {
        let service = create_test_service();

        // Create topics in different projects
        let topic_a = Topic {
            name: "projects/project-a/topics/topic1".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic_a)).await.unwrap();

        let topic_b = Topic {
            name: "projects/project-b/topics/topic2".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic_b)).await.unwrap();

        // List topics for project-a only
        let list_request = ListTopicsRequest {
            project: "projects/project-a".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_topics(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.topics.len(), 1);
        assert!(list_response.topics[0].name.contains("project-a"));
    }

    // ========================================================================
    // Publish Tests
    // ========================================================================

    #[tokio::test]
    async fn test_publish_success_no_subscriptions() {
        let service = create_test_service();

        // Create topic
        let topic = Topic {
            name: "projects/test-project/topics/pub-topic".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Publish messages (no subscriptions)
        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/pub-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"Message 1".to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };

        let request = Request::new(publish_request);
        let response = service.publish(request).await.unwrap();
        let publish_response = response.into_inner();

        assert_eq!(publish_response.message_ids.len(), 1);
        assert!(!publish_response.message_ids[0].is_empty());
    }

    #[tokio::test]
    async fn test_publish_with_subscription_fanout() {
        let service = create_test_service();

        // Create topic
        let topic = Topic {
            name: "projects/test-project/topics/fanout-topic".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Create backing queue for subscription (subscriptions have their own message storage)
        let sub_queue_config = QueueConfig {
            id: "test-project:test-sub".to_string(),
            name: "projects/test-project/subscriptions/test-sub".to_string(),
            queue_type: QueueType::PubSubTopic,
            visibility_timeout: 60,
            message_retention_period: 604800,
            max_message_size: 10 * 1024 * 1024,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: Default::default(),
            redrive_allow_policy: None,
        };
        service
            .backend
            .create_queue(sub_queue_config)
            .await
            .unwrap();

        // Create subscription for the topic
        let sub_config = SubscriptionConfig {
            id: "test-project:test-sub".to_string(),
            name: "projects/test-project/subscriptions/test-sub".to_string(),
            topic_id: "test-project:fanout-topic".to_string(),
            ack_deadline_seconds: 10,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: None,
        };
        service
            .backend
            .create_subscription(sub_config)
            .await
            .unwrap();

        // Publish messages
        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/fanout-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"Fanout message".to_vec(),
                attributes: [("key".to_string(), "value".to_string())]
                    .into_iter()
                    .collect(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };

        let request = Request::new(publish_request);
        let response = service.publish(request).await.unwrap();
        let publish_response = response.into_inner();

        assert_eq!(publish_response.message_ids.len(), 1);

        // Verify message was fanned out to subscription
        let messages = service
            .backend
            .receive_messages(
                "test-project:test-sub",
                crate::types::ReceiveOptions {
                    max_messages: 10,
                    visibility_timeout: Some(30),
                    wait_time_seconds: 0,
                    attribute_names: vec![],
                    message_attribute_names: vec![],
                },
            )
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&messages[0].message.body)
            .unwrap();
        assert_eq!(decoded, b"Fanout message");
    }

    #[tokio::test]
    async fn test_publish_topic_not_found() {
        let service = create_test_service();

        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/nonexistent".to_string(),
            messages: vec![PubsubMessage {
                data: b"test".to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };

        let request = Request::new(publish_request);
        let result = service.publish(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_publish_with_ordering_key() {
        let service = create_test_service();

        // Create topic
        let topic = Topic {
            name: "projects/test-project/topics/ordered-topic".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Publish messages with ordering key
        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/ordered-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"Ordered message".to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: "order-key-1".to_string(),
            }],
        };

        let request = Request::new(publish_request);
        let response = service.publish(request).await.unwrap();
        let publish_response = response.into_inner();

        assert_eq!(publish_response.message_ids.len(), 1);
    }

    // ========================================================================
    // ListTopicSubscriptions Tests
    // ========================================================================

    #[tokio::test]
    async fn test_list_topic_subscriptions_empty() {
        let service = create_test_service();

        // Create topic
        let topic = Topic {
            name: "projects/test-project/topics/list-sub-topic".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // List subscriptions (should be empty)
        let list_request = ListTopicSubscriptionsRequest {
            topic: "projects/test-project/topics/list-sub-topic".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_topic_subscriptions(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.subscriptions.len(), 0);
    }

    #[tokio::test]
    async fn test_list_topic_subscriptions_with_data() {
        let service = create_test_service();

        // Create topic
        let topic = Topic {
            name: "projects/test-project/topics/list-sub-topic2".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Create two subscriptions
        let sub1 = SubscriptionConfig {
            id: "test-project:sub1".to_string(),
            name: "projects/test-project/subscriptions/sub1".to_string(),
            topic_id: "test-project:list-sub-topic2".to_string(),
            ack_deadline_seconds: 10,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: None,
        };
        service.backend.create_subscription(sub1).await.unwrap();

        let sub2 = SubscriptionConfig {
            id: "test-project:sub2".to_string(),
            name: "projects/test-project/subscriptions/sub2".to_string(),
            topic_id: "test-project:list-sub-topic2".to_string(),
            ack_deadline_seconds: 10,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: None,
        };
        service.backend.create_subscription(sub2).await.unwrap();

        // List subscriptions
        let list_request = ListTopicSubscriptionsRequest {
            topic: "projects/test-project/topics/list-sub-topic2".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_topic_subscriptions(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.subscriptions.len(), 2);
    }

    #[tokio::test]
    async fn test_list_topic_subscriptions_topic_not_found() {
        let service = create_test_service();

        let list_request = ListTopicSubscriptionsRequest {
            topic: "projects/test-project/topics/nonexistent".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let result = service.list_topic_subscriptions(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    // ========================================================================
    // DeleteTopic Tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_topic_success() {
        let service = create_test_service();

        // Create topic
        let topic = Topic {
            name: "projects/test-project/topics/delete-topic".to_string(),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Delete topic
        let delete_request = DeleteTopicRequest {
            topic: "projects/test-project/topics/delete-topic".to_string(),
        };

        let request = Request::new(delete_request);
        let response = service.delete_topic(request).await;

        assert!(response.is_ok());

        // Verify topic is deleted
        let get_request = GetTopicRequest {
            topic: "projects/test-project/topics/delete-topic".to_string(),
        };
        let result = service.get_topic(Request::new(get_request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_topic_not_found() {
        let service = create_test_service();

        let delete_request = DeleteTopicRequest {
            topic: "projects/test-project/topics/nonexistent".to_string(),
        };

        let request = Request::new(delete_request);
        let result = service.delete_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_delete_topic_invalid_name() {
        let service = create_test_service();

        let delete_request = DeleteTopicRequest {
            topic: "invalid-format".to_string(),
        };

        let request = Request::new(delete_request);
        let result = service.delete_topic(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    // ========================================================================
    // DetachSubscription Tests
    // ========================================================================

    #[tokio::test]
    async fn test_detach_subscription_unimplemented() {
        let service = create_test_service();

        let detach_request = DetachSubscriptionRequest {
            subscription: "projects/test-project/subscriptions/test-sub".to_string(),
        };

        let request = Request::new(detach_request);
        let result = service.detach_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_publish_enqueues_push_subscriptions() {
        use crate::pubsub::push_queue::DeliveryQueue;
        use crate::types::{PushConfig, RetryPolicy};
        use std::time::Instant;

        let backend = Arc::new(InMemoryBackend::new());
        let delivery_queue = DeliveryQueue::new();
        let service = PublisherService::new(backend.clone(), Some(delivery_queue.clone()));

        // Create topic
        let topic = Topic {
            name: "projects/test/topics/test-topic".to_string(),
            labels: std::collections::HashMap::new(),
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
            message_retention_duration: None,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Create backing queue for subscription (subscriptions have their own message storage)
        let sub_queue_config = QueueConfig {
            id: "test:test-sub".to_string(),
            name: "projects/test/subscriptions/test-sub".to_string(),
            queue_type: QueueType::PubSubTopic,
            visibility_timeout: 60,
            message_retention_period: 604800,
            max_message_size: 10 * 1024 * 1024,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: Default::default(),
            redrive_allow_policy: None,
        };
        backend.create_queue(sub_queue_config).await.unwrap();

        // Create push subscription
        let subscription = crate::types::SubscriptionConfig {
            id: "test:test-sub".to_string(),
            name: "projects/test/subscriptions/test-sub".to_string(),
            topic_id: "test:test-topic".to_string(),
            ack_deadline_seconds: 30,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: Some(PushConfig {
                endpoint: "https://example.com/webhook".to_string(),
                retry_policy: Some(RetryPolicy::default()),
                timeout_seconds: Some(30),
            }),
        };
        backend.create_subscription(subscription).await.unwrap();

        // Publish message
        let publish_req = PublishRequest {
            topic: "projects/test/topics/test-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"test message".to_vec(),
                attributes: std::collections::HashMap::new(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };

        service.publish(Request::new(publish_req)).await.unwrap();

        // Verify task was enqueued
        assert_eq!(delivery_queue.len().await, 1);

        let task = delivery_queue.dequeue_ready().await.unwrap();
        assert_eq!(task.attempt, 0);
        assert!(task.scheduled_time <= Instant::now());
    }
}
