//! REST API implementation for Google Cloud Pub/Sub.
//!
//! This module implements the HTTP/REST API for Pub/Sub, providing an alternative
//! to the gRPC API. The REST API uses JSON for requests and responses.
//!
//! ## API Endpoints
//!
//! ### Topics
//! - `PUT /v1/projects/{project}/topics/{topic}` - Create a topic
//! - `GET /v1/projects/{project}/topics/{topic}` - Get topic details
//! - `DELETE /v1/projects/{project}/topics/{topic}` - Delete a topic
//! - `GET /v1/projects/{project}/topics` - List topics
//! - `POST /v1/projects/{project}/topics/{topic}:publish` - Publish messages
//!
//! ### Subscriptions
//! - `PUT /v1/projects/{project}/subscriptions/{subscription}` - Create a subscription
//! - `GET /v1/projects/{project}/subscriptions/{subscription}` - Get subscription details
//! - `DELETE /v1/projects/{project}/subscriptions/{subscription}` - Delete a subscription
//! - `GET /v1/projects/{project}/subscriptions` - List subscriptions
//! - `POST /v1/projects/{project}/subscriptions/{subscription}:pull` - Pull messages
//! - `POST /v1/projects/{project}/subscriptions/{subscription}:acknowledge` - Acknowledge messages
//! - `POST /v1/projects/{project}/subscriptions/{subscription}:modifyAckDeadline` - Modify ack deadline

use crate::error::Result;
use crate::storage::StorageBackend;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Configuration for the Pub/Sub REST server.
#[derive(Debug, Clone)]
pub struct RestServerConfig {
    /// The address to bind to (e.g., "127.0.0.1:8086")
    pub bind_address: String,
}

/// Shared application state for REST handlers.
#[derive(Clone)]
pub struct RestState {
    backend: Arc<dyn StorageBackend>,
}

impl RestState {
    /// Create a new REST state.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Convert REST Topic to internal QueueConfig.
    fn topic_to_queue_config(topic: &Topic, project: &str, topic_name: &str) -> Result<crate::types::QueueConfig> {
        let topic_id = format!("{}:{}", project, topic_name);
        let full_name = format!("projects/{}/topics/{}", project, topic_name);

        Ok(crate::types::QueueConfig {
            id: topic_id,
            name: full_name,
            queue_type: crate::types::QueueType::PubSubTopic,
            visibility_timeout: 60,
            message_retention_period: topic
                .message_retention_duration
                .as_ref()
                .and_then(|d| d.strip_suffix('s').and_then(|s| s.parse().ok()))
                .unwrap_or(604800), // 7 days default
            max_message_size: 10 * 1024 * 1024, // 10 MB for Pub/Sub
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: topic.labels.clone().unwrap_or_default(),
            redrive_allow_policy: None,
        })
    }

    /// Convert internal QueueConfig to REST Topic.
    fn queue_config_to_topic(config: &crate::types::QueueConfig) -> Topic {
        Topic {
            name: Some(config.name.clone()),
            labels: if config.tags.is_empty() {
                None
            } else {
                Some(config.tags.clone())
            },
            message_retention_duration: Some(format!("{}s", config.message_retention_period)),
        }
    }

    /// Convert REST Subscription to internal SubscriptionConfig.
    fn subscription_to_config(subscription: &Subscription, project: &str, sub_name: &str) -> Result<crate::types::SubscriptionConfig> {
        let sub_id = format!("{}:{}", project, sub_name);
        let full_name = format!("projects/{}/subscriptions/{}", project, sub_name);

        // Parse topic name to extract project and topic
        let topic_parts: Vec<&str> = subscription.topic.split('/').collect();
        let topic_id = if topic_parts.len() == 4 && topic_parts[0] == "projects" && topic_parts[2] == "topics" {
            format!("{}:{}", topic_parts[1], topic_parts[3])
        } else {
            return Err(crate::error::Error::Validation(
                crate::error::ValidationError::InvalidParameter {
                    name: "topic".to_string(),
                    reason: format!("Invalid topic name format: {}", subscription.topic),
                }
            ));
        };

        Ok(crate::types::SubscriptionConfig {
            id: sub_id,
            name: full_name,
            topic_id,
            ack_deadline_seconds: subscription.ack_deadline_seconds.unwrap_or(10) as u32,
            message_retention_duration: subscription
                .message_retention_duration
                .as_ref()
                .and_then(|d| d.strip_suffix('s').and_then(|s| s.parse().ok()))
                .unwrap_or(604800), // 7 days default
            enable_message_ordering: subscription.enable_message_ordering.unwrap_or(false),
            filter: subscription.filter.clone(),
            dead_letter_policy: subscription.dead_letter_policy.as_ref().map(|dlp| {
                crate::types::DeadLetterPolicy {
                    dead_letter_topic: dlp.dead_letter_topic.clone(),
                    max_delivery_attempts: dlp.max_delivery_attempts.unwrap_or(5) as u32,
                }
            }),
        })
    }

    /// Convert internal SubscriptionConfig to REST Subscription.
    fn config_to_subscription(config: &crate::types::SubscriptionConfig) -> Subscription {
        // Parse subscription_id to get project and subscription name
        let parts: Vec<&str> = config.id.split(':').collect();
        let (project, _sub_name) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            ("default", config.id.as_str())
        };

        // Parse topic_id to get topic
        let topic_parts: Vec<&str> = config.topic_id.split(':').collect();
        let (topic_project, topic_name) = if topic_parts.len() == 2 {
            (topic_parts[0], topic_parts[1])
        } else {
            (project, config.topic_id.as_str())
        };

        let topic = format!("projects/{}/topics/{}", topic_project, topic_name);

