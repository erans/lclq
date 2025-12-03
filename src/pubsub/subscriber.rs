//! Subscriber service implementation.
//!
//! Implements the Google Cloud Pub/Sub Subscriber service, including:
//! - CreateSubscription
//! - GetSubscription
//! - UpdateSubscription
//! - ListSubscriptions
//! - DeleteSubscription
//! - Pull
//! - StreamingPull
//! - Acknowledge
//! - ModifyAckDeadline
//! - ModifyPushConfig
//! - Seek

use crate::error::Result;
use crate::pubsub::proto::subscriber_server::Subscriber;
use crate::pubsub::proto::*;
use crate::pubsub::types::{ResourceName, validate_subscription_id};
use crate::storage::StorageBackend;
use crate::types::{QueueConfig, QueueType, ReceiveOptions, SubscriptionConfig};
use base64::Engine;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

/// Subscriber service implementation.
pub struct SubscriberService {
    backend: Arc<dyn StorageBackend>,
}

impl SubscriberService {
    /// Create a new Subscriber service.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Convert proto Subscription to our internal SubscriptionConfig.
    fn subscription_to_config(subscription: &Subscription) -> Result<SubscriptionConfig> {
        let resource_name = ResourceName::parse(&subscription.name)?;
        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Parse topic name to get topic_id
        let topic_resource = ResourceName::parse(&subscription.topic)?;
        let topic_id = format!(
            "{}:{}",
            topic_resource.project(),
            topic_resource.resource_id()
        );

        // Parse push config if present
        let push_config = subscription.push_config.as_ref().map(|pc| {
            let retry_policy = subscription.retry_policy.as_ref().map(|rp| {
                crate::types::RetryPolicy {
                    min_backoff_seconds: rp
                        .minimum_backoff
                        .as_ref()
                        .map(|d| d.seconds as u32)
                        .unwrap_or(10),
                    max_backoff_seconds: rp
                        .maximum_backoff
                        .as_ref()
                        .map(|d| d.seconds as u32)
                        .unwrap_or(600),
                    max_attempts: 5, // Not in proto, use default
                }
            });

            crate::types::PushConfig {
                endpoint: pc.push_endpoint.clone(),
                retry_policy,
                timeout_seconds: Some(30), // Default timeout
            }
        });

        Ok(SubscriptionConfig {
            id: sub_id.clone(),
            name: subscription.name.clone(),
            topic_id,
            ack_deadline_seconds: subscription.ack_deadline_seconds as u32,
            message_retention_duration: subscription
                .message_retention_duration
                .as_ref()
                .map(|d| d.seconds as u32)
                .unwrap_or(604800), // 7 days default
            enable_message_ordering: subscription.enable_message_ordering,
            filter: if subscription.filter.is_empty() {
                None
            } else {
                Some(subscription.filter.clone())
            },
            dead_letter_policy: subscription.dead_letter_policy.as_ref().map(|dlp| {
                crate::types::DeadLetterPolicy {
                    dead_letter_topic: dlp.dead_letter_topic.clone(),
                    max_delivery_attempts: dlp.max_delivery_attempts as u32,
                }
            }),
            push_config,
        })
    }

    /// Convert our internal SubscriptionConfig to proto Subscription.
    fn config_to_subscription(config: &SubscriptionConfig) -> Subscription {
        // Parse the subscription name to get project and subscription ID
        let parts: Vec<&str> = config.id.split(':').collect();
        let (project, _sub_id) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            ("default", config.id.as_str())
        };

        // Parse topic_id to get topic name
        let topic_parts: Vec<&str> = config.topic_id.split(':').collect();
        let (topic_project, topic_name) = if topic_parts.len() == 2 {
            (topic_parts[0], topic_parts[1])
        } else {
            (project, config.topic_id.as_str())
        };

        let topic = format!("projects/{}/topics/{}", topic_project, topic_name);

        // Serialize push config
        let push_config = config.push_config.as_ref().map(|pc| PushConfig {
            push_endpoint: pc.endpoint.clone(),
            attributes: std::collections::HashMap::new(),
            authentication_method: None,
        });

        let retry_policy = config
            .push_config
            .as_ref()
            .and_then(|pc| pc.retry_policy.as_ref())
            .map(|rp| RetryPolicy {
                minimum_backoff: Some(prost_types::Duration {
                    seconds: rp.min_backoff_seconds as i64,
                    nanos: 0,
                }),
                maximum_backoff: Some(prost_types::Duration {
                    seconds: rp.max_backoff_seconds as i64,
                    nanos: 0,
                }),
            });

        Subscription {
            name: config.name.clone(),
            topic,
            push_config,
            ack_deadline_seconds: config.ack_deadline_seconds as i32,
            retain_acked_messages: false,
            message_retention_duration: Some(prost_types::Duration {
                seconds: config.message_retention_duration as i64,
                nanos: 0,
            }),
            labels: Default::default(),
            enable_message_ordering: config.enable_message_ordering,
            expiration_policy: None,
            filter: config.filter.clone().unwrap_or_default(),
            dead_letter_policy: config
                .dead_letter_policy
                .as_ref()
                .map(|dlp| DeadLetterPolicy {
                    dead_letter_topic: dlp.dead_letter_topic.clone(),
                    max_delivery_attempts: dlp.max_delivery_attempts as i32,
                }),
            retry_policy,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        }
    }
}

