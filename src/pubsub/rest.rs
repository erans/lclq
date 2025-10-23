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
    routing::{get, post, put},
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
    #[allow(dead_code)] // Will be used when implementing handlers
    backend: Arc<dyn StorageBackend>,
}

impl RestState {
    /// Create a new REST state.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
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
    pub name: String,
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
    pub name: String,
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
            "/v1/projects/:project/topics/:topic",
            put(create_topic).get(get_topic).delete(delete_topic),
        )
        .route("/v1/projects/:project/topics", get(list_topics))
        .route(
            "/v1/projects/:project/topics/:topic:publish",
            post(publish),
        )
        // Subscription endpoints
        .route(
            "/v1/projects/:project/subscriptions/:subscription",
            put(create_subscription)
                .get(get_subscription)
                .delete(delete_subscription),
        )
        .route(
            "/v1/projects/:project/subscriptions",
            get(list_subscriptions),
        )
        .route(
            "/v1/projects/:project/subscriptions/:subscription:pull",
            post(pull),
        )
        .route(
            "/v1/projects/:project/subscriptions/:subscription:acknowledge",
            post(acknowledge),
        )
        .route(
            "/v1/projects/:project/subscriptions/:subscription:modifyAckDeadline",
            post(modify_ack_deadline),
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
// Topic Handlers - Placeholders (to be implemented)
// ============================================================================

async fn create_topic(
    Path((project, topic)): Path<(String, String)>,
    State(_state): State<RestState>,
    Json(_payload): Json<Topic>,
) -> std::result::Result<Json<Topic>, ErrorResponse> {
    info!("REST: CreateTopic {}/{}", project, topic);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "CreateTopic not yet implemented",
    ))
}

async fn get_topic(
    Path((project, topic)): Path<(String, String)>,
    State(_state): State<RestState>,
) -> std::result::Result<Json<Topic>, ErrorResponse> {
    debug!("REST: GetTopic {}/{}", project, topic);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "GetTopic not yet implemented",
    ))
}

async fn delete_topic(
    Path((project, topic)): Path<(String, String)>,
    State(_state): State<RestState>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    info!("REST: DeleteTopic {}/{}", project, topic);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "DeleteTopic not yet implemented",
    ))
}

async fn list_topics(
    Path(project): Path<String>,
    Query(_query): Query<ListTopicsQuery>,
    State(_state): State<RestState>,
) -> std::result::Result<Json<ListTopicsResponse>, ErrorResponse> {
    debug!("REST: ListTopics {}", project);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "ListTopics not yet implemented",
    ))
}

async fn publish(
    Path((project, topic)): Path<(String, String)>,
    State(_state): State<RestState>,
    Json(_payload): Json<PublishRequest>,
) -> std::result::Result<Json<PublishResponse>, ErrorResponse> {
    debug!("REST: Publish {}/{}", project, topic);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "Publish not yet implemented",
    ))
}

// ============================================================================
// Subscription Handlers - Placeholders (to be implemented)
// ============================================================================

async fn create_subscription(
    Path((project, subscription)): Path<(String, String)>,
    State(_state): State<RestState>,
    Json(_payload): Json<Subscription>,
) -> std::result::Result<Json<Subscription>, ErrorResponse> {
    info!("REST: CreateSubscription {}/{}", project, subscription);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "CreateSubscription not yet implemented",
    ))
}

async fn get_subscription(
    Path((project, subscription)): Path<(String, String)>,
    State(_state): State<RestState>,
) -> std::result::Result<Json<Subscription>, ErrorResponse> {
    debug!("REST: GetSubscription {}/{}", project, subscription);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "GetSubscription not yet implemented",
    ))
}

async fn delete_subscription(
    Path((project, subscription)): Path<(String, String)>,
    State(_state): State<RestState>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    info!("REST: DeleteSubscription {}/{}", project, subscription);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "DeleteSubscription not yet implemented",
    ))
}

async fn list_subscriptions(
    Path(project): Path<String>,
    Query(_query): Query<ListSubscriptionsQuery>,
    State(_state): State<RestState>,
) -> std::result::Result<Json<ListSubscriptionsResponse>, ErrorResponse> {
    debug!("REST: ListSubscriptions {}", project);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "ListSubscriptions not yet implemented",
    ))
}

async fn pull(
    Path((project, subscription)): Path<(String, String)>,
    State(_state): State<RestState>,
    Json(_payload): Json<PullRequest>,
) -> std::result::Result<Json<PullResponse>, ErrorResponse> {
    debug!("REST: Pull {}/{}", project, subscription);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "Pull not yet implemented",
    ))
}

async fn acknowledge(
    Path((project, subscription)): Path<(String, String)>,
    State(_state): State<RestState>,
    Json(_payload): Json<AcknowledgeRequest>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    debug!("REST: Acknowledge {}/{}", project, subscription);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "Acknowledge not yet implemented",
    ))
}

async fn modify_ack_deadline(
    Path((project, subscription)): Path<(String, String)>,
    State(_state): State<RestState>,
    Json(_payload): Json<ModifyAckDeadlineRequest>,
) -> std::result::Result<StatusCode, ErrorResponse> {
    debug!("REST: ModifyAckDeadline {}/{}", project, subscription);
    Err(ErrorResponse::new(
        StatusCode::NOT_IMPLEMENTED,
        "ModifyAckDeadline not yet implemented",
    ))
}
