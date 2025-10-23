//! SQS HTTP server implementation.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};

use crate::config::LclqConfig;
use crate::sqs::{SqsHandler, SqsRequest};
use crate::storage::StorageBackend;

/// SQS server state.
#[derive(Clone)]
struct SqsServerState {
    handler: Arc<SqsHandler>,
}

/// Start the SQS HTTP server.
pub async fn start_sqs_server(
    backend: Arc<dyn StorageBackend>,
    config: LclqConfig,
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
    axum::serve(listener, app).await?;

    Ok(())
}

/// Handle SQS POST request.
async fn handle_sqs_request(
    State(state): State<SqsServerState>,
    body: Bytes,
) -> Response {
    // Parse form-encoded body
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

    // Parse SQS request
    let request = match SqsRequest::parse(body_str) {
        Ok(req) => req,
        Err(e) => {
            error!(error = %e, "Failed to parse SQS request");
            return (StatusCode::BAD_REQUEST, format!("Invalid request: {}", e))
                .into_response();
        }
    };

    // Handle the request
    let response_xml = state.handler.handle_request(request).await;

    // Return XML response
    (
        StatusCode::OK,
        [("Content-Type", "application/xml")],
        response_xml,
    )
        .into_response()
}