#[tonic::async_trait]
impl Subscriber for SubscriberService {
    /// Creates a subscription to a given topic.
    async fn create_subscription(
        &self,
        request: Request<Subscription>,
    ) -> std::result::Result<Response<Subscription>, Status> {
        let subscription = request.into_inner();
        info!("CreateSubscription: {}", subscription.name);

        // Validate subscription name
        let resource_name = ResourceName::parse(&subscription.name)
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription name: {}", e)))?;

        // Validate subscription ID
        validate_subscription_id(resource_name.resource_id())
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription ID: {}", e)))?;

        // Verify topic exists
        let topic_resource = ResourceName::parse(&subscription.topic)
            .map_err(|e| Status::invalid_argument(format!("Invalid topic name: {}", e)))?;
        let topic_id = format!(
            "{}:{}",
            topic_resource.project(),
            topic_resource.resource_id()
        );

        self.backend
            .get_queue(&topic_id)
            .await
            .map_err(|_| Status::not_found(format!("Topic not found: {}", subscription.topic)))?;

        // Convert to config
        let config = Self::subscription_to_config(&subscription)
            .map_err(|e| Status::internal(format!("Failed to convert subscription: {}", e)))?;

        // Create a queue for this subscription to store its messages
        let sub_queue_config = QueueConfig {
            id: config.id.clone(),
            name: config.name.clone(),
            queue_type: QueueType::PubSubTopic, // Subscriptions use PubSubTopic queue type
            visibility_timeout: config.ack_deadline_seconds,
            message_retention_period: 604800,   // 7 days
            max_message_size: 10 * 1024 * 1024, // 10 MB
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };

        // Create the queue
        self.backend
            .create_queue(sub_queue_config)
            .await
            .map_err(|e| Status::internal(format!("Failed to create subscription queue: {}", e)))?;

        // Create the subscription metadata
        let created_config = self
            .backend
            .create_subscription(config)
            .await
            .map_err(|e| Status::internal(format!("Failed to create subscription: {}", e)))?;

        let response_sub = Self::config_to_subscription(&created_config);
        Ok(Response::new(response_sub))
    }

    /// Gets the configuration details of a subscription.
    async fn get_subscription(
        &self,
        request: Request<GetSubscriptionRequest>,
    ) -> std::result::Result<Response<Subscription>, Status> {
        let req = request.into_inner();
        debug!("GetSubscription: {}", req.subscription);

        // Parse subscription name
        let resource_name = ResourceName::parse(&req.subscription)
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription name: {}", e)))?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get from backend
        let config = self.backend.get_subscription(&sub_id).await.map_err(|_| {
            Status::not_found(format!("Subscription not found: {}", req.subscription))
        })?;

        let subscription = Self::config_to_subscription(&config);
        Ok(Response::new(subscription))
    }

    /// Updates an existing subscription.
    async fn update_subscription(
        &self,
        request: Request<UpdateSubscriptionRequest>,
    ) -> std::result::Result<Response<Subscription>, Status> {
        let _req = request.into_inner();
        // TODO: Implement subscription update
        Err(Status::unimplemented(
            "UpdateSubscription not yet implemented",
        ))
    }

    /// Lists matching subscriptions.
    async fn list_subscriptions(
        &self,
        request: Request<ListSubscriptionsRequest>,
    ) -> std::result::Result<Response<ListSubscriptionsResponse>, Status> {
        let req = request.into_inner();
        debug!("ListSubscriptions: project={}", req.project);

        // Extract project ID from "projects/{project}" format
        let project_id = req
            .project
            .strip_prefix("projects/")
            .unwrap_or(&req.project);

        // List all subscriptions
        let configs = self
            .backend
            .list_subscriptions()
            .await
            .map_err(|e| Status::internal(format!("Failed to list subscriptions: {}", e)))?;

        // Filter by project
        let subscriptions: Vec<Subscription> = configs
            .iter()
            .filter(|c| {
                if let Ok(resource_name) = ResourceName::parse(&c.name) {
                    resource_name.project() == project_id
                } else {
                    false
                }
            })
            .map(Self::config_to_subscription)
            .collect();

        let response = ListSubscriptionsResponse {
            subscriptions,
            next_page_token: String::new(), // TODO: Implement pagination
        };

        Ok(Response::new(response))
    }

    /// Deletes an existing subscription.
    async fn delete_subscription(
        &self,
        request: Request<DeleteSubscriptionRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("DeleteSubscription: {}", req.subscription);

        // Parse subscription name
        let resource_name = ResourceName::parse(&req.subscription)
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription name: {}", e)))?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Delete the subscription queue first
        let _ = self.backend.delete_queue(&sub_id).await; // Ignore errors if queue doesn't exist

        // Delete subscription metadata
        self.backend
            .delete_subscription(&sub_id)
            .await
            .map_err(|_| {
                Status::not_found(format!("Subscription not found: {}", req.subscription))
            })?;

        Ok(Response::new(()))
    }

    /// Modifies the ack deadline for a specific message.
    async fn modify_ack_deadline(
        &self,
        request: Request<ModifyAckDeadlineRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        let req = request.into_inner();
        debug!("ModifyAckDeadline: {} messages", req.ack_ids.len());

        // Parse subscription name
        let resource_name = ResourceName::parse(&req.subscription)
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription name: {}", e)))?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Verify subscription exists
        let _ = self.backend.get_subscription(&sub_id).await.map_err(|_| {
            Status::not_found(format!("Subscription not found: {}", req.subscription))
        })?;

        // Modify visibility for each ack_id (receipt handle)
        for ack_id in &req.ack_ids {
            self.backend
                .change_visibility(&sub_id, ack_id, req.ack_deadline_seconds as u32)
                .await
                .map_err(|e| Status::internal(format!("Failed to modify ack deadline: {}", e)))?;
        }

        Ok(Response::new(()))
    }