        Subscription {
            name: Some(config.name.clone()),
            topic,
            push_config: None,
            ack_deadline_seconds: Some(config.ack_deadline_seconds as i32),
            retain_acked_messages: Some(false),
            message_retention_duration: Some(format!("{}s", config.message_retention_duration)),
            labels: None,
            enable_message_ordering: Some(config.enable_message_ordering),
            expiration_policy: None,
            filter: config.filter.clone(),
            dead_letter_policy: config.dead_letter_policy.as_ref().map(|dlp| {
                DeadLetterPolicy {
                    dead_letter_topic: dlp.dead_letter_topic.clone(),
                    max_delivery_attempts: Some(dlp.max_delivery_attempts as i32),
                }
            }),
        }
    }

    /// Convert REST PubsubMessage to internal Message.
    fn pubsub_message_to_message(msg: &PubsubMessage, topic_id: &str) -> crate::types::Message {
        use crate::types::{Message, MessageAttributes, MessageAttributeValue, MessageId};
        use chrono::Utc;

        let mut attributes = MessageAttributes::new();
        if let Some(attrs) = &msg.attributes {
            for (key, value) in attrs {
                attributes.insert(
                    key.clone(),
                    MessageAttributeValue {
                        data_type: "String".to_string(),
                        string_value: Some(value.clone()),
                        binary_value: None,
                    },
                );
            }
        }

        // Message data is already Vec<u8> (deserialized from base64)
        // Encode it to base64 string for storage
        use ::base64::Engine as _;
        let body = ::base64::engine::general_purpose::STANDARD.encode(&msg.data);

        Message {
            id: MessageId::new(),
            body,
            attributes,
            queue_id: topic_id.to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: msg.ordering_key.clone(),
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        }
    }

    /// Convert internal Message to REST PubsubMessage.
    fn message_to_pubsub_message(msg: &crate::types::Message) -> PubsubMessage {
        // Decode base64 body back to bytes
        use ::base64::Engine as _;
        let data = ::base64::engine::general_purpose::STANDARD
            .decode(&msg.body)
            .unwrap_or_default();

        let mut attributes = HashMap::new();
        for (key, value) in &msg.attributes {
            if let Some(string_val) = &value.string_value {
                attributes.insert(key.clone(), string_val.clone());
            }
        }

        PubsubMessage {
            data,
            attributes: if attributes.is_empty() { None } else { Some(attributes) },
            message_id: Some(msg.id.0.clone()),
            publish_time: Some(msg.sent_timestamp.to_rfc3339()),
            ordering_key: msg.message_group_id.clone(),
        }
    }
}

/// Error response format for Google Cloud APIs.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error details.
    pub error: ErrorDetail,
}

/// Error detail information.
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    /// HTTP status code.
    pub code: u16,
    /// Error message.
    pub message: String,
    /// Error status string.
    pub status: String,
}

impl ErrorResponse {
    fn new(code: StatusCode, message: impl Into<String>) -> Self {
        let status = match code {
            StatusCode::BAD_REQUEST => "INVALID_ARGUMENT",
            StatusCode::NOT_FOUND => "NOT_FOUND",
            StatusCode::CONFLICT => "ALREADY_EXISTS",
            StatusCode::INTERNAL_SERVER_ERROR => "INTERNAL",
            StatusCode::UNPROCESSABLE_ENTITY => "FAILED_PRECONDITION",
            _ => "UNKNOWN",
        };

        Self {
            error: ErrorDetail {
                code: code.as_u16(),
                message: message.into(),
                status: status.to_string(),
            },
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.error.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self)).into_response()
    }
}

// ============================================================================
// Topic Request/Response Types
// ============================================================================

/// Topic resource representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    /// Topic name (projects/{project}/topics/{topic}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Labels for the topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    /// Message retention duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_retention_duration: Option<String>, // Duration as string (e.g., "604800s")
}

/// Query parameters for listing topics.
#[derive(Debug, Deserialize)]
pub struct ListTopicsQuery {
    #[serde(rename = "pageSize")]
    #[allow(dead_code)] // Will be used for pagination
    page_size: Option<i32>,
    #[serde(rename = "pageToken")]
    #[allow(dead_code)] // Will be used for pagination
    page_token: Option<String>,
}

/// Response for listing topics.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTopicsResponse {
    /// List of topics.
    pub topics: Vec<Topic>,
    /// Token for retrieving the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// Request for publishing messages.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    /// Messages to publish.
    pub messages: Vec<PubsubMessage>,
}

/// A Pub/Sub message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubsubMessage {
    /// Message data (base64-encoded).
    #[serde(with = "base64")]
    pub data: Vec<u8>,
    /// Message attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, String>>,
    /// Message ID (set by server).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Publish timestamp (set by server).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publish_time: Option<String>,
    /// Ordering key for ordered delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordering_key: Option<String>,
}

// Base64 encoding/decoding helpers
mod base64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

/// Response for publishing messages.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResponse {
    /// Message IDs assigned by the server.
    pub message_ids: Vec<String>,
}

// ============================================================================
// Subscription Request/Response Types
// ============================================================================

/// A Pub/Sub subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Subscription name (projects/{project}/subscriptions/{subscription}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Topic name (projects/{project}/topics/{topic}).
    pub topic: String,
    /// Push configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_config: Option<PushConfig>,
    /// Acknowledgment deadline in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ack_deadline_seconds: Option<i32>,
    /// Whether to retain acknowledged messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_acked_messages: Option<bool>,
    /// Message retention duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_retention_duration: Option<String>,
    /// Labels for the subscription.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    /// Whether message ordering is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_message_ordering: Option<bool>,
    /// Expiration policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_policy: Option<ExpirationPolicy>,
    /// Message filter expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Dead letter policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dead_letter_policy: Option<DeadLetterPolicy>,
}

/// Push configuration for a subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushConfig {
    /// HTTP endpoint for push delivery.
    pub push_endpoint: String,
    /// Attributes for push delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<HashMap<String, String>>,
}

/// Expiration policy for a subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpirationPolicy {
    /// Time-to-live duration.
    pub ttl: String, // Duration as string
}

/// Dead letter policy for a subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterPolicy {
    /// Dead letter topic name.
    pub dead_letter_topic: String,
    /// Maximum delivery attempts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_delivery_attempts: Option<i32>,
}

/// Query parameters for listing subscriptions.
#[derive(Debug, Deserialize)]
pub struct ListSubscriptionsQuery {
    #[serde(rename = "pageSize")]
    #[allow(dead_code)] // Will be used for pagination
    page_size: Option<i32>,
    #[serde(rename = "pageToken")]
    #[allow(dead_code)] // Will be used for pagination
    page_token: Option<String>,
}

/// Response for listing subscriptions.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSubscriptionsResponse {
    /// List of subscriptions.
    pub subscriptions: Vec<Subscription>,
    /// Token for retrieving the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

