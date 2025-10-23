use std::sync::Arc;

use lclq::config::LclqConfig;
use lclq::sqs::start_sqs_server;
use lclq::storage::memory::InMemoryBackend;
use lclq::storage::StorageBackend;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    info!("Starting lclq - Local Cloud Queue");

    // Load configuration
    let config = LclqConfig::default();

    info!("Configuration loaded successfully");
    info!("SQS port: {}", config.server.sqs_port);
    info!("Pub/Sub gRPC port: {}", config.server.pubsub_grpc_port);
    info!("Pub/Sub HTTP port: {}", config.server.pubsub_http_port);
    info!("Admin port: {}", config.server.admin_port);

    // Create storage backend
    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    info!("In-memory storage backend initialized");

    // Start SQS server
    info!("Starting SQS HTTP server...");
    start_sqs_server(backend, config).await?;

    Ok(())
}
