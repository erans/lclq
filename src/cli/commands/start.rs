// Start command implementation
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::info;

use crate::config::LclqConfig;
use crate::core::cleanup::CleanupManager;
use crate::core::visibility::VisibilityManager;
use crate::pubsub::grpc_server::{GrpcServerConfig, start_grpc_server};
use crate::pubsub::push_queue::DeliveryQueue;
use crate::pubsub::push_worker::PushWorkerPool;
use crate::pubsub::rest::{RestServerConfig, start_rest_server};
use crate::server::admin::start_admin_server;
use crate::server::metrics::start_metrics_server;
use crate::server::shutdown::{ShutdownSignal, shutdown_with_timeout};
use crate::sqs::start_sqs_server;
use crate::storage::StorageBackend;
use crate::storage::memory::InMemoryBackend;
use crate::storage::sqlite::{SqliteBackend, SqliteConfig};

/// Execute the start command - launches the lclq server with all services
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    sqs_port: u16,
    pubsub_port: u16,
    pubsub_rest_port: u16,
    admin_port: u16,
    metrics_port: u16,
    bind_address: String,
    backend: String,
    db_path: String,
    disable_sqs: bool,
    disable_pubsub: bool,
) -> Result<()> {
    info!("Starting lclq - Local Cloud Queue");

    // Check if both services are disabled
    if disable_sqs && disable_pubsub {
        anyhow::bail!(
            "Cannot start lclq with both SQS and Pub/Sub disabled. Enable at least one service."
        );
    }

    // Load configuration
    let mut config = LclqConfig::default();

    // Override configuration with CLI arguments
    config.server.sqs_port = sqs_port;
    config.server.admin_port = admin_port;
    config.server.bind_address = bind_address.clone();

    info!("Configuration loaded successfully");
    if !disable_sqs {
        info!("SQS port: {}", config.server.sqs_port);
    } else {
        info!("SQS: disabled");
    }
    if !disable_pubsub {
        info!("Pub/Sub gRPC port: {}", pubsub_port);
        info!("Pub/Sub REST port: {}", pubsub_rest_port);
    } else {
        info!("Pub/Sub: disabled");
    }
    info!("Admin port: {}", config.server.admin_port);
    info!("Metrics port: {}", metrics_port);
    info!("Bind address: {}", config.server.bind_address);

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

        // Start visibility manager for SQLite
        let visibility_manager = Arc::new(VisibilityManager::new(sqlite_backend.clone()));
        tokio::spawn(async move {
            visibility_manager.start().await;
        });
        info!("Visibility timeout manager started for SQLite backend");

        sqlite_backend as Arc<dyn StorageBackend>
    } else {
        let memory_backend = Arc::new(InMemoryBackend::new());
        info!("In-Memory storage backend initialized");

        // Start visibility manager for in-memory backend
        let visibility_manager = Arc::new(VisibilityManager::new(memory_backend.clone()));
        tokio::spawn(async move {
            visibility_manager.start().await;
        });
        info!("Visibility timeout manager started for in-memory backend");

        memory_backend as Arc<dyn StorageBackend>
    };

    info!(
        backend_type = if use_sqlite { "SQLite" } else { "In-Memory" },
        "Storage backend initialized"
    );

    // Clone backend for servers
    let admin_backend = storage_backend.clone();
    let sqs_backend = storage_backend.clone();
    let pubsub_grpc_backend = storage_backend.clone();
    let pubsub_rest_backend = storage_backend.clone();

    // Collect all server handles
    let mut server_handles = Vec::new();

    // Start Admin API server in background
    let admin_shutdown_rx = shutdown_signal.subscribe();
    let admin_bind_address = bind_address.clone();
    let admin_handle = tokio::spawn(async move {
        if let Err(e) = start_admin_server(
            admin_backend,
            admin_bind_address,
            admin_port,
            admin_shutdown_rx,
        )
        .await
        {
            tracing::error!("Admin API server error: {}", e);
        }
    });
    server_handles.push(admin_handle);

    // Start Metrics server in background
    let metrics_shutdown_rx = shutdown_signal.subscribe();
    let metrics_bind_address = bind_address.clone();
    let metrics_handle = tokio::spawn(async move {
        if let Err(e) =
            start_metrics_server(metrics_bind_address, metrics_port, metrics_shutdown_rx).await
        {
            tracing::error!("Metrics server error: {}", e);
        }
    });
    server_handles.push(metrics_handle);

    // Conditionally start SQS server
    if !disable_sqs {
        let sqs_shutdown_rx = shutdown_signal.subscribe();
        let sqs_handle = tokio::spawn(async move {
            if let Err(e) = start_sqs_server(sqs_backend, config, sqs_shutdown_rx).await {
                tracing::error!("SQS HTTP server error: {}", e);
            }
        });
        server_handles.push(sqs_handle);
    }

    // Conditionally start Pub/Sub servers
    let mut push_worker_pool: Option<PushWorkerPool> = None;
    if !disable_pubsub {
        // Create shared delivery queue for push subscriptions
        let delivery_queue = DeliveryQueue::new();

        // Create push worker pool for HTTP delivery (shared between gRPC and REST)
        let worker_pool = PushWorkerPool::new(
            None, // Use default worker count (2, or LCLQ_PUSH_WORKERS env var)
            delivery_queue.clone(),
            storage_backend.clone(),
        );
        push_worker_pool = Some(worker_pool);

        // Start Pub/Sub gRPC server in background
        let pubsub_grpc_shutdown_rx = shutdown_signal.subscribe();
        let pubsub_grpc_bind_address = bind_address.clone();
        let grpc_delivery_queue = delivery_queue.clone();
        let pubsub_grpc_handle = tokio::spawn(async move {
            let grpc_config = GrpcServerConfig {
                bind_address: format!("{}:{}", pubsub_grpc_bind_address, pubsub_port),
            };
            if let Err(e) = start_grpc_server(
                grpc_config,
                pubsub_grpc_backend,
                Some(grpc_delivery_queue),
                pubsub_grpc_shutdown_rx,
            )
            .await
            {
                tracing::error!("Pub/Sub gRPC server error: {}", e);
            }
        });
        server_handles.push(pubsub_grpc_handle);

        // Start Pub/Sub REST server in background
        let pubsub_rest_shutdown_rx = shutdown_signal.subscribe();
        let pubsub_rest_bind_address = bind_address.clone();
        let rest_delivery_queue = delivery_queue.clone();
        let pubsub_rest_handle = tokio::spawn(async move {
            let rest_config = RestServerConfig {
                bind_address: format!("{}:{}", pubsub_rest_bind_address, pubsub_rest_port),
            };
            if let Err(e) = start_rest_server(
                rest_config,
                pubsub_rest_backend,
                Some(rest_delivery_queue),
                pubsub_rest_shutdown_rx,
            )
            .await
            {
                tracing::error!("Pub/Sub REST server error: {}", e);
            }
        });
        server_handles.push(pubsub_rest_handle);
    }

    info!("All enabled servers started successfully");

    // Wait for shutdown signal and coordinate graceful shutdown
    shutdown_with_timeout(shutdown_signal, Duration::from_secs(30)).await?;

    // Wait for all servers to complete shutdown
    for handle in server_handles {
        let _ = handle.await;
    }

    // Shutdown push worker pool gracefully
    if let Some(pool) = push_worker_pool {
        pool.shutdown().await;
    }

    info!("All servers shut down. Exiting.");
    Ok(())
}