    /// Acknowledges the messages associated with the `ack_ids` in the
    /// `AcknowledgeRequest`.
    async fn acknowledge(
        &self,
        request: Request<AcknowledgeRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        let req = request.into_inner();
        debug!("Acknowledge: {} messages", req.ack_ids.len());

        // Parse subscription name to get topic
        let resource_name = ResourceName::parse(&req.subscription)
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription name: {}", e)))?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Verify subscription exists
        let _ = self.backend.get_subscription(&sub_id).await.map_err(|_| {
            Status::not_found(format!("Subscription not found: {}", req.subscription))
        })?;

        // Delete each message using ack_id (receipt handle)
        for ack_id in &req.ack_ids {
            self.backend
                .delete_message(&sub_id, ack_id)
                .await
                .map_err(|e| Status::internal(format!("Failed to acknowledge message: {}", e)))?;
        }

        Ok(Response::new(()))
    }

    /// Pulls messages from the server.
    async fn pull(
        &self,
        request: Request<PullRequest>,
    ) -> std::result::Result<Response<PullResponse>, Status> {
        let req = request.into_inner();
        debug!("Pull: max_messages={}", req.max_messages);

        // Parse subscription name
        let resource_name = ResourceName::parse(&req.subscription)
            .map_err(|e| Status::invalid_argument(format!("Invalid subscription name: {}", e)))?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get subscription config
        let config = self.backend.get_subscription(&sub_id).await.map_err(|_| {
            Status::not_found(format!("Subscription not found: {}", req.subscription))
        })?;

        // Receive messages from subscription queue (not topic - messages are fanned out to subscriptions)
        let options = ReceiveOptions {
            max_messages: req.max_messages as u32,
            visibility_timeout: Some(config.ack_deadline_seconds),
            wait_time_seconds: 0,
            attribute_names: vec![],
            message_attribute_names: vec![],
        };

        let messages = self
            .backend
            .receive_messages(&sub_id, options)
            .await
            .map_err(|e| Status::internal(format!("Failed to pull messages: {}", e)))?;

        // Convert to proto ReceivedMessage
        let received_messages: Vec<ReceivedMessage> = messages
            .iter()
            .map(|rm| {
                // Decode base64 body
                let data = base64::engine::general_purpose::STANDARD
                    .decode(&rm.message.body)
                    .unwrap_or_else(|_| rm.message.body.as_bytes().to_vec());

                // Convert attributes
                let mut attributes = std::collections::HashMap::new();
                for (key, value) in &rm.message.attributes {
                    if let Some(string_val) = &value.string_value {
                        attributes.insert(key.clone(), string_val.clone());
                    }
                }

                let pubsub_message = PubsubMessage {
                    data,
                    attributes,
                    message_id: rm.message.id.0.clone(),
                    publish_time: Some(prost_types::Timestamp {
                        seconds: rm.message.sent_timestamp.timestamp(),
                        nanos: rm.message.sent_timestamp.timestamp_subsec_nanos() as i32,
                    }),
                    ordering_key: rm.message.message_group_id.clone().unwrap_or_default(),
                };

                ReceivedMessage {
                    ack_id: rm.receipt_handle.clone(),
                    message: Some(pubsub_message),
                    delivery_attempt: rm.message.receive_count as i32,
                }
            })
            .collect();

        let response = PullResponse { received_messages };

        Ok(Response::new(response))
    }

    /// Server streaming response type for the StreamingPull method.
    type StreamingPullStream = ReceiverStream<std::result::Result<StreamingPullResponse, Status>>;

