//! gRPC server implementation for Pub/Sub.
//!
//! Sets up the Tonic gRPC server with Publisher and Subscriber services.

use crate::error::Result;
use crate::storage::StorageBackend;
use std::sync::Arc;

/// gRPC server configuration.
pub struct GrpcServerConfig {
    /// Address to bind to (e.g., "127.0.0.1:8085").
    pub bind_address: String,
}

/// Start the Pub/Sub gRPC server.
pub async fn start_grpc_server(
    _config: GrpcServerConfig,
    _backend: Arc<dyn StorageBackend>,
) -> Result<()> {
    // TODO: Implement gRPC server
    Ok(())
}
