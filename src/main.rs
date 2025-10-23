use std::sync::Arc;

use lclq::config::LclqConfig;
use lclq::core::cleanup::CleanupManager;
use lclq::sqs::start_sqs_server;
use lclq::storage::memory::InMemoryBackend;
use lclq::storage::sqlite::{SqliteBackend, SqliteConfig};
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

    // Determine backend from environment or use in-memory by default
    let use_sqlite = std::env::var("LCLQ_BACKEND")
        .unwrap_or_default()
        .eq_ignore_ascii_case("sqlite");

    let backend: Arc<dyn StorageBackend> = if use_sqlite {
        let sqlite_config = SqliteConfig {
            database_path: std::env::var("LCLQ_DB_PATH").unwrap_or_else(|_| "lclq.db".to_string()),
            max_connections: 10,
        };

        let sqlite_backend = Arc::new(SqliteBackend::new(sqlite_config).await?);
        info!("SQLite storage backend initialized");

        // Start cleanup manager for SQLite
        let cleanup_manager = Arc::new(CleanupManager::new(sqlite_backend.clone()));
        tokio::spawn(async move {
            cleanup_manager.start().await;
        });
        info!("Cleanup manager started for SQLite backend");

        sqlite_backend as Arc<dyn StorageBackend>
    } else {
        Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>
    };

    info!(
        backend_type = if use_sqlite { "SQLite" } else { "In-Memory" },
        "Storage backend initialized"
    );

    // Start SQS server
    info!("Starting SQS HTTP server...");
    start_sqs_server(backend, config).await?;

    Ok(())
}