/// Request for pulling messages.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    /// Maximum number of messages to return.
    #[serde(default = "default_max_messages")]
    pub max_messages: i32,
    /// Whether to return immediately if no messages are available.
    #[serde(default)]
    pub return_immediately: bool,
}

fn default_max_messages() -> i32 {
    1
}

/// Response for pulling messages.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    /// Received messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_messages: Option<Vec<ReceivedMessage>>,
}

/// A received message.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedMessage {
    /// Acknowledgment ID.
    pub ack_id: String,
    /// The message.
    pub message: PubsubMessage,
    /// Delivery attempt counter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_attempt: Option<i32>,
}

/// Request for acknowledging messages.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcknowledgeRequest {
    /// Acknowledgment IDs.
    pub ack_ids: Vec<String>,
}

/// Request for modifying acknowledgment deadline.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyAckDeadlineRequest {
    /// Acknowledgment IDs.
    pub ack_ids: Vec<String>,
    /// New acknowledgment deadline in seconds.
    pub ack_deadline_seconds: i32,
}

// ============================================================================
// REST API Handlers
// ============================================================================

/// Create the REST API router.
pub fn create_router(state: RestState) -> Router {
    Router::new()
        // Topic endpoints
        .route(
            "/v1/projects/{project}/topics/{topic}",
            put(create_topic).get(get_topic).delete(delete_topic).post(handle_topic_action),
        )
        .route("/v1/projects/{project}/topics", get(list_topics))
        // Subscription endpoints
        .route(
            "/v1/projects/{project}/subscriptions/{subscription}",
            put(create_subscription)
                .get(get_subscription)
                .delete(delete_subscription)
                .post(handle_subscription_action),
        )
        .route(
            "/v1/projects/{project}/subscriptions",
            get(list_subscriptions),
        )
        .with_state(state)
}

/// Start the Pub/Sub REST server.
pub async fn start_rest_server(
    config: RestServerConfig,
    backend: Arc<dyn StorageBackend>,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    info!("Starting Pub/Sub REST server on {}", config.bind_address);

    let state = RestState::new(backend);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
            info!("Pub/Sub REST server shutting down");
        })
        .await?;

    Ok(())
}

// ============================================================================
// Topic Handlers
// ============================================================================

/// Handle topic actions (like :publish)
async fn handle_topic_action(
    Path((project, topic_action)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(payload): Json<serde_json::Value>,
) -> std::result::Result<Json<serde_json::Value>, ErrorResponse> {
    // Parse topic:action format
    if let Some((topic, action)) = topic_action.rsplit_once(':') {
        match action {
            "publish" => {
                let publish_req: PublishRequest = serde_json::from_value(payload)
                    .map_err(|e| ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;
                let response = publish(Path((project, topic.to_string())), State(state), Json(publish_req)).await?;
                let value = serde_json::to_value(response.0)
                    .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize response: {}", e)))?;
                Ok(Json(value))
            }
            _ => Err(ErrorResponse::new(
                StatusCode::NOT_FOUND,
                format!("Unknown topic action: {}", action),
            )),
        }
    } else {
        Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Invalid topic action format (expected topic:action)",
        ))
    }
}

