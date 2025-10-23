use lclq::config::LclqConfig;
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

    // TODO: Start servers
    info!("lclq is ready!");

    Ok(())
}
