/// Admin HTTP API server for queue management and monitoring
use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::server::shutdown::shutdown_receiver;
use crate::storage::StorageBackend;
use crate::types::{QueueConfig, QueueType};

/// Admin API server state
#[derive(Clone)]
struct AdminState {
    backend: Arc<dyn StorageBackend>,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    backend_status: String,
}

/// Overall statistics response
#[derive(Debug, Serialize, Deserialize)]
struct StatsResponse {
    total_queues: usize,
    total_messages: u64,
    total_in_flight: u64,
}

/// Queue list response
#[derive(Debug, Serialize, Deserialize)]
struct QueueListResponse {
    queues: Vec<QueueSummary>,
}

/// Queue summary for list endpoint
#[derive(Debug, Serialize, Deserialize)]
struct QueueSummary {
    id: String,
    name: String,
    queue_type: String,
    available_messages: u64,
    in_flight_messages: u64,
}

/// Queue details response
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
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
async fn list_queues(
    State(state): State<AdminState>,
) -> Result<Json<QueueListResponse>, AdminError> {
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
    bind_address: String,
    port: u16,
    shutdown_rx: broadcast::Receiver<()>,
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

    let addr = format!("{}:{}", bind_address, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Admin API server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_receiver(shutdown_rx))
        .await
        .map_err(|e| {
            error!("Admin API server error: {}", e);
            anyhow::anyhow!("Admin API server failed: {}", e)
        })?;

    info!("Admin API server shut down gracefully");
    Ok(())
}

