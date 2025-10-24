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
use crate::pubsub::types::{validate_topic_id, ResourceName};
use crate::storage::StorageBackend;
use crate::types::{Message, MessageAttributeValue, MessageAttributes, MessageId, QueueConfig, QueueType};
use base64::Engine;
use chrono::Utc;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

/// Publisher service implementation.
pub struct PublisherService {
    backend: Arc<dyn StorageBackend>,
}

impl PublisherService {
    /// Create a new Publisher service.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Convert proto Topic to our internal QueueConfig.
    fn topic_to_queue_config(topic: &Topic) -> Result<QueueConfig> {
        let resource_name = ResourceName::parse(&topic.name)?;
        let topic_id = format!("{}:{}", resource_name.project(), resource_name.resource_id());

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
    fn pubsub_message_to_message(
        msg: &PubsubMessage,
        topic_id: &str,
    ) -> Message {
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
        let created_config = self
            .backend
            .create_queue(queue_config)
            .await
            .map_err(|e| match e {
                Error::QueueAlreadyExists(_) => Status::already_exists(format!("Topic already exists: {}", topic.name)),
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
        let topic = req.topic.ok_or_else(|| Status::invalid_argument("Topic is required"))?;

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

        let topic_id = format!("{}:{}", resource_name.project(), resource_name.resource_id());

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
                message.message_group_id.as_ref().unwrap_or(&"<none>".to_string())
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
                    .map_err(|e| Status::internal(format!("Failed to publish messages to subscription {}: {}", sub.id, e)))?;

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

        let topic_id = format!("{}:{}", resource_name.project(), resource_name.resource_id());

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
        let project_id = req.project
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

        let topic_id = format!("{}:{}", resource_name.project(), resource_name.resource_id());

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

        let topic_id = format!("{}:{}", resource_name.project(), resource_name.resource_id());

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
        Err(Status::unimplemented("DetachSubscription not yet implemented"))
    }
}

