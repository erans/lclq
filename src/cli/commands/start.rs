// Start command implementation
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;

use crate::config::LclqConfig;
use crate::core::cleanup::CleanupManager;
use crate::server::admin::start_admin_server;
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

    // Clone backend for admin server
    let admin_backend = storage_backend.clone();

    // Start Admin API server in background
    let admin_handle = tokio::spawn(async move {
        if let Err(e) = start_admin_server(admin_backend, admin_port).await {
            tracing::error!("Admin API server error: {}", e);
        }
    });

    // TODO: Start Metrics server on metrics_port

    // Start SQS server (blocks until server stops)
    info!("Starting SQS HTTP server on port {}...", sqs_port);
    let sqs_result = start_sqs_server(storage_backend, config).await;

    // If SQS server stops, abort admin server
    admin_handle.abort();

    sqs_result
}