/// Helper function to create a router for testing without starting the server
#[cfg(test)]
fn create_router(backend: Arc<dyn StorageBackend>) -> Router {
    let state = AdminState { backend };
    Router::new()
        .route("/health", get(health_check))
        .route("/stats", get(get_stats))
        .route("/queues", get(list_queues))
        .route("/queues", post(create_queue))
        .route("/queues/{name}", get(get_queue))
        .route("/queues/{name}", delete(delete_queue))
        .route("/queues/{name}/purge", post(purge_queue))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageBackend;
    use crate::storage::memory::InMemoryBackend;
    use crate::types::{QueueConfig, QueueType};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;

    /// Create a test admin router with in-memory backend.
    fn create_test_router() -> Router {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        create_router(backend)
    }

    /// Helper to extract JSON body from response.
    async fn extract_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
        let (_parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ========================================================================
    // Health Check Tests
    // ========================================================================

    #[tokio::test]
    async fn test_health_check_success() {
        let app = create_test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let health: HealthResponse = extract_json(response).await;
        assert_eq!(health.status, "healthy");
    }

    // ========================================================================
    // Stats Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_stats_empty() {
        let app = create_test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let stats: StatsResponse = extract_json(response).await;
        assert_eq!(stats.total_queues, 0);
        assert_eq!(stats.total_messages, 0);
    }

    #[tokio::test]
    async fn test_get_stats_with_data() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let app = create_router(backend.clone());

        // Create some queues
        let queue1 = QueueConfig {
            id: "queue1".to_string(),
            name: "queue1".to_string(),
            queue_type: QueueType::SqsStandard,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };
        backend.create_queue(queue1).await.unwrap();

        let queue2 = QueueConfig {
            id: "queue2".to_string(),
            name: "queue2".to_string(),
            queue_type: QueueType::SqsFifo,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };
        backend.create_queue(queue2).await.unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let stats: StatsResponse = extract_json(response).await;
        assert_eq!(stats.total_queues, 2);
    }

    // ========================================================================
    // List Queues Tests
    // ========================================================================

    #[tokio::test]
    async fn test_list_queues_empty() {
        let app = create_test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/queues")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let list: QueueListResponse = extract_json(response).await;
        assert_eq!(list.queues.len(), 0);
    }

    #[tokio::test]
    async fn test_list_queues_with_data() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let app = create_router(backend.clone());

        // Create two queues
        for i in 1..=2 {
            let queue = QueueConfig {
                id: format!("queue{}", i),
                name: format!("queue{}", i),
                queue_type: QueueType::SqsStandard,
                visibility_timeout: 30,
                message_retention_period: 345600,
                max_message_size: 262144,
                delay_seconds: 0,
                dlq_config: None,
                content_based_deduplication: false,
                tags: std::collections::HashMap::new(),
                redrive_allow_policy: None,
            };
            backend.create_queue(queue).await.unwrap();
        }

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/queues")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let list: QueueListResponse = extract_json(response).await;
        assert_eq!(list.queues.len(), 2);
    }

    // Note: Prefix filtering via query parameters is not currently implemented in the list_queues endpoint.
    // The endpoint passes None to backend.list_queues(), which returns all queues.

    // ========================================================================
    // Get Queue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_queue_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let app = create_router(backend.clone());

        // Create queue
        let queue = QueueConfig {
            id: "my-queue".to_string(),
            name: "my-queue".to_string(),
            queue_type: QueueType::SqsStandard,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };
        backend.create_queue(queue).await.unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/queues/my-queue")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let detail: QueueDetailsResponse = extract_json(response).await;
        assert_eq!(detail.name, "my-queue");
        assert_eq!(detail.available_messages, 0);
    }

    #[tokio::test]
    async fn test_get_queue_not_found() {
        let app = create_test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/queues/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let error: ErrorResponse = extract_json(response).await;
        assert!(error.error.contains("not found") || error.error.contains("Not found"));
    }

    // ========================================================================
    // Create Queue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_queue_standard() {
        let app = create_test_router();

        let create_request = json!({
            "name": "new-queue",
            "visibility_timeout": 30,
            "message_retention_period": 345600,
            "max_message_size": 262144
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/queues")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let created: QueueDetailsResponse = extract_json(response).await;
        assert_eq!(created.name, "new-queue");
        assert_eq!(created.queue_type, "SqsStandard");
    }

    #[tokio::test]
    async fn test_create_queue_fifo() {
        let app = create_test_router();

        let create_request = json!({
            "name": "fifo-queue.fifo",
            "queue_type": "fifo",
            "visibility_timeout": 30,
            "message_retention_period": 345600,
            "max_message_size": 262144,
            "content_based_deduplication": true
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/queues")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let created: QueueDetailsResponse = extract_json(response).await;
        assert_eq!(created.name, "fifo-queue.fifo");
        assert_eq!(created.queue_type, "SqsFifo");
    }

    #[tokio::test]
    async fn test_create_queue_pubsub() {
        let app = create_test_router();

        let create_request = json!({
            "name": "pubsub-topic",
            "queue_type": "pubsub",
            "visibility_timeout": 60,
            "message_retention_period": 604800,
            "max_message_size": 10485760
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/queues")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let created: QueueDetailsResponse = extract_json(response).await;
        assert_eq!(created.name, "pubsub-topic");
        assert_eq!(created.queue_type, "PubSubTopic");
    }

    // Note: The current implementation generates a new UUID for each queue,
    // so duplicate queue names are not prevented. Duplicate detection is based on queue ID only.

    // ========================================================================
    // Delete Queue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_queue_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let app = create_router(backend.clone());

        // Create queue first
        let queue = QueueConfig {
            id: "delete-me".to_string(),
            name: "delete-me".to_string(),
            queue_type: QueueType::SqsStandard,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };
        backend.create_queue(queue).await.unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/queues/delete-me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_queue_not_found() {
        let app = create_test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/queues/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let error: ErrorResponse = extract_json(response).await;
        assert!(error.error.contains("not found") || error.error.contains("Not found"));
    }

    // ========================================================================
    // Purge Queue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_purge_queue_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let app = create_router(backend.clone());

        // Create queue
        let queue = QueueConfig {
            id: "purge-me".to_string(),
            name: "purge-me".to_string(),
            queue_type: QueueType::SqsStandard,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };
        backend.create_queue(queue).await.unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/queues/purge-me/purge")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_purge_queue_not_found() {
        let app = create_test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/queues/nonexistent/purge")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let error: ErrorResponse = extract_json(response).await;
        assert!(error.error.contains("not found") || error.error.contains("Not found"));
    }

    #[tokio::test]
    async fn test_error_invalid_input() {
        // Test InvalidInput error response (line 103)
        let error = AdminError::InvalidInput("Invalid queue name".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_error_backend_error() {
        // Test BackendError error response (line 104)
        let error = AdminError::BackendError("Database connection failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_error_conversion_validation() {
        // Test From<crate::Error> for Validation error (line 115)
        let err = crate::Error::Validation(crate::error::ValidationError::InvalidParameter {
            name: "QueueName".to_string(),
            reason: "Too long".to_string(),
        });

        let admin_err: AdminError = err.into();
        let response = admin_err.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_error_conversion_queue_not_found() {
        // Test From<crate::Error> for QueueNotFound (line 114)
        let err = crate::Error::QueueNotFound("test-queue".to_string());

        let admin_err: AdminError = err.into();
        let response = admin_err.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_error_conversion_other_errors() {
        // Test From<crate::Error> for other errors (line 116)
        let err = crate::Error::InvalidReceiptHandle;

        let admin_err: AdminError = err.into();
        let response = admin_err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_start_admin_server_lifecycle() {
        use tokio::sync::broadcast;

        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Start server in background
        let server_handle = tokio::spawn(async move {
            start_admin_server(backend, "127.0.0.1".to_string(), 0, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown (covers lines 293-326)
        let _ = shutdown_tx.send(());

        // Wait for graceful shutdown
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(5), server_handle).await;

        assert!(result.is_ok());
        assert!(result.unwrap().unwrap().is_ok());
    }
}
