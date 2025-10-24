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
use crate::pubsub::types::{validate_subscription_id, ResourceName};
use crate::storage::StorageBackend;
use crate::types::{QueueConfig, QueueType, ReceiveOptions, SubscriptionConfig};
use base64::Engine;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
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

        Subscription {
            name: config.name.clone(),
            topic,
            push_config: None,
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
            dead_letter_policy: config.dead_letter_policy.as_ref().map(|dlp| {
                DeadLetterPolicy {
                    dead_letter_topic: dlp.dead_letter_topic.clone(),
                    max_delivery_attempts: dlp.max_delivery_attempts as i32,
                }
            }),
            retry_policy: None,
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
            message_retention_period: 604800, // 7 days
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
        let resource_name = ResourceName::parse(&req.subscription).map_err(|e| {
            Status::invalid_argument(format!("Invalid subscription name: {}", e))
        })?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get from backend
        let config = self
            .backend
            .get_subscription(&sub_id)
            .await
            .map_err(|_| Status::not_found(format!("Subscription not found: {}", req.subscription)))?;

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
        Err(Status::unimplemented("UpdateSubscription not yet implemented"))
    }

    /// Lists matching subscriptions.
    async fn list_subscriptions(
        &self,
        request: Request<ListSubscriptionsRequest>,
    ) -> std::result::Result<Response<ListSubscriptionsResponse>, Status> {
        let req = request.into_inner();
        debug!("ListSubscriptions: project={}", req.project);

        // Extract project ID from "projects/{project}" format
        let project_id = req.project
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
        let resource_name = ResourceName::parse(&req.subscription).map_err(|e| {
            Status::invalid_argument(format!("Invalid subscription name: {}", e))
        })?;

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
            .map_err(|_| Status::not_found(format!("Subscription not found: {}", req.subscription)))?;

        Ok(Response::new(()))
    }

    /// Modifies the ack deadline for a specific message.
    async fn modify_ack_deadline(
        &self,
        request: Request<ModifyAckDeadlineRequest>,
    ) -> std::result::Result<Response<()>, Status> {
        let req = request.into_inner();
        debug!(
            "ModifyAckDeadline: {} messages",
            req.ack_ids.len()
        );

        // Parse subscription name
        let resource_name = ResourceName::parse(&req.subscription).map_err(|e| {
            Status::invalid_argument(format!("Invalid subscription name: {}", e))
        })?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get subscription config to find topic
        let config = self
            .backend
            .get_subscription(&sub_id)
            .await
            .map_err(|_| Status::not_found(format!("Subscription not found: {}", req.subscription)))?;

        // Modify visibility for each ack_id (receipt handle)
        for ack_id in &req.ack_ids {
            self.backend
                .change_visibility(&config.topic_id, ack_id, req.ack_deadline_seconds as u32)
                .await
                .map_err(|e| {
                    Status::internal(format!("Failed to modify ack deadline: {}", e))
                })?;
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
        let resource_name = ResourceName::parse(&req.subscription).map_err(|e| {
            Status::invalid_argument(format!("Invalid subscription name: {}", e))
        })?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get subscription config to find topic
        let config = self
            .backend
            .get_subscription(&sub_id)
            .await
            .map_err(|_| Status::not_found(format!("Subscription not found: {}", req.subscription)))?;

        // Delete each message using ack_id (receipt handle)
        for ack_id in &req.ack_ids {
            self.backend
                .delete_message(&config.topic_id, ack_id)
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
        let resource_name = ResourceName::parse(&req.subscription).map_err(|e| {
            Status::invalid_argument(format!("Invalid subscription name: {}", e))
        })?;

        let sub_id = format!(
            "{}:{}",
            resource_name.project(),
            resource_name.resource_id()
        );

        // Get subscription config to find topic
        let config = self
            .backend
            .get_subscription(&sub_id)
            .await
            .map_err(|_| Status::not_found(format!("Subscription not found: {}", req.subscription)))?;

        // Receive messages from topic
        let options = ReceiveOptions {
            max_messages: req.max_messages as u32,
            visibility_timeout: Some(config.ack_deadline_seconds),
            wait_time_seconds: 0,
            attribute_names: vec![],
            message_attribute_names: vec![],
        };

        let messages = self
            .backend
            .receive_messages(&config.topic_id, options)
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

        let response = PullResponse {
            received_messages,
        };

        Ok(Response::new(response))
    }

    /// Server streaming response type for the StreamingPull method.
    type StreamingPullStream =
        ReceiverStream<std::result::Result<StreamingPullResponse, Status>>;

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
        Err(Status::unimplemented("ModifyPushConfig not yet implemented"))
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

