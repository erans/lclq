//! gRPC server implementation for Pub/Sub.
//!
//! Sets up the Tonic gRPC server with Publisher and Subscriber services.

use crate::error::Result;
use crate::pubsub::proto::publisher_server::PublisherServer;
use crate::pubsub::proto::subscriber_server::SubscriberServer;
use crate::pubsub::publisher::PublisherService;
use crate::pubsub::subscriber::SubscriberService;
use crate::storage::StorageBackend;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;

/// gRPC server configuration.
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    /// Address to bind to (e.g., "127.0.0.1:8085").
    pub bind_address: String,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8085".to_string(),
        }
    }
}

/// Start the Pub/Sub gRPC server.
///
/// This creates a Tonic gRPC server that hosts both Publisher and Subscriber services.
/// The server will run until a shutdown signal is received.
pub async fn start_grpc_server(
    config: GrpcServerConfig,
    backend: Arc<dyn StorageBackend>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let addr = config
        .bind_address
        .parse()
        .map_err(|e| crate::error::Error::Config(format!("Invalid bind address: {}", e)))?;

    info!("Starting Pub/Sub gRPC server on {}", addr);

    // Create service instances
    let publisher = PublisherService::new(backend.clone());
    let subscriber = SubscriberService::new(backend);

    // Build the gRPC server
    let server = Server::builder()
        .add_service(PublisherServer::new(publisher))
        .add_service(SubscriberServer::new(subscriber))
        .serve_with_shutdown(addr, async {
            let _ = shutdown.resubscribe().recv().await;
            info!("Pub/Sub gRPC server shutting down gracefully");
        });

    // Run the server
    server
        .await
        .map_err(|e| crate::error::Error::Internal(format!("gRPC server error: {}", e)))?;

    info!("Pub/Sub gRPC server stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::InMemoryBackend;

    #[test]
    fn test_default_config() {
        let config = GrpcServerConfig::default();
        assert_eq!(config.bind_address, "127.0.0.1:8085");
    }

    #[test]
    fn test_config_parsing() {
        let config = GrpcServerConfig {
            bind_address: "0.0.0.0:9090".to_string(),
        };
        assert!(config.bind_address.parse::<std::net::SocketAddr>().is_ok());
    }

    #[test]
    fn test_config_clone() {
        let config = GrpcServerConfig::default();
        let cloned = config.clone();
        assert_eq!(config.bind_address, cloned.bind_address);
    }

    #[test]
    fn test_custom_config() {
        let config = GrpcServerConfig {
            bind_address: "[::1]:8086".to_string(),
        };
        assert_eq!(config.bind_address, "[::1]:8086");
    }

    #[tokio::test]
    async fn test_invalid_bind_address() {
        let config = GrpcServerConfig {
            bind_address: "invalid:address:format".to_string(),
        };
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let result = start_grpc_server(config, backend, shutdown_rx).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::Error::Config(_)
        ));
    }

    #[tokio::test]
    async fn test_start_grpc_server() {
        let config = GrpcServerConfig {
            bind_address: "127.0.0.1:0".to_string(), // Port 0 = random available port
        };
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        // Start server in background
        let server_handle =
            tokio::spawn(async move { start_grpc_server(config, backend, shutdown_rx).await });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown
        let _ = shutdown_tx.send(());

        // Wait for server to shutdown gracefully
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(5), server_handle).await;

        assert!(result.is_ok());
        let server_result = result.unwrap().unwrap();
        assert!(server_result.is_ok());
    }

    #[tokio::test]
    async fn test_grpc_server_with_shutdown_signal() {
        use crate::server::shutdown::ShutdownSignal;

        let config = GrpcServerConfig {
            bind_address: "127.0.0.1:0".to_string(),
        };
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let signal = ShutdownSignal::new();
        let shutdown_rx = signal.subscribe();

        // Start server
        let server_handle =
            tokio::spawn(async move { start_grpc_server(config, backend, shutdown_rx).await });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Trigger shutdown via signal
        signal.shutdown();

        // Wait for graceful shutdown
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(5), server_handle).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_grpc_server_creates_services() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Test that services can be created
        let publisher = PublisherService::new(backend.clone());
        let subscriber = SubscriberService::new(backend);

        // Services should be created successfully (no panics)
        drop(publisher);
        drop(subscriber);
    }
}
