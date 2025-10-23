/// Admin HTTP API server for queue management and monitoring
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::storage::StorageBackend;
use crate::types::{QueueConfig, QueueType};

/// Admin API server state
#[derive(Clone)]
struct AdminState {
    backend: Arc<dyn StorageBackend>,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    backend_status: String,
}

/// Overall statistics response
#[derive(Debug, Serialize)]
struct StatsResponse {
    total_queues: usize,
    total_messages: u64,
    total_in_flight: u64,
}

/// Queue list response
#[derive(Debug, Serialize)]
struct QueueListResponse {
    queues: Vec<QueueSummary>,
}

/// Queue summary for list endpoint
#[derive(Debug, Serialize)]
struct QueueSummary {
    id: String,
    name: String,
    queue_type: String,
    available_messages: u64,
    in_flight_messages: u64,
}

/// Queue details response
#[derive(Debug, Serialize)]
struct QueueDetailsResponse {
    id: String,
    name: String,
    queue_type: String,
    visibility_timeout: u32,
    message_retention_period: u32,
    max_message_size: usize,
    delay_seconds: u32,
    available_messages: u64,
    in_flight_messages: u64,
    dlq_target_queue_id: Option<String>,
    max_receive_count: Option<u32>,
}

/// Create queue request
#[derive(Debug, Deserialize)]
struct CreateQueueRequest {
    name: String,
    #[serde(default)]
    queue_type: Option<String>,
    #[serde(default)]
    visibility_timeout: Option<u32>,
    #[serde(default)]
    message_retention_period: Option<u32>,
}

/// Error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Admin API error type
#[derive(Debug)]
enum AdminError {
    NotFound(String),
    InvalidInput(String),
    BackendError(String),
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AdminError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AdminError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            AdminError::BackendError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<crate::Error> for AdminError {
    fn from(err: crate::Error) -> Self {
        match err {
            crate::Error::QueueNotFound(_) => AdminError::NotFound(err.to_string()),
            crate::Error::Validation(_) => AdminError::InvalidInput(err.to_string()),
            _ => AdminError::BackendError(err.to_string()),
        }
    }
}

/// Health check endpoint
async fn health_check(State(state): State<AdminState>) -> Result<Json<HealthResponse>, AdminError> {
    let backend_health = state.backend.health_check().await;

    let status = match backend_health {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    };

    Ok(Json(HealthResponse {
        status: status.to_string(),
        backend_status: status.to_string(),
    }))
}

/// Overall statistics endpoint
async fn get_stats(State(state): State<AdminState>) -> Result<Json<StatsResponse>, AdminError> {
    let queues = state.backend.list_queues(None).await?;

    let mut total_messages = 0u64;
    let mut total_in_flight = 0u64;

    for queue in &queues {
        if let Ok(stats) = state.backend.get_stats(&queue.id).await {
            total_messages += stats.available_messages;
            total_in_flight += stats.in_flight_messages;
        }
    }

    Ok(Json(StatsResponse {
        total_queues: queues.len(),
        total_messages,
        total_in_flight,
    }))
}

/// List all queues endpoint
async fn list_queues(State(state): State<AdminState>) -> Result<Json<QueueListResponse>, AdminError> {
    let queues = state.backend.list_queues(None).await?;

    let mut queue_summaries = Vec::new();
    for queue in queues {
        let stats = state.backend.get_stats(&queue.id).await.ok();

        queue_summaries.push(QueueSummary {
            id: queue.id.clone(),
            name: queue.name.clone(),
            queue_type: format!("{:?}", queue.queue_type),
            available_messages: stats.as_ref().map(|s| s.available_messages).unwrap_or(0),
            in_flight_messages: stats.as_ref().map(|s| s.in_flight_messages).unwrap_or(0),
        });
    }

    Ok(Json(QueueListResponse {
        queues: queue_summaries,
    }))
}

/// Get queue details endpoint
async fn get_queue(
    State(state): State<AdminState>,
    Path(name): Path<String>,
) -> Result<Json<QueueDetailsResponse>, AdminError> {
    // Find queue by name
    let queues = state.backend.list_queues(None).await?;

    let queue = queues
        .into_iter()
        .find(|q| q.name == name)
        .ok_or_else(|| AdminError::NotFound(format!("Queue '{}' not found", name)))?;

    let stats = state.backend.get_stats(&queue.id).await.ok();

    Ok(Json(QueueDetailsResponse {
        id: queue.id.clone(),
        name: queue.name.clone(),
        queue_type: format!("{:?}", queue.queue_type),
        visibility_timeout: queue.visibility_timeout,
        message_retention_period: queue.message_retention_period,
        max_message_size: queue.max_message_size,
        delay_seconds: queue.delay_seconds,
        available_messages: stats.as_ref().map(|s| s.available_messages).unwrap_or(0),
        in_flight_messages: stats.as_ref().map(|s| s.in_flight_messages).unwrap_or(0),
        dlq_target_queue_id: queue.dlq_config.as_ref().map(|c| c.target_queue_id.clone()),
        max_receive_count: queue.dlq_config.as_ref().map(|c| c.max_receive_count),
    }))
}

/// Create queue endpoint
async fn create_queue(
    State(state): State<AdminState>,
    Json(req): Json<CreateQueueRequest>,
) -> Result<Json<QueueDetailsResponse>, AdminError> {
    // Parse queue type
    let queue_type = match req.queue_type.as_deref() {
        Some("fifo") | Some("FIFO") => QueueType::SqsFifo,
        Some("pubsub") | Some("PubSub") => QueueType::PubSubTopic,
        _ => QueueType::SqsStandard,
    };

    // Create queue config
    let config = QueueConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name.clone(),
        queue_type,
        visibility_timeout: req.visibility_timeout.unwrap_or(30),
        message_retention_period: req.message_retention_period.unwrap_or(345600),
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: std::collections::HashMap::new(),
        redrive_allow_policy: None,
    };

