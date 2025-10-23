// Start command implementation
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::info;

use crate::config::LclqConfig;
use crate::core::cleanup::CleanupManager;
use crate::server::admin::start_admin_server;
use crate::server::metrics::start_metrics_server;
use crate::server::shutdown::{shutdown_with_timeout, ShutdownSignal};
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

    info!("Configuration loaded successfully");
    info!("SQS port: {}", config.server.sqs_port);
    info!("Admin port: {}", config.server.admin_port);
    info!("Metrics port: {}", metrics_port);

    // Create shutdown signal for coordinating graceful shutdown
    let shutdown_signal = ShutdownSignal::new();

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

    // Clone backend for servers
    let admin_backend = storage_backend.clone();
    let sqs_backend = storage_backend.clone();

    // Start Admin API server in background
    let admin_shutdown_rx = shutdown_signal.subscribe();
    let admin_handle = tokio::spawn(async move {
        if let Err(e) = start_admin_server(admin_backend, admin_port, admin_shutdown_rx).await {
            tracing::error!("Admin API server error: {}", e);
        }
    });

    // Start Metrics server in background
    let metrics_shutdown_rx = shutdown_signal.subscribe();
    let metrics_handle = tokio::spawn(async move {
        if let Err(e) = start_metrics_server(metrics_port, metrics_shutdown_rx).await {
            tracing::error!("Metrics server error: {}", e);
        }
    });

    // Start SQS server in background
    let sqs_shutdown_rx = shutdown_signal.subscribe();
    let sqs_handle = tokio::spawn(async move {
        if let Err(e) = start_sqs_server(sqs_backend, config, sqs_shutdown_rx).await {
            tracing::error!("SQS HTTP server error: {}", e);
        }
    });

    info!("All servers started successfully");

    // Wait for shutdown signal and coordinate graceful shutdown
    shutdown_with_timeout(shutdown_signal, Duration::from_secs(30)).await?;

    // Wait for all servers to complete shutdown
    let _ = tokio::join!(admin_handle, metrics_handle, sqs_handle);

    info!("All servers shut down. Exiting.");
    Ok(())
}
