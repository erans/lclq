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
}