    /// Establishes a stream with the server for receiving messages.
    async fn streaming_pull(
        &self,
        request: Request<tonic::Streaming<StreamingPullRequest>>,
    ) -> std::result::Result<Response<Self::StreamingPullStream>, Status> {
        let mut in_stream = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let backend = self.backend.clone();

        // Spawn a task to handle the bidirectional stream
        tokio::spawn(async move {
            let mut subscription_id: Option<String> = None;
            let mut topic_id: Option<String> = None;
            let mut ack_deadline = 10; // Default 10 seconds
            let mut message_ordering_enabled = false; // Track if ordering is enabled

            // Main stream loop
            loop {
                tokio::select! {
                    // Handle incoming requests from client (acks, modify deadlines)
                    Some(result) = in_stream.next() => {
                        match result {
                            Ok(req) => {
                                debug!("StreamingPull request: subscription={}", req.subscription);

                                // Initialize subscription on first request
                                if subscription_id.is_none() {
                                    // Parse subscription name
                                    match ResourceName::parse(&req.subscription) {
                                        Ok(resource_name) => {
                                            let sub_id = format!(
                                                "{}:{}",
                                                resource_name.project(),
                                                resource_name.resource_id()
                                            );

                                            // Get subscription config to find topic
                                            match backend.get_subscription(&sub_id).await {
                                                Ok(config) => {
                                                    subscription_id = Some(sub_id.clone());
                                                    topic_id = Some(config.topic_id.clone());
                                                    message_ordering_enabled = config.enable_message_ordering;

                                                    if req.stream_ack_deadline_seconds > 0 {
                                                        ack_deadline = req.stream_ack_deadline_seconds as u32;
                                                    }

                                                    debug!("StreamingPull initialized for subscription: {}, topic: {}, ordering_enabled: {}", sub_id, config.topic_id, message_ordering_enabled);
                                                }
                                                Err(e) => {
                                                    let _ = tx.send(Err(Status::not_found(format!("Subscription not found: {}", e)))).await;
                                                    break;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(Status::invalid_argument(format!("Invalid subscription name: {}", e)))).await;
                                            break;
                                        }
                                    }
                                }

                                // Process acknowledgments
                                if !req.ack_ids.is_empty() {
                                    if let (Some(_topic), Some(sub_id)) = (&topic_id, &subscription_id) {
                                        for ack_id in &req.ack_ids {
                                            if let Err(e) = backend.delete_message(sub_id, ack_id).await {
                                                debug!("Failed to acknowledge message {}: {}", ack_id, e);
                                            }
                                        }
                                    }
                                }

                                // Process modify deadline requests
                                if !req.modify_deadline_ack_ids.is_empty() {
                                    if let (Some(_topic), Some(sub_id)) = (&topic_id, &subscription_id) {
                                        for (i, ack_id) in req.modify_deadline_ack_ids.iter().enumerate() {
                                            let deadline = req.modify_deadline_seconds.get(i).copied().unwrap_or(10);
                                            if let Err(e) = backend.change_visibility(sub_id, ack_id, deadline as u32).await {
                                                debug!("Failed to modify deadline for {}: {}", ack_id, e);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("StreamingPull stream error: {}", e);
                                break;
                            }
                        }
                    }

                    // Poll for messages to send to client
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        if let (Some(_topic), Some(sub_id)) = (&topic_id, &subscription_id) {
                            let options = ReceiveOptions {
                                max_messages: 10, // Pull up to 10 messages at a time
                                visibility_timeout: Some(ack_deadline),
                                wait_time_seconds: 0,
                                attribute_names: vec![],
                                message_attribute_names: vec![],
                            };

                            match backend.receive_messages(sub_id, options).await {
                                Ok(messages) => {
                                    if !messages.is_empty() {
                                        // Convert to ReceivedMessage format
                                        let received_messages: Vec<ReceivedMessage> = messages
                                            .into_iter()
                                            .map(|msg| {
                                                // Decode base64 body
                                                let data = base64::engine::general_purpose::STANDARD
                                                    .decode(&msg.message.body)
                                                    .unwrap_or_else(|_| msg.message.body.as_bytes().to_vec());

                                                // Convert message attributes to simple string map
                                                let attributes: std::collections::HashMap<String, String> = msg
                                                    .message
                                                    .attributes
                                                    .iter()
                                                    .map(|(k, v)| (k.clone(), v.string_value.clone().unwrap_or_default()))
                                                    .collect();

                                                ReceivedMessage {
                                                    ack_id: msg.receipt_handle,
                                                    message: Some(PubsubMessage {
                                                        data,
                                                        attributes,
                                                        message_id: msg.message.id.to_string(),
                                                        publish_time: Some(prost_types::Timestamp {
                                                            seconds: msg.message.sent_timestamp.timestamp(),
                                                            nanos: msg.message.sent_timestamp.timestamp_subsec_nanos() as i32,
                                                        }),
                                                        ordering_key: msg.message.message_group_id.unwrap_or_default(),
                                                    }),
                                                    delivery_attempt: msg.message.receive_count as i32,
                                                }
                                            })
                                            .collect();

                                        let response = StreamingPullResponse {
                                            received_messages,
                                            subscription_properties: Some(SubscriptionProperties {
                                                exactly_once_delivery_enabled: false,
                                                message_ordering_enabled,
                                            }),
                                        };

                                        if tx.send(Ok(response)).await.is_err() {
                                            debug!("Client disconnected");
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to receive messages: {}", e);
                                }
                            }
                        }
                    }
                }
            }

            debug!("StreamingPull ended");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// Modifies the `PushConfig` for a specified subscription.
    async fn modify_push_config(
        &self,
        _request: Request<ModifyPushConfigRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        // TODO: Implement push config modification
        Err(Status::unimplemented(
            "ModifyPushConfig not yet implemented",
        ))
    }

    /// Gets the configuration details of a snapshot.
    async fn get_snapshot(
        &self,
        _request: Request<GetSnapshotRequest>,
    ) -> std::result::Result<Response<Snapshot>, Status> {
        // TODO: Implement snapshot support
        Err(Status::unimplemented("GetSnapshot not yet implemented"))
    }

    /// Lists the existing snapshots.
    async fn list_snapshots(
        &self,
        _request: Request<ListSnapshotsRequest>,
    ) -> std::result::Result<Response<ListSnapshotsResponse>, Status> {
        // TODO: Implement snapshot support
        Err(Status::unimplemented("ListSnapshots not yet implemented"))
    }

    /// Creates a snapshot from the requested subscription.
    async fn create_snapshot(
        &self,
        _request: Request<CreateSnapshotRequest>,
    ) -> std::result::Result<Response<Snapshot>, Status> {
        // TODO: Implement snapshot support
        Err(Status::unimplemented("CreateSnapshot not yet implemented"))
    }

    /// Updates an existing snapshot.
    async fn update_snapshot(
        &self,
        _request: Request<UpdateSnapshotRequest>,
    ) -> std::result::Result<Response<Snapshot>, Status> {
        // TODO: Implement snapshot support
        Err(Status::unimplemented("UpdateSnapshot not yet implemented"))
    }

    /// Removes an existing snapshot.
    async fn delete_snapshot(
        &self,
        _request: Request<DeleteSnapshotRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        // TODO: Implement snapshot support
        Err(Status::unimplemented("DeleteSnapshot not yet implemented"))
    }

    /// Seeks an existing subscription to a point in time or to a given snapshot.
    async fn seek(
        &self,
        _request: Request<SeekRequest>,
    ) -> std::result::Result<Response<SeekResponse>, Status> {
        // TODO: Implement seek support
        Err(Status::unimplemented("Seek not yet implemented"))
    }
}

#[cfg(test)]
#[allow(deprecated)] // PullRequest::return_immediately is deprecated but still used in tests
mod tests {
    use super::*;
    use crate::pubsub::proto::publisher_server::Publisher;
    use crate::pubsub::publisher::PublisherService;
    use crate::storage::memory::InMemoryBackend;
    use tonic::Request;

    /// Create a test subscriber service with in-memory backend.
    fn create_test_service() -> SubscriberService {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        SubscriberService::new(backend)
    }

    /// Create a test publisher service (for creating topics).
    fn create_test_publisher(backend: Arc<dyn StorageBackend>) -> PublisherService {
        PublisherService::new(backend, None)
    }

    /// Helper to create a test topic.
    async fn create_test_topic(
        publisher: &PublisherService,
        project: &str,
        topic: &str,
    ) -> crate::pubsub::proto::Topic {
        let topic_proto = crate::pubsub::proto::Topic {
            name: format!("projects/{}/topics/{}", project, topic),
            labels: Default::default(),
            message_retention_duration: None,
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
        };

        let request = Request::new(topic_proto.clone());
        publisher.create_topic(request).await.unwrap();
        topic_proto
    }

    // ========================================================================
    // Helper Function Tests
    // ========================================================================

    #[test]
    fn test_subscription_to_config_conversion() {
        let subscription = Subscription {
            name: "projects/test-project/subscriptions/test-sub".to_string(),
            topic: "projects/test-project/topics/test-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 30,
            retain_acked_messages: false,
            message_retention_duration: Some(prost_types::Duration {
                seconds: 86400,
                nanos: 0,
            }),
            labels: Default::default(),
            enable_message_ordering: true,
            expiration_policy: None,
            filter: "attributes.key = 'value'".to_string(),
            dead_letter_policy: Some(DeadLetterPolicy {
                dead_letter_topic: "projects/test-project/topics/dlq".to_string(),
                max_delivery_attempts: 5,
            }),
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let config = SubscriberService::subscription_to_config(&subscription).unwrap();

        assert_eq!(config.id, "test-project:test-sub");
        assert_eq!(config.name, "projects/test-project/subscriptions/test-sub");
        assert_eq!(config.topic_id, "test-project:test-topic");
        assert_eq!(config.ack_deadline_seconds, 30);
        assert_eq!(config.message_retention_duration, 86400);
        assert!(config.enable_message_ordering);
        assert_eq!(config.filter, Some("attributes.key = 'value'".to_string()));
        assert!(config.dead_letter_policy.is_some());
        assert_eq!(
            config
                .dead_letter_policy
                .as_ref()
                .unwrap()
                .max_delivery_attempts,
            5
        );
    }

    #[test]
    fn test_subscription_to_config_without_optional_fields() {
        let subscription = Subscription {
            name: "projects/test/subscriptions/sub1".to_string(),
            topic: "projects/test/topics/topic1".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None, // Use default
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(), // Empty filter
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let config = SubscriberService::subscription_to_config(&subscription).unwrap();

        assert_eq!(config.message_retention_duration, 604800); // Default 7 days
        assert!(config.filter.is_none());
        assert!(config.dead_letter_policy.is_none());
    }

    #[test]
    fn test_config_to_subscription_conversion() {
        let config = SubscriptionConfig {
            id: "test-project:test-sub".to_string(),
            name: "projects/test-project/subscriptions/test-sub".to_string(),
            topic_id: "test-project:test-topic".to_string(),
            ack_deadline_seconds: 20,
            message_retention_duration: 172800,
            enable_message_ordering: true,
            filter: Some("attributes.env = 'prod'".to_string()),
            dead_letter_policy: Some(crate::types::DeadLetterPolicy {
                dead_letter_topic: "projects/test-project/topics/dlq".to_string(),
                max_delivery_attempts: 3,
            }),
            push_config: None,
        };

        let subscription = SubscriberService::config_to_subscription(&config);

        assert_eq!(
            subscription.name,
            "projects/test-project/subscriptions/test-sub"
        );
        assert_eq!(
            subscription.topic,
            "projects/test-project/topics/test-topic"
        );
        assert_eq!(subscription.ack_deadline_seconds, 20);
        assert_eq!(
            subscription.message_retention_duration.unwrap().seconds,
            172800
        );
        assert!(subscription.enable_message_ordering);
        assert_eq!(subscription.filter, "attributes.env = 'prod'");
        assert!(subscription.dead_letter_policy.is_some());
    }

    // ========================================================================
    // CreateSubscription Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_subscription_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic first
        create_test_topic(&publisher, "test-project", "my-topic").await;

        // Create subscription
        let subscription = Subscription {
            name: "projects/test-project/subscriptions/my-sub".to_string(),
            topic: "projects/test-project/topics/my-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 30,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let request = Request::new(subscription.clone());
        let response = service.create_subscription(request).await.unwrap();
        let created_sub = response.into_inner();

        assert_eq!(
            created_sub.name,
            "projects/test-project/subscriptions/my-sub"
        );
        assert_eq!(created_sub.topic, "projects/test-project/topics/my-topic");
        assert_eq!(created_sub.ack_deadline_seconds, 30);
    }

    #[tokio::test]
    async fn test_create_subscription_topic_not_found() {
        let service = create_test_service();

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/my-sub".to_string(),
            topic: "projects/test-project/topics/nonexistent".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let request = Request::new(subscription);
        let result = service.create_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_create_subscription_invalid_name() {
        let service = create_test_service();

        let subscription = Subscription {
            name: "invalid-subscription-name".to_string(),
            topic: "projects/test/topics/topic1".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let request = Request::new(subscription);
        let result = service.create_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_create_subscription_invalid_id() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic first
        create_test_topic(&publisher, "test-project", "topic1").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/123invalid".to_string(), // Starts with number
            topic: "projects/test-project/topics/topic1".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let request = Request::new(subscription);
        let result = service.create_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_create_subscription_with_message_ordering() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic
        create_test_topic(&publisher, "test-project", "ordered-topic").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/ordered-sub".to_string(),
            topic: "projects/test-project/topics/ordered-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: true, // Enable ordering
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };

        let request = Request::new(subscription);
        let response = service.create_subscription(request).await.unwrap();
        let created_sub = response.into_inner();

        assert!(created_sub.enable_message_ordering);
    }

    // ========================================================================
    // GetSubscription Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_subscription_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "topic1").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/get-sub".to_string(),
            topic: "projects/test-project/topics/topic1".to_string(),
            push_config: None,
            ack_deadline_seconds: 15,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Get subscription
        let get_request = GetSubscriptionRequest {
            subscription: "projects/test-project/subscriptions/get-sub".to_string(),
        };

        let request = Request::new(get_request);
        let response = service.get_subscription(request).await.unwrap();
        let retrieved_sub = response.into_inner();

        assert_eq!(
            retrieved_sub.name,
            "projects/test-project/subscriptions/get-sub"
        );
        assert_eq!(retrieved_sub.ack_deadline_seconds, 15);
    }

    #[tokio::test]
    async fn test_get_subscription_not_found() {
        let service = create_test_service();

        let get_request = GetSubscriptionRequest {
            subscription: "projects/test-project/subscriptions/nonexistent".to_string(),
        };

        let request = Request::new(get_request);
        let result = service.get_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_get_subscription_invalid_name() {
        let service = create_test_service();

        let get_request = GetSubscriptionRequest {
            subscription: "invalid-format".to_string(),
        };

        let request = Request::new(get_request);
        let result = service.get_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    // ========================================================================
    // UpdateSubscription Tests
    // ========================================================================

    #[tokio::test]
    async fn test_update_subscription_unimplemented() {
        let service = create_test_service();

        let update_request = UpdateSubscriptionRequest {
            subscription: None,
            update_mask: None,
        };

        let request = Request::new(update_request);
        let result = service.update_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    // ========================================================================
    // ListSubscriptions Tests
    // ========================================================================

    #[tokio::test]
    async fn test_list_subscriptions_empty() {
        let service = create_test_service();

        let list_request = ListSubscriptionsRequest {
            project: "projects/test-project".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_subscriptions(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.subscriptions.len(), 0);
    }

    #[tokio::test]
    async fn test_list_subscriptions_with_data() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic
        create_test_topic(&publisher, "test-project", "list-topic").await;

        // Create two subscriptions
        for i in 1..=2 {
            let subscription = Subscription {
                name: format!("projects/test-project/subscriptions/sub{}", i),
                topic: "projects/test-project/topics/list-topic".to_string(),
                push_config: None,
                ack_deadline_seconds: 10,
                retain_acked_messages: false,
                message_retention_duration: None,
                labels: Default::default(),
                enable_message_ordering: false,
                expiration_policy: None,
                filter: String::new(),
                dead_letter_policy: None,
                retry_policy: None,
                detached: false,
                enable_exactly_once_delivery: false,
                topic_message_retention_duration: None,
                state: State::Active as i32,
            };
            service
                .create_subscription(Request::new(subscription))
                .await
                .unwrap();
        }

        // List subscriptions
        let list_request = ListSubscriptionsRequest {
            project: "projects/test-project".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_subscriptions(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.subscriptions.len(), 2);
    }

    #[tokio::test]
    async fn test_list_subscriptions_filters_by_project() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topics in different projects
        create_test_topic(&publisher, "project-a", "topic-a").await;
        create_test_topic(&publisher, "project-b", "topic-b").await;

        // Create subscriptions in different projects
        let sub_a = Subscription {
            name: "projects/project-a/subscriptions/sub-a".to_string(),
            topic: "projects/project-a/topics/topic-a".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(sub_a))
            .await
            .unwrap();

        let sub_b = Subscription {
            name: "projects/project-b/subscriptions/sub-b".to_string(),
            topic: "projects/project-b/topics/topic-b".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(sub_b))
            .await
            .unwrap();

        // List subscriptions for project-a only
        let list_request = ListSubscriptionsRequest {
            project: "projects/project-a".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let request = Request::new(list_request);
        let response = service.list_subscriptions(request).await.unwrap();
        let list_response = response.into_inner();

        assert_eq!(list_response.subscriptions.len(), 1);
        assert!(list_response.subscriptions[0].name.contains("project-a"));
    }

    // ========================================================================
    // DeleteSubscription Tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_subscription_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "del-topic").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/del-sub".to_string(),
            topic: "projects/test-project/topics/del-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Delete subscription
        let delete_request = DeleteSubscriptionRequest {
            subscription: "projects/test-project/subscriptions/del-sub".to_string(),
        };

        let request = Request::new(delete_request);
        let response = service.delete_subscription(request).await;

        assert!(response.is_ok());

        // Verify subscription is deleted
        let get_request = GetSubscriptionRequest {
            subscription: "projects/test-project/subscriptions/del-sub".to_string(),
        };
        let result = service.get_subscription(Request::new(get_request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_subscription_not_found() {
        let service = create_test_service();

        let delete_request = DeleteSubscriptionRequest {
            subscription: "projects/test-project/subscriptions/nonexistent".to_string(),
        };

        let request = Request::new(delete_request);
        let result = service.delete_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_delete_subscription_invalid_name() {
        let service = create_test_service();

        let delete_request = DeleteSubscriptionRequest {
            subscription: "invalid-format".to_string(),
        };

        let request = Request::new(delete_request);
        let result = service.delete_subscription(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    // ========================================================================
    // Pull Tests
    // ========================================================================

    #[tokio::test]
    async fn test_pull_empty_queue() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend);

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "pull-topic").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/pull-sub".to_string(),
            topic: "projects/test-project/topics/pull-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Pull messages (should be empty)
        let pull_request = PullRequest {
            subscription: "projects/test-project/subscriptions/pull-sub".to_string(),
            return_immediately: true,
            max_messages: 10,
        };

        let request = Request::new(pull_request);
        let response = service.pull(request).await.unwrap();
        let pull_response = response.into_inner();

        assert_eq!(pull_response.received_messages.len(), 0);
    }

    #[tokio::test]
    async fn test_pull_with_messages() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend.clone());

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "pull-topic2").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/pull-sub2".to_string(),
            topic: "projects/test-project/topics/pull-topic2".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Publish a message to topic using Publisher service (which fans out to subscriptions)
        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/pull-topic2".to_string(),
            messages: vec![PubsubMessage {
                data: b"Test message".to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };
        publisher
            .publish(Request::new(publish_request))
            .await
            .unwrap();

        // Pull messages
        let pull_request = PullRequest {
            subscription: "projects/test-project/subscriptions/pull-sub2".to_string(),
            return_immediately: true,
            max_messages: 10,
        };

        let request = Request::new(pull_request);
        let response = service.pull(request).await.unwrap();
        let pull_response = response.into_inner();

        assert_eq!(pull_response.received_messages.len(), 1);
        let received = &pull_response.received_messages[0];
        assert!(!received.ack_id.is_empty());
        assert!(received.message.is_some());

        let msg = received.message.as_ref().unwrap();
        assert_eq!(msg.data, b"Test message");
    }

    #[tokio::test]
    async fn test_pull_subscription_not_found() {
        let service = create_test_service();

        let pull_request = PullRequest {
            subscription: "projects/test-project/subscriptions/nonexistent".to_string(),
            return_immediately: true,
            max_messages: 10,
        };

        let request = Request::new(pull_request);
        let result = service.pull(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_pull_respects_max_messages() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend.clone());

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "max-msg-topic").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/max-msg-sub".to_string(),
            topic: "projects/test-project/topics/max-msg-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Publish 5 messages to topic using Publisher service (which fans out to subscriptions)
        let mut messages = Vec::new();
        for i in 0..5 {
            messages.push(PubsubMessage {
                data: format!("Message {}", i).as_bytes().to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            });
        }

        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/max-msg-topic".to_string(),
            messages,
        };
        publisher
            .publish(Request::new(publish_request))
            .await
            .unwrap();

        // Pull with max_messages=2
        let pull_request = PullRequest {
            subscription: "projects/test-project/subscriptions/max-msg-sub".to_string(),
            return_immediately: true,
            max_messages: 2,
        };

        let request = Request::new(pull_request);
        let response = service.pull(request).await.unwrap();
        let pull_response = response.into_inner();

        assert_eq!(pull_response.received_messages.len(), 2); // Should respect max_messages
    }

    // ========================================================================
    // Acknowledge Tests
    // ========================================================================

    #[tokio::test]
    async fn test_acknowledge_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend.clone());

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "ack-topic").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/ack-sub".to_string(),
            topic: "projects/test-project/topics/ack-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Publish a message to topic using Publisher service (which fans out to subscriptions)
        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/ack-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"Ack test".to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };
        publisher
            .publish(Request::new(publish_request))
            .await
            .unwrap();

        // Pull message
        let pull_request = PullRequest {
            subscription: "projects/test-project/subscriptions/ack-sub".to_string(),
            return_immediately: true,
            max_messages: 1,
        };
        let response = service.pull(Request::new(pull_request)).await.unwrap();
        let ack_id = response.into_inner().received_messages[0].ack_id.clone();

        // Acknowledge the message
        let ack_request = AcknowledgeRequest {
            subscription: "projects/test-project/subscriptions/ack-sub".to_string(),
            ack_ids: vec![ack_id],
        };

        let request = Request::new(ack_request);
        let response = service.acknowledge(request).await;

        assert!(response.is_ok());

        // Pull again - should be empty now
        let pull_request2 = PullRequest {
            subscription: "projects/test-project/subscriptions/ack-sub".to_string(),
            return_immediately: true,
            max_messages: 1,
        };
        let response2 = service.pull(Request::new(pull_request2)).await.unwrap();
        assert_eq!(response2.into_inner().received_messages.len(), 0);
    }

    #[tokio::test]
    async fn test_acknowledge_subscription_not_found() {
        let service = create_test_service();

        let ack_request = AcknowledgeRequest {
            subscription: "projects/test-project/subscriptions/nonexistent".to_string(),
            ack_ids: vec!["some-ack-id".to_string()],
        };

        let request = Request::new(ack_request);
        let result = service.acknowledge(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    // ========================================================================
    // ModifyAckDeadline Tests
    // ========================================================================

    #[tokio::test]
    async fn test_modify_ack_deadline_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let publisher = create_test_publisher(backend.clone());
        let service = SubscriberService::new(backend.clone());

        // Create topic and subscription
        create_test_topic(&publisher, "test-project", "modify-topic").await;

        let subscription = Subscription {
            name: "projects/test-project/subscriptions/modify-sub".to_string(),
            topic: "projects/test-project/topics/modify-topic".to_string(),
            push_config: None,
            ack_deadline_seconds: 10,
            retain_acked_messages: false,
            message_retention_duration: None,
            labels: Default::default(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: None,
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: State::Active as i32,
        };
        service
            .create_subscription(Request::new(subscription))
            .await
            .unwrap();

        // Publish a message to topic using Publisher service (which fans out to subscriptions)
        let publish_request = PublishRequest {
            topic: "projects/test-project/topics/modify-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"Modify test".to_vec(),
                attributes: Default::default(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };
        publisher
            .publish(Request::new(publish_request))
            .await
            .unwrap();

        // Pull message
        let pull_request = PullRequest {
            subscription: "projects/test-project/subscriptions/modify-sub".to_string(),
            return_immediately: true,
            max_messages: 1,
        };
        let response = service.pull(Request::new(pull_request)).await.unwrap();
        let ack_id = response.into_inner().received_messages[0].ack_id.clone();

        // Modify ack deadline
        let modify_request = ModifyAckDeadlineRequest {
            subscription: "projects/test-project/subscriptions/modify-sub".to_string(),
            ack_ids: vec![ack_id],
            ack_deadline_seconds: 60,
        };

        let request = Request::new(modify_request);
        let response = service.modify_ack_deadline(request).await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_modify_ack_deadline_subscription_not_found() {
        let service = create_test_service();

        let modify_request = ModifyAckDeadlineRequest {
            subscription: "projects/test-project/subscriptions/nonexistent".to_string(),
            ack_ids: vec!["some-ack-id".to_string()],
            ack_deadline_seconds: 60,
        };

        let request = Request::new(modify_request);
        let result = service.modify_ack_deadline(request).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    // ========================================================================
    // Unimplemented Method Tests
    // ========================================================================

    #[tokio::test]
    async fn test_modify_push_config_unimplemented() {
        let service = create_test_service();

        let request = ModifyPushConfigRequest {
            subscription: "projects/test/subscriptions/sub1".to_string(),
            push_config: None,
        };

        let result = service.modify_push_config(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_get_snapshot_unimplemented() {
        let service = create_test_service();

        let request = GetSnapshotRequest {
            snapshot: "projects/test/snapshots/snap1".to_string(),
        };

        let result = service.get_snapshot(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_list_snapshots_unimplemented() {
        let service = create_test_service();

        let request = ListSnapshotsRequest {
            project: "projects/test".to_string(),
            page_size: 0,
            page_token: String::new(),
        };

        let result = service.list_snapshots(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_create_snapshot_unimplemented() {
        let service = create_test_service();

        let request = CreateSnapshotRequest {
            name: "projects/test/snapshots/snap1".to_string(),
            subscription: "projects/test/subscriptions/sub1".to_string(),
            labels: Default::default(),
        };

        let result = service.create_snapshot(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_update_snapshot_unimplemented() {
        let service = create_test_service();

        let request = UpdateSnapshotRequest {
            snapshot: None,
            update_mask: None,
        };

        let result = service.update_snapshot(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_delete_snapshot_unimplemented() {
        let service = create_test_service();

        let request = DeleteSnapshotRequest {
            snapshot: "projects/test/snapshots/snap1".to_string(),
        };

        let result = service.delete_snapshot(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_seek_unimplemented() {
        let service = create_test_service();

        let request = SeekRequest {
            subscription: "projects/test/subscriptions/sub1".to_string(),
            target: None,
        };

        let result = service.seek(Request::new(request)).await;

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    // ========================================================================
    // Push Config Tests
    // ========================================================================

    #[test]
    fn test_subscription_to_config_with_push() {
        let sub = Subscription {
            name: "projects/test/subscriptions/test-sub".to_string(),
            topic: "projects/test/topics/test-topic".to_string(),
            push_config: Some(PushConfig {
                push_endpoint: "https://example.com/webhook".to_string(),
                attributes: std::collections::HashMap::new(),
                authentication_method: None,
            }),
            ack_deadline_seconds: 30,
            retain_acked_messages: false,
            message_retention_duration: Some(prost_types::Duration {
                seconds: 604800,
                nanos: 0,
            }),
            labels: std::collections::HashMap::new(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: Some(RetryPolicy {
                minimum_backoff: Some(prost_types::Duration {
                    seconds: 10,
                    nanos: 0,
                }),
                maximum_backoff: Some(prost_types::Duration {
                    seconds: 600,
                    nanos: 0,
                }),
            }),
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: 0,
        };

        let config = SubscriberService::subscription_to_config(&sub).unwrap();

        assert!(config.push_config.is_some());
        let push_config = config.push_config.unwrap();
        assert_eq!(push_config.endpoint, "https://example.com/webhook");
        assert!(push_config.retry_policy.is_some());

        let retry_policy = push_config.retry_policy.unwrap();
        assert_eq!(retry_policy.min_backoff_seconds, 10);
        assert_eq!(retry_policy.max_backoff_seconds, 600);
    }
}