    // Create queue in backend
    let queue = state.backend.create_queue(config).await?;

    let stats = state.backend.get_stats(&queue.id).await.ok();

    Ok(Json(QueueDetailsResponse {
        id: queue.id.clone(),
        name: queue.name.clone(),
        queue_type: format!("{:?}", queue.queue_type),
        visibility_timeout: queue.visibility_timeout,
        message_retention_period: queue.message_retention_period,
        max_message_size: queue.max_message_size,
        delay_seconds: queue.delay_seconds,
        available_messages: stats.as_ref().map(|s| s.available_messages).unwrap_or(0),
        in_flight_messages: stats.as_ref().map(|s| s.in_flight_messages).unwrap_or(0),
        dlq_target_queue_id: queue.dlq_config.as_ref().map(|c| c.target_queue_id.clone()),
        max_receive_count: queue.dlq_config.as_ref().map(|c| c.max_receive_count),
    }))
}

/// Delete queue endpoint
async fn delete_queue(
    State(state): State<AdminState>,
    Path(name): Path<String>,
) -> Result<StatusCode, AdminError> {
    // Find queue by name
    let queues = state.backend.list_queues(None).await?;

    let queue = queues
        .into_iter()
        .find(|q| q.name == name)
        .ok_or_else(|| AdminError::NotFound(format!("Queue '{}' not found", name)))?;

    state.backend.delete_queue(&queue.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Purge queue endpoint
async fn purge_queue(
    State(state): State<AdminState>,
    Path(name): Path<String>,
) -> Result<StatusCode, AdminError> {
    // Find queue by name
    let queues = state.backend.list_queues(None).await?;

    let queue = queues
        .into_iter()
        .find(|q| q.name == name)
        .ok_or_else(|| AdminError::NotFound(format!("Queue '{}' not found", name)))?;

    state.backend.purge_queue(&queue.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Start the Admin HTTP API server
pub async fn start_admin_server(
    backend: Arc<dyn StorageBackend>,
    port: u16,
) -> anyhow::Result<()> {
    let state = AdminState { backend };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/stats", get(get_stats))
        .route("/queues", get(list_queues))
        .route("/queues", post(create_queue))
        .route("/queues/{name}", get(get_queue))
        .route("/queues/{name}", delete(delete_queue))
        .route("/queues/{name}/purge", post(purge_queue))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Admin API server listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| {
            error!("Admin API server error: {}", e);
            anyhow::anyhow!("Admin API server failed: {}", e)
        })
}
