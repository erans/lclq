// Start command implementation
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;

use crate::config::LclqConfig;
use crate::core::cleanup::CleanupManager;
use crate::sqs::start_sqs_server;
use crate::storage::memory::InMemoryBackend;
use crate::storage::sqlite::{SqliteBackend, SqliteConfig};
use crate::storage::StorageBackend;

/// Execute the start command - launches the lclq server with all services
pub async fn execute(
    sqs_port: u16,
    admin_port: u16,
    metrics_port: u16,
    backend: String,
    db_path: String,
) -> Result<()> {
    info!("Starting lclq - Local Cloud Queue");

    // Load configuration
    let mut config = LclqConfig::default();

    // Override configuration with CLI arguments
    config.server.sqs_port = sqs_port;
    config.server.admin_port = admin_port;
    // Note: metrics_port will be used when we implement the metrics server

    info!("Configuration loaded successfully");
    info!("SQS port: {}", config.server.sqs_port);
    info!("Admin port: {}", config.server.admin_port);
    info!("Metrics port: {}", metrics_port);

    // Determine backend
    let use_sqlite = backend.eq_ignore_ascii_case("sqlite");

    let storage_backend: Arc<dyn StorageBackend> = if use_sqlite {
        let sqlite_config = SqliteConfig {
            database_path: db_path.clone(),
            max_connections: 10,
        };

        let sqlite_backend = Arc::new(
            SqliteBackend::new(sqlite_config)
                .await
                .context("Failed to initialize SQLite backend")?,
        );
        info!("SQLite storage backend initialized at {}", db_path);

        // Start cleanup manager for SQLite
        let cleanup_manager = Arc::new(CleanupManager::new(sqlite_backend.clone()));
        tokio::spawn(async move {
            cleanup_manager.start().await;
        });
        info!("Cleanup manager started for SQLite backend");

        sqlite_backend as Arc<dyn StorageBackend>
    } else {
        info!("In-Memory storage backend initialized");
        Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>
    };

    info!(
        backend_type = if use_sqlite { "SQLite" } else { "In-Memory" },
        "Storage backend initialized"
    );

    // TODO: Start Admin API server on admin_port
    // TODO: Start Metrics server on metrics_port

    // Start SQS server
    info!("Starting SQS HTTP server on port {}...", sqs_port);
    start_sqs_server(storage_backend, config).await?;

    Ok(())
}