async fn create_topic(
    Path((project, topic)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(mut payload): Json<Topic>,
) -> std::result::Result<Json<Topic>, ErrorResponse> {
    info!("REST: CreateTopic {}/{}", project, topic);

    // Validate topic ID
    use crate::pubsub::types::validate_topic_id;
    validate_topic_id(&topic).map_err(|e| {
        ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid topic ID: {}", e))
    })?;

    // Set the name from path if not provided
    if payload.name.is_none() {
        payload.name = Some(format!("projects/{}/topics/{}", project, topic));
    }

    // Convert to queue config
    let queue_config = RestState::topic_to_queue_config(&payload, &project, &topic)
        .map_err(|e| ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid topic: {}", e)))?;

    // Create in backend
    let created_config = state
        .backend
        .create_queue(queue_config)
        .await
        .map_err(|e| match e {
            crate::error::Error::QueueAlreadyExists(_) => {
                ErrorResponse::new(StatusCode::CONFLICT, format!("Topic already exists: {}/{}", project, topic))
            }
            _ => ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create topic: {}", e)),
        })?;

    let response_topic = RestState::queue_config_to_topic(&created_config);
    Ok(Json(response_topic))
}

async fn get_topic(
    Path((project, topic)): Path<(String, String)>,
    State(state): State<RestState>,
) -> std::result::Result<Json<Topic>, ErrorResponse> {
    debug!("REST: GetTopic {}/{}", project, topic);

    let topic_id = format!("{}:{}", project, topic);

    // Get from backend
    let config = state
        .backend
        .get_queue(&topic_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Topic not found: {}/{}", project, topic)))?;

    let response_topic = RestState::queue_config_to_topic(&config);
    Ok(Json(response_topic))
}

async fn delete_topic(
    Path((project, topic)): Path<(String, String)>,
    State(state): State<RestState>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    info!("REST: DeleteTopic {}/{}", project, topic);

    let topic_id = format!("{}:{}", project, topic);

    // Delete from backend
    state
        .backend
        .delete_queue(&topic_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Topic not found: {}/{}", project, topic)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_topics(
    Path(project): Path<String>,
    Query(_query): Query<ListTopicsQuery>,
    State(state): State<RestState>,
) -> std::result::Result<Json<ListTopicsResponse>, ErrorResponse> {
    debug!("REST: ListTopics {}", project);

    // List all queues and filter for Pub/Sub topics
    let configs = state
        .backend
        .list_queues(None)
        .await
        .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list topics: {}", e)))?;

    let topics: Vec<Topic> = configs
        .iter()
        .filter(|c| c.queue_type == crate::types::QueueType::PubSubTopic)
        .filter(|c| {
            // Filter by project
            let parts: Vec<&str> = c.id.split(':').collect();
            parts.len() == 2 && parts[0] == project
        })
        .map(RestState::queue_config_to_topic)
        .collect();

    let response = ListTopicsResponse {
        topics,
        next_page_token: None, // TODO: Implement pagination
    };

    Ok(Json(response))
}

async fn publish(
    Path((project, topic)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(payload): Json<PublishRequest>,
) -> std::result::Result<Json<PublishResponse>, ErrorResponse> {
    debug!("REST: Publish {} messages to {}/{}", payload.messages.len(), project, topic);

    let topic_id = format!("{}:{}", project, topic);

    // Verify topic exists
    state
        .backend
        .get_queue(&topic_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Topic not found: {}/{}", project, topic)))?;

    // Convert messages
    let messages: Vec<crate::types::Message> = payload
        .messages
        .iter()
        .map(|msg| RestState::pubsub_message_to_message(msg, &topic_id))
        .collect();

    // Publish messages
    let published = state
        .backend
        .send_messages(&topic_id, messages)
        .await
        .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to publish messages: {}", e)))?;

    // Build response with message IDs
    let message_ids: Vec<String> = published.iter().map(|m| m.id.0.clone()).collect();

    let response = PublishResponse { message_ids };
    Ok(Json(response))
}

// ============================================================================
// Subscription Handlers
// ============================================================================

async fn create_subscription(
    Path((project, subscription)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(mut payload): Json<Subscription>,
) -> std::result::Result<Json<Subscription>, ErrorResponse> {
    info!("REST: CreateSubscription {}/{}", project, subscription);

    // Validate subscription ID
    use crate::pubsub::types::validate_subscription_id;
    validate_subscription_id(&subscription).map_err(|e| {
        ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid subscription ID: {}", e))
    })?;

    // Set the name from path if not provided
    if payload.name.is_none() {
        payload.name = Some(format!("projects/{}/subscriptions/{}", project, subscription));
    }

    // Parse topic name to extract topic_id and verify topic exists
    let topic_parts: Vec<&str> = payload.topic.split('/').collect();
    if topic_parts.len() != 4 || topic_parts[0] != "projects" || topic_parts[2] != "topics" {
        return Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid topic name: {}", payload.topic),
        ));
    }
    let topic_id = format!("{}:{}", topic_parts[1], topic_parts[3]);

    // Verify topic exists
    state
        .backend
        .get_queue(&topic_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Topic not found: {}", payload.topic)))?;

    // Convert to subscription config
    let config = RestState::subscription_to_config(&payload, &project, &subscription)
        .map_err(|e| ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid subscription: {}", e)))?;

    // Create in backend
    let created_config = state
        .backend
        .create_subscription(config)
        .await
        .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create subscription: {}", e)))?;

    let response_sub = RestState::config_to_subscription(&created_config);
    Ok(Json(response_sub))
}

async fn get_subscription(
    Path((project, subscription)): Path<(String, String)>,
    State(state): State<RestState>,
) -> std::result::Result<Json<Subscription>, ErrorResponse> {
    debug!("REST: GetSubscription {}/{}", project, subscription);

    let sub_id = format!("{}:{}", project, subscription);

    // Get from backend
    let config = state
        .backend
        .get_subscription(&sub_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Subscription not found: {}/{}", project, subscription)))?;

    let response_sub = RestState::config_to_subscription(&config);
    Ok(Json(response_sub))
}

async fn delete_subscription(
    Path((project, subscription)): Path<(String, String)>,
    State(state): State<RestState>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    info!("REST: DeleteSubscription {}/{}", project, subscription);

    let sub_id = format!("{}:{}", project, subscription);

    // Delete from backend
    state
        .backend
        .delete_subscription(&sub_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Subscription not found: {}/{}", project, subscription)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn list_subscriptions(
    Path(project): Path<String>,
    Query(_query): Query<ListSubscriptionsQuery>,
    State(state): State<RestState>,
) -> std::result::Result<Json<ListSubscriptionsResponse>, ErrorResponse> {
    debug!("REST: ListSubscriptions {}", project);

    // List all subscriptions
    let configs = state
        .backend
        .list_subscriptions()
        .await
        .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list subscriptions: {}", e)))?;

    let subscriptions: Vec<Subscription> = configs
        .iter()
        .filter(|c| {
            // Filter by project
            let parts: Vec<&str> = c.id.split(':').collect();
            parts.len() == 2 && parts[0] == project
        })
        .map(RestState::config_to_subscription)
        .collect();

    let response = ListSubscriptionsResponse {
        subscriptions,
        next_page_token: None, // TODO: Implement pagination
    };

    Ok(Json(response))
}

/// Handle subscription actions (like :pull, :acknowledge, :modifyAckDeadline)
async fn handle_subscription_action(
    Path((project, subscription_action)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(payload): Json<serde_json::Value>,
) -> std::result::Result<impl IntoResponse, ErrorResponse> {
    // Parse subscription:action format
    if let Some((subscription, action)) = subscription_action.rsplit_once(':') {
        match action {
            "pull" => {
                let pull_req: PullRequest = serde_json::from_value(payload)
                    .map_err(|e| ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;
                let response = pull(Path((project, subscription.to_string())), State(state), Json(pull_req)).await?;
                Ok((StatusCode::OK, Json(response.0)).into_response())
            }
            "acknowledge" => {
                let ack_req: AcknowledgeRequest = serde_json::from_value(payload)
                    .map_err(|e| ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;
                let status = acknowledge(Path((project, subscription.to_string())), State(state), Json(ack_req)).await?;
                Ok((status, Json(serde_json::json!({}))).into_response())
            }
            "modifyAckDeadline" => {
                let modify_req: ModifyAckDeadlineRequest = serde_json::from_value(payload)
                    .map_err(|e| ErrorResponse::new(StatusCode::BAD_REQUEST, format!("Invalid request: {}", e)))?;
                let status = modify_ack_deadline(Path((project, subscription.to_string())), State(state), Json(modify_req)).await?;
                Ok((status, Json(serde_json::json!({}))).into_response())
            }
            _ => Err(ErrorResponse::new(
                StatusCode::NOT_FOUND,
                format!("Unknown subscription action: {}", action),
            )),
        }
    } else {
        Err(ErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "Invalid subscription action format (expected subscription:action)",
        ))
    }
}

async fn pull(
    Path((project, subscription)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(payload): Json<PullRequest>,
) -> std::result::Result<Json<PullResponse>, ErrorResponse> {
    debug!("REST: Pull {}/{}, max_messages={}", project, subscription, payload.max_messages);

    let sub_id = format!("{}:{}", project, subscription);

    // Get subscription config to find topic
    let config = state
        .backend
        .get_subscription(&sub_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Subscription not found: {}/{}", project, subscription)))?;

    // Receive messages from topic
    let options = crate::types::ReceiveOptions {
        max_messages: payload.max_messages as u32,
        visibility_timeout: Some(config.ack_deadline_seconds),
        wait_time_seconds: 0,
        attribute_names: vec![],
        message_attribute_names: vec![],
    };

    let messages = state
        .backend
        .receive_messages(&config.topic_id, options)
        .await
        .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to pull messages: {}", e)))?;

    // Convert to REST ReceivedMessage
    let received_messages: Vec<ReceivedMessage> = messages
        .iter()
        .map(|rm| {
            let pubsub_message = RestState::message_to_pubsub_message(&rm.message);

            ReceivedMessage {
                ack_id: rm.receipt_handle.clone(),
                message: pubsub_message,
                delivery_attempt: Some(rm.message.receive_count as i32),
            }
        })
        .collect();

    let response = PullResponse {
        received_messages: if received_messages.is_empty() {
            None
        } else {
            Some(received_messages)
        },
    };

    Ok(Json(response))
}

async fn acknowledge(
    Path((project, subscription)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(payload): Json<AcknowledgeRequest>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    debug!("REST: Acknowledge {}/{}, {} messages", project, subscription, payload.ack_ids.len());

    let sub_id = format!("{}:{}", project, subscription);

    // Get subscription config to find topic
    let config = state
        .backend
        .get_subscription(&sub_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Subscription not found: {}/{}", project, subscription)))?;

    // Delete each message using ack_id (receipt handle)
    for ack_id in &payload.ack_ids {
        state
            .backend
            .delete_message(&config.topic_id, ack_id)
            .await
            .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to acknowledge message: {}", e)))?;
    }

    Ok(StatusCode::OK)
}

async fn modify_ack_deadline(
    Path((project, subscription)): Path<(String, String)>,
    State(state): State<RestState>,
    Json(payload): Json<ModifyAckDeadlineRequest>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    debug!("REST: ModifyAckDeadline {}/{}, {} messages", project, subscription, payload.ack_ids.len());

    let sub_id = format!("{}:{}", project, subscription);

    // Get subscription config to find topic
    let config = state
        .backend
        .get_subscription(&sub_id)
        .await
        .map_err(|_| ErrorResponse::new(StatusCode::NOT_FOUND, format!("Subscription not found: {}/{}", project, subscription)))?;

    // Modify visibility for each ack_id (receipt handle)
    for ack_id in &payload.ack_ids {
        state
            .backend
            .change_visibility(&config.topic_id, ack_id, payload.ack_deadline_seconds as u32)
            .await
            .map_err(|e| ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to modify ack deadline: {}", e)))?;
    }

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::InMemoryBackend;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use ::base64::Engine as _;
    use ::base64::engine::general_purpose;
    use tower::ServiceExt; // for oneshot()

    /// Create a test state with an in-memory backend.
    fn create_test_state() -> RestState {
        let backend = Arc::new(InMemoryBackend::new());
        RestState::new(backend)
    }

    /// Helper to create a test app with the REST router.
    fn create_test_app() -> Router {
        let state = create_test_state();
        create_router(state)
    }

    /// Helper to send a request and get the response.
    async fn send_request(
        app: Router,
        method: Method,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder().method(method).uri(uri);

        let body_bytes = if let Some(json) = body {
            request = request.header("content-type", "application/json");
            serde_json::to_vec(&json).unwrap()
        } else {
            Vec::new()
        };

        let request = request.body(Body::from(body_bytes)).unwrap();

        let response = app.oneshot(request).await.unwrap();
        let status = response.status();

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_json: serde_json::Value = if body_bytes.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&body_bytes).unwrap_or(serde_json::json!({}))
        };

        (status, body_json)
    }

    // ========================================================================
    // Topic Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_topic_success() {
        let app = create_test_app();

        let body = serde_json::json!({
            "labels": {"env": "test"}
        });

        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/topics/test-topic",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response["name"],
            "projects/test-project/topics/test-topic"
        );
        assert_eq!(response["labels"]["env"], "test");
    }

    #[tokio::test]
    async fn test_create_topic_invalid_id() {
        let app = create_test_app();

        let body = serde_json::json!({});

        // Topic ID "123" starts with a number (invalid)
        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/topics/123invalid",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid topic ID"));
    }

    #[tokio::test]
    async fn test_create_topic_duplicate() {
        let app = create_test_app();

        let body = serde_json::json!({});

        // Create first time
        let (status, _) = send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/dup-topic",
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Create again (should fail)
        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/topics/dup-topic",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already exists"));
    }

    #[tokio::test]
    async fn test_get_topic_success() {
        let app = create_test_app();

        // Create topic first
        let body = serde_json::json!({});
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/get-topic",
            Some(body),
        )
        .await;

        // Get topic
        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/test-project/topics/get-topic",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["name"], "projects/test-project/topics/get-topic");
    }

    #[tokio::test]
    async fn test_get_topic_not_found() {
        let app = create_test_app();

        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/test-project/topics/nonexistent",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_topic_success() {
        let app = create_test_app();

        // Create topic first
        let body = serde_json::json!({});
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/del-topic",
            Some(body),
        )
        .await;

        // Delete topic
        let (status, _) = send_request(
            app,
            Method::DELETE,
            "/v1/projects/test-project/topics/del-topic",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_topic_not_found() {
        let app = create_test_app();

        let (status, response) = send_request(
            app,
            Method::DELETE,
            "/v1/projects/test-project/topics/nonexistent",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_list_topics_empty() {
        let app = create_test_app();

        let (status, response) =
            send_request(app, Method::GET, "/v1/projects/test-project/topics", None).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["topics"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_list_topics_with_data() {
        let app = create_test_app();

        // Create two topics
        let body = serde_json::json!({});
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/topic1",
            Some(body.clone()),
        )
        .await;
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/topic2",
            Some(body),
        )
        .await;

        // List topics
        let (status, response) =
            send_request(app, Method::GET, "/v1/projects/test-project/topics", None).await;

        assert_eq!(status, StatusCode::OK);
        let topics = response["topics"].as_array().unwrap();
        assert_eq!(topics.len(), 2);
    }

    #[tokio::test]
    async fn test_list_topics_filters_by_project() {
        let app = create_test_app();

        // Create topics in different projects
        let body = serde_json::json!({});
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/project-a/topics/topic1",
            Some(body.clone()),
        )
        .await;
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/project-b/topics/topic2",
            Some(body),
        )
        .await;

        // List topics for project-a only
        let (status, response) =
            send_request(app, Method::GET, "/v1/projects/project-a/topics", None).await;

        assert_eq!(status, StatusCode::OK);
        let topics = response["topics"].as_array().unwrap();
        assert_eq!(topics.len(), 1);
        assert!(topics[0]["name"]
            .as_str()
            .unwrap()
            .contains("project-a"));
    }

    #[tokio::test]
    async fn test_publish_success() {
        let app = create_test_app();

        // Create topic first
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/pub-topic",
            Some(serde_json::json!({})),
        )
        .await;

        // Publish messages
        let data = general_purpose::STANDARD.encode("Hello world");
        let body = serde_json::json!({
            "messages": [
                {
                    "data": data,
                    "attributes": {"key1": "value1"}
                }
            ]
        });

        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/topics/pub-topic:publish",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let message_ids = response["messageIds"].as_array().unwrap();
        assert_eq!(message_ids.len(), 1);
        assert!(!message_ids[0].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_publish_topic_not_found() {
        let app = create_test_app();

        let data = general_purpose::STANDARD.encode("Hello");
        let body = serde_json::json!({
            "messages": [{"data": data}]
        });

        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/topics/nonexistent:publish",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_publish_with_ordering_key() {
        let app = create_test_app();

        // Create topic
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/ordered-topic",
            Some(serde_json::json!({})),
        )
        .await;

        // Publish with ordering key
        let data = general_purpose::STANDARD.encode("Ordered message");
        let body = serde_json::json!({
            "messages": [
                {
                    "data": data,
                    "orderingKey": "order-key-1"
                }
            ]
        });

        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/topics/ordered-topic:publish",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["messageIds"].as_array().unwrap().len(), 1);
    }

    // ========================================================================
    // Subscription Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_subscription_success() {
        let app = create_test_app();

        // Create topic first
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/sub-topic",
            Some(serde_json::json!({})),
        )
        .await;

        // Create subscription
        let body = serde_json::json!({
            "topic": "projects/test-project/topics/sub-topic",
            "ackDeadlineSeconds": 30
        });

        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/subscriptions/test-sub",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response["name"],
            "projects/test-project/subscriptions/test-sub"
        );
        assert_eq!(
            response["topic"],
            "projects/test-project/topics/sub-topic"
        );
        assert_eq!(response["ackDeadlineSeconds"], 30);
    }

    #[tokio::test]
    async fn test_create_subscription_invalid_id() {
        let app = create_test_app();

        let body = serde_json::json!({
            "topic": "projects/test-project/topics/some-topic"
        });

        // Subscription ID "123" starts with a number (invalid)
        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/subscriptions/123invalid",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid subscription ID"));
    }

    #[tokio::test]
    async fn test_create_subscription_topic_not_found() {
        let app = create_test_app();

        let body = serde_json::json!({
            "topic": "projects/test-project/topics/nonexistent"
        });

        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/subscriptions/test-sub",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_create_subscription_invalid_topic_format() {
        let app = create_test_app();

        let body = serde_json::json!({
            "topic": "invalid-topic-format"
        });

        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test-project/subscriptions/test-sub",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid topic name"));
    }

    #[tokio::test]
    async fn test_get_subscription_success() {
        let app = create_test_app();

        // Create topic and subscription
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/sub-topic",
            Some(serde_json::json!({})),
        )
        .await;

        let body = serde_json::json!({
            "topic": "projects/test-project/topics/sub-topic"
        });
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/get-sub",
            Some(body),
        )
        .await;

        // Get subscription
        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/test-project/subscriptions/get-sub",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response["name"],
            "projects/test-project/subscriptions/get-sub"
        );
    }

    #[tokio::test]
    async fn test_get_subscription_not_found() {
        let app = create_test_app();

        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/test-project/subscriptions/nonexistent",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_subscription_success() {
        let app = create_test_app();

        // Create topic and subscription
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/sub-topic",
            Some(serde_json::json!({})),
        )
        .await;

        let body = serde_json::json!({
            "topic": "projects/test-project/topics/sub-topic"
        });
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/del-sub",
            Some(body),
        )
        .await;

        // Delete subscription
        let (status, _) = send_request(
            app,
            Method::DELETE,
            "/v1/projects/test-project/subscriptions/del-sub",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_subscription_not_found() {
        let app = create_test_app();

        let (status, response) = send_request(
            app,
            Method::DELETE,
            "/v1/projects/test-project/subscriptions/nonexistent",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_list_subscriptions_empty() {
        let app = create_test_app();

        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/test-project/subscriptions",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["subscriptions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_list_subscriptions_with_data() {
        let app = create_test_app();

        // Create topic
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/list-topic",
            Some(serde_json::json!({})),
        )
        .await;

        // Create two subscriptions
        let body = serde_json::json!({
            "topic": "projects/test-project/topics/list-topic"
        });
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/sub1",
            Some(body.clone()),
        )
        .await;
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/sub2",
            Some(body),
        )
        .await;

        // List subscriptions
        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/test-project/subscriptions",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let subs = response["subscriptions"].as_array().unwrap();
        assert_eq!(subs.len(), 2);
    }

    #[tokio::test]
    async fn test_list_subscriptions_filters_by_project() {
        let app = create_test_app();

        // Create topics in different projects
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/project-a/topics/topic-a",
            Some(serde_json::json!({})),
        )
        .await;
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/project-b/topics/topic-b",
            Some(serde_json::json!({})),
        )
        .await;

        // Create subscriptions in different projects
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/project-a/subscriptions/sub-a",
            Some(serde_json::json!({
                "topic": "projects/project-a/topics/topic-a"
            })),
        )
        .await;
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/project-b/subscriptions/sub-b",
            Some(serde_json::json!({
                "topic": "projects/project-b/topics/topic-b"
            })),
        )
        .await;

        // List subscriptions for project-a only
        let (status, response) = send_request(
            app,
            Method::GET,
            "/v1/projects/project-a/subscriptions",
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let subs = response["subscriptions"].as_array().unwrap();
        assert_eq!(subs.len(), 1);
        assert!(subs[0]["name"]
            .as_str()
            .unwrap()
            .contains("project-a"));
    }

    #[tokio::test]
    async fn test_pull_empty_queue() {
        let app = create_test_app();

        // Create topic and subscription
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/pull-topic",
            Some(serde_json::json!({})),
        )
        .await;

        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/pull-sub",
            Some(serde_json::json!({
                "topic": "projects/test-project/topics/pull-topic"
            })),
        )
        .await;

        // Pull messages (should be empty)
        let body = serde_json::json!({
            "maxMessages": 10
        });

        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/subscriptions/pull-sub:pull",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(response["receivedMessages"].is_null());
    }

    #[tokio::test]
    async fn test_pull_with_messages() {
        let app = create_test_app();

        // Create topic and subscription
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/pull-topic2",
            Some(serde_json::json!({})),
        )
        .await;

        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/pull-sub2",
            Some(serde_json::json!({
                "topic": "projects/test-project/topics/pull-topic2"
            })),
        )
        .await;

        // Publish a message
        let data = general_purpose::STANDARD.encode("Test message");
        send_request(
            app.clone(),
            Method::POST,
            "/v1/projects/test-project/topics/pull-topic2:publish",
            Some(serde_json::json!({
                "messages": [{"data": data}]
            })),
        )
        .await;

        // Pull messages
        let body = serde_json::json!({
            "maxMessages": 10
        });

        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/subscriptions/pull-sub2:pull",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let received = response["receivedMessages"].as_array().unwrap();
        assert_eq!(received.len(), 1);
        assert!(!received[0]["ackId"].as_str().unwrap().is_empty());
        assert!(!received[0]["message"]["messageId"]
            .as_str()
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_acknowledge_messages() {
        let app = create_test_app();

        // Create topic and subscription
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/ack-topic",
            Some(serde_json::json!({})),
        )
        .await;

        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/ack-sub",
            Some(serde_json::json!({
                "topic": "projects/test-project/topics/ack-topic"
            })),
        )
        .await;

        // Publish and pull a message
        let data = general_purpose::STANDARD.encode("Ack test");
        send_request(
            app.clone(),
            Method::POST,
            "/v1/projects/test-project/topics/ack-topic:publish",
            Some(serde_json::json!({
                "messages": [{"data": data}]
            })),
        )
        .await;

        let (_, pull_response) = send_request(
            app.clone(),
            Method::POST,
            "/v1/projects/test-project/subscriptions/ack-sub:pull",
            Some(serde_json::json!({"maxMessages": 1})),
        )
        .await;

        let ack_id = pull_response["receivedMessages"][0]["ackId"]
            .as_str()
            .unwrap()
            .to_string();

        // Acknowledge the message
        let body = serde_json::json!({
            "ackIds": [ack_id]
        });

        let (status, _) = send_request(
            app.clone(),
            Method::POST,
            "/v1/projects/test-project/subscriptions/ack-sub:acknowledge",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);

        // Pull again - should be empty now
        let (_, pull_response2) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/subscriptions/ack-sub:pull",
            Some(serde_json::json!({"maxMessages": 1})),
        )
        .await;

        assert!(pull_response2["receivedMessages"].is_null());
    }

    #[tokio::test]
    async fn test_modify_ack_deadline() {
        let app = create_test_app();

        // Create topic and subscription
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/topics/modify-topic",
            Some(serde_json::json!({})),
        )
        .await;

        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test-project/subscriptions/modify-sub",
            Some(serde_json::json!({
                "topic": "projects/test-project/topics/modify-topic"
            })),
        )
        .await;

        // Publish and pull a message
        let data = general_purpose::STANDARD.encode("Modify test");
        send_request(
            app.clone(),
            Method::POST,
            "/v1/projects/test-project/topics/modify-topic:publish",
            Some(serde_json::json!({
                "messages": [{"data": data}]
            })),
        )
        .await;

        let (_, pull_response) = send_request(
            app.clone(),
            Method::POST,
            "/v1/projects/test-project/subscriptions/modify-sub:pull",
            Some(serde_json::json!({"maxMessages": 1})),
        )
        .await;

        let ack_id = pull_response["receivedMessages"][0]["ackId"]
            .as_str()
            .unwrap()
            .to_string();

        // Modify ack deadline
        let body = serde_json::json!({
            "ackIds": [ack_id],
            "ackDeadlineSeconds": 60
        });

        let (status, _) = send_request(
            app,
            Method::POST,
            "/v1/projects/test-project/subscriptions/modify-sub:modifyAckDeadline",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    // ========================================================================
    // Error Response Tests
    // ========================================================================

    #[test]
    fn test_error_response_formatting() {
        let err = ErrorResponse::new(StatusCode::BAD_REQUEST, "Test error");
        assert_eq!(err.error.code, 400);
        assert_eq!(err.error.message, "Test error");
        assert_eq!(err.error.status, "INVALID_ARGUMENT");

        let err = ErrorResponse::new(StatusCode::NOT_FOUND, "Not found");
        assert_eq!(err.error.status, "NOT_FOUND");

        let err = ErrorResponse::new(StatusCode::CONFLICT, "Conflict");
        assert_eq!(err.error.status, "ALREADY_EXISTS");
    }

    // ========================================================================
    // Conversion Tests
    // ========================================================================

    #[test]
    fn test_topic_to_queue_config_conversion() {
        let topic = Topic {
            name: Some("projects/test/topics/my-topic".to_string()),
            labels: Some([("key".to_string(), "value".to_string())].into()),
            message_retention_duration: Some("86400s".to_string()),
        };

        let config = RestState::topic_to_queue_config(&topic, "test", "my-topic").unwrap();
        assert_eq!(config.id, "test:my-topic");
        assert_eq!(config.name, "projects/test/topics/my-topic");
        assert_eq!(config.message_retention_period, 86400);
        assert_eq!(config.tags.get("key").unwrap(), "value");
    }

    #[test]
    fn test_queue_config_to_topic_conversion() {
        use crate::types::QueueConfig;

        let config = QueueConfig {
            id: "test:my-topic".to_string(),
            name: "projects/test/topics/my-topic".to_string(),
            queue_type: crate::types::QueueType::PubSubTopic,
            visibility_timeout: 60,
            message_retention_period: 86400,
            max_message_size: 10 * 1024 * 1024,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: [("key".to_string(), "value".to_string())].into(),
            redrive_allow_policy: None,
        };

        let topic = RestState::queue_config_to_topic(&config);
        assert_eq!(topic.name.unwrap(), "projects/test/topics/my-topic");
        assert_eq!(topic.labels.unwrap().get("key").unwrap(), "value");
        assert_eq!(topic.message_retention_duration.unwrap(), "86400s");
    }

    #[test]
    fn test_pubsub_message_base64_encoding() {
        use crate::types::{Message, MessageId};
        use chrono::Utc;

        let msg = Message {
            id: MessageId::new(),
            body: general_purpose::STANDARD.encode(b"test"),
            attributes: Default::default(),
            queue_id: "test:topic".to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        };

        let pubsub_msg = RestState::message_to_pubsub_message(&msg);
        assert_eq!(pubsub_msg.data, b"test");
    }

    #[tokio::test]
    async fn test_create_subscription_with_dead_letter_policy() {
        let app = create_test_app();

        // First create a topic
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test/topics/test-topic",
            Some(serde_json::json!({})),
        )
        .await;

        // Create subscription with dead letter policy
        let body = serde_json::json!({
            "topic": "projects/test/topics/test-topic",
            "deadLetterPolicy": {
                "deadLetterTopic": "projects/test/topics/dlq-topic",
                "maxDeliveryAttempts": 5
            }
        });

        let (status, response) = send_request(
            app,
            Method::PUT,
            "/v1/projects/test/subscriptions/test-sub",
            Some(body),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(response["deadLetterPolicy"]["deadLetterTopic"], "projects/test/topics/dlq-topic");
        assert_eq!(response["deadLetterPolicy"]["maxDeliveryAttempts"], 5);
    }

    #[tokio::test]
    async fn test_subscription_with_non_standard_id_format() {
        // This should use fallback logic for parsing (lines 140, 148)
        let config = crate::types::SubscriptionConfig {
            id: "simple-sub".to_string(), // No project prefix
            name: "simple-sub".to_string(),
            topic_id: "simple-topic".to_string(), // No project prefix
            ack_deadline_seconds: 10,
            message_retention_duration: 86400,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
        };

        let converted = RestState::config_to_subscription(&config);
        assert_eq!(converted.topic, "projects/default/topics/simple-topic");
    }

    #[tokio::test]
    async fn test_publish_message_with_string_attributes() {
        use crate::types::{Message, MessageId, MessageAttributeValue};
        use chrono::Utc;
        use std::collections::HashMap;

        let mut attributes = HashMap::new();
        attributes.insert(
            "attr1".to_string(),
            MessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some("value1".to_string()),
                binary_value: None,
            },
        );
        attributes.insert(
            "attr2".to_string(),
            MessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some("value2".to_string()),
                binary_value: None,
            },
        );

        let msg = Message {
            id: MessageId::new(),
            body: general_purpose::STANDARD.encode(b"test"),
            attributes,
            queue_id: "test:topic".to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        };

        // This should extract string attributes (lines 221-222)
        let pubsub_msg = RestState::message_to_pubsub_message(&msg);
        assert_eq!(pubsub_msg.data, b"test");
        assert!(pubsub_msg.attributes.is_some());
        let attrs = pubsub_msg.attributes.unwrap();
        assert_eq!(attrs.get("attr1").unwrap(), "value1");
        assert_eq!(attrs.get("attr2").unwrap(), "value2");
    }

    #[test]
    fn test_error_response_status_codes() {
        // Test INTERNAL_SERVER_ERROR (line 260)
        let err = ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
        assert_eq!(err.error.status, "INTERNAL");
        assert_eq!(err.error.code, 500);

        // Test UNPROCESSABLE_ENTITY (line 261)
        let err = ErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "Unprocessable");
        assert_eq!(err.error.status, "FAILED_PRECONDITION");
        assert_eq!(err.error.code, 422);

        // Test default case (line 262)
        let err = ErrorResponse::new(StatusCode::IM_A_TEAPOT, "I'm a teapot");
        assert_eq!(err.error.status, "UNKNOWN");
        assert_eq!(err.error.code, 418);
    }

    #[test]
    fn test_pubsub_message_invalid_base64() {
        // Test deserialization with invalid base64 (line 369)
        let json = serde_json::json!({
            "data": "invalid-base64-!!!" // Invalid base64 characters
        });

        let result: std::result::Result<PubsubMessage, _> = serde_json::from_value(json);
        assert!(result.is_err()); // Should fail to deserialize
    }

    #[tokio::test]
    async fn test_start_rest_server_lifecycle() {
        use tokio::sync::broadcast;

        let config = RestServerConfig {
            bind_address: "127.0.0.1:0".to_string(), // Port 0 = random available port
        };
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Start server in background
        let server_handle = tokio::spawn(async move {
            start_rest_server(config, backend, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown
        let _ = shutdown_tx.send(());

        // Wait for graceful shutdown
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            server_handle
        ).await;

        assert!(result.is_ok());
        assert!(result.unwrap().unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_unknown_topic_action() {
        let app = create_test_app();

        // Create a topic first
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test/topics/test-topic",
            Some(serde_json::json!({})),
        )
        .await;

        // Try unknown action (line 605-607)
        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test/topics/test-topic:unknownAction",
            Some(serde_json::json!({})),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"].as_str().unwrap().contains("Unknown topic action"));
    }

    #[tokio::test]
    async fn test_invalid_topic_action_format() {
        let app = create_test_app();

        // No colon separator (line 611)
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/projects/test/topics/no-action-separator")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&serde_json::json!({})).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_unknown_subscription_action() {
        let app = create_test_app();

        // Create topic and subscription first
        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test/topics/test-topic",
            Some(serde_json::json!({})),
        )
        .await;

        send_request(
            app.clone(),
            Method::PUT,
            "/v1/projects/test/subscriptions/test-sub",
            Some(serde_json::json!({
                "topic": "projects/test/topics/test-topic"
            })),
        )
        .await;

        // Try unknown action (line 913-915)
        let (status, response) = send_request(
            app,
            Method::POST,
            "/v1/projects/test/subscriptions/test-sub:unknownAction",
            Some(serde_json::json!({})),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(response["error"]["message"].as_str().unwrap().contains("Unknown subscription action"));
    }

    #[tokio::test]
    async fn test_invalid_subscription_action_format() {
        let app = create_test_app();

        // No colon separator (line 919)
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/projects/test/subscriptions/no-action-separator")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&serde_json::json!({})).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
