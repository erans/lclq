//! SQS HTTP server implementation.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};

use crate::config::LclqConfig;
use crate::server::shutdown::shutdown_receiver;
use crate::sqs::{SqsHandler, SqsRequest};
use crate::storage::StorageBackend;
use tokio::sync::broadcast;

/// SQS server state.
#[derive(Clone)]
struct SqsServerState {
    handler: Arc<SqsHandler>,
}

/// Start the SQS HTTP server.
pub async fn start_sqs_server(
    backend: Arc<dyn StorageBackend>,
    config: LclqConfig,
    shutdown_rx: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let bind_addr = format!("{}:{}", config.server.bind_address, config.server.sqs_port);

    let handler = Arc::new(SqsHandler::new(backend, config));
    let state = SqsServerState { handler };

    let app = Router::new()
        .route("/", post(handle_sqs_request))
        .route("/queue/{queue_name}", post(handle_sqs_request))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    info!(address = %bind_addr, "Starting SQS HTTP server");

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_receiver(shutdown_rx))
        .await?;

    info!("SQS HTTP server shut down gracefully");
    Ok(())
}

/// Handle SQS POST request.
async fn handle_sqs_request(
    State(state): State<SqsServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Log headers for debugging
    info!("Request headers: {:?}", headers);

    // Parse body as UTF-8
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "Failed to parse request body as UTF-8");
            return (
                StatusCode::BAD_REQUEST,
                "Invalid request body encoding",
            )
                .into_response();
        }
    };

    // Parse SQS request (handles both JSON and form-encoded)
    let request = match SqsRequest::parse_with_headers(body_str, &headers) {
        Ok(req) => req,
        Err(e) => {
            error!(error = %e, "Failed to parse SQS request");
            return (StatusCode::BAD_REQUEST, format!("Invalid request: {}", e))
                .into_response();
        }
    };

    // Handle the request
    let (response_body, content_type) = state.handler.handle_request(request).await;

    // Return response with appropriate content type
    (
        StatusCode::OK,
        [("Content-Type", content_type.as_str())],
        response_body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::InMemoryBackend;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Create a test SQS server app with in-memory backend
    fn create_test_app() -> Router {
        let backend = Arc::new(InMemoryBackend::new());
        let config = LclqConfig::default();
        let handler = Arc::new(SqsHandler::new(backend, config));
        let state = SqsServerState { handler };

        Router::new()
            .route("/", post(handle_sqs_request))
            .route("/queue/{queue_name}", post(handle_sqs_request))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_handle_valid_form_encoded_request() {
        let app = create_test_app();

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("Action=ListQueues"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Check content type is XML (default for SQS)
        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(content_type, "application/xml");
    }

    #[tokio::test]
    async fn test_handle_valid_json_request() {
        let app = create_test_app();

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-amz-json-1.0")
            .header("x-amz-target", "AmazonSQS.ListQueues")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_handle_create_queue_request() {
        let app = create_test_app();

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("Action=CreateQueue&QueueName=test-queue"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify response contains queue URL
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        assert!(body_str.contains("QueueUrl"));
        assert!(body_str.contains("test-queue"));
    }

    #[tokio::test]
    async fn test_handle_invalid_utf8_body() {
        let app = create_test_app();

        // Create invalid UTF-8 byte sequence
        let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(invalid_utf8))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        assert!(body_str.contains("Invalid request body encoding"));
    }

    #[tokio::test]
    async fn test_handle_missing_action_parameter() {
        let app = create_test_app();

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("QueueName=test"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        assert!(body_str.contains("Invalid request"));
    }

    #[tokio::test]
    async fn test_handle_invalid_action() {
        let app = create_test_app();

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("Action=InvalidAction"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        assert!(body_str.contains("Invalid request"));
    }

    #[tokio::test]
    async fn test_handle_json_missing_target_header() {
        let app = create_test_app();

        // JSON request without X-Amz-Target header should fail
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-amz-json-1.0")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        assert!(body_str.contains("Invalid request"));
    }

    #[tokio::test]
    async fn test_handle_send_and_receive_message() {
        let app = create_test_app().clone();

        // First, create a queue
        let create_request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("Action=CreateQueue&QueueName=test-queue-msg"))
            .unwrap();

        let create_response = app.clone().oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);

        // Send a message
        let send_request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(
                "Action=SendMessage&QueueUrl=http://localhost:9324/queue/test-queue-msg&MessageBody=Hello"
            ))
            .unwrap();

        let send_response = app.clone().oneshot(send_request).await.unwrap();
        assert_eq!(send_response.status(), StatusCode::OK);

        let send_body = axum::body::to_bytes(send_response.into_body(), usize::MAX).await.unwrap();
        let send_str = std::str::from_utf8(&send_body).unwrap();
        assert!(send_str.contains("MessageId"));

        // Receive the message
        let receive_request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(
                "Action=ReceiveMessage&QueueUrl=http://localhost:9324/queue/test-queue-msg"
            ))
            .unwrap();

        let receive_response = app.oneshot(receive_request).await.unwrap();
        assert_eq!(receive_response.status(), StatusCode::OK);

        let receive_body = axum::body::to_bytes(receive_response.into_body(), usize::MAX).await.unwrap();
        let receive_str = std::str::from_utf8(&receive_body).unwrap();
        assert!(receive_str.contains("Hello"));
    }

    #[tokio::test]
    async fn test_handle_request_with_queue_name_route() {
        let app = create_test_app();

        // Test using the /queue/{queue_name} route
        let request = Request::builder()
            .method("POST")
            .uri("/queue/my-queue")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("Action=ListQueues"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        // Should still work (queue name in URL is informational)
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_sqs_server_state_creation() {
        let backend = Arc::new(InMemoryBackend::new());
        let config = LclqConfig::default();
        let handler = Arc::new(SqsHandler::new(backend, config));

        let state = SqsServerState {
            handler: handler.clone(),
        };

        // Verify state is created correctly
        assert!(Arc::ptr_eq(&state.handler, &handler));
    }

    #[tokio::test]
    async fn test_sqs_server_state_clone() {
        let backend = Arc::new(InMemoryBackend::new());
        let config = LclqConfig::default();
        let handler = Arc::new(SqsHandler::new(backend, config));

        let state1 = SqsServerState {
            handler: handler.clone(),
        };

        let state2 = state1.clone();

        // Verify cloned state shares the same handler
        assert!(Arc::ptr_eq(&state1.handler, &state2.handler));
    }

    #[tokio::test]
    async fn test_start_sqs_server() {
        let backend = Arc::new(InMemoryBackend::new());
        let mut config = LclqConfig::default();
        config.server.sqs_port = 0; // Use random port
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Start server in background
        let server_handle = tokio::spawn(async move {
            start_sqs_server(backend, config, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown
        let _ = shutdown_tx.send(());

        // Wait for server to shutdown gracefully
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            server_handle
        ).await;

        assert!(result.is_ok());
        let server_result = result.unwrap().unwrap();
        assert!(server_result.is_ok());
    }

    #[tokio::test]
    async fn test_start_sqs_server_with_shutdown_signal() {
        use crate::server::shutdown::ShutdownSignal;

        let backend = Arc::new(InMemoryBackend::new());
        let mut config = LclqConfig::default();
        config.server.sqs_port = 0; // Use random port

        let signal = ShutdownSignal::new();
        let shutdown_rx = signal.subscribe();

        // Start server
        let server_handle = tokio::spawn(async move {
            start_sqs_server(backend, config, shutdown_rx).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown via signal
        signal.shutdown();

        // Wait for graceful shutdown
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            server_handle
        ).await;

        assert!(result.is_ok());
    }
}
