//! Configuration system for lclq.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LclqConfig {
    /// Server configuration.
    pub server: ServerConfig,
    /// Storage configuration.
    pub storage: StorageConfig,
    /// SQS configuration.
    pub sqs: SqsConfig,
    /// Pub/Sub configuration.
    pub pubsub: PubsubConfig,
    /// Logging configuration.
    pub logging: LoggingConfig,
    /// Metrics configuration.
    pub metrics: MetricsConfig,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// SQS HTTP port.
    pub sqs_port: u16,
    /// Pub/Sub gRPC port.
    pub pubsub_grpc_port: u16,
    /// Pub/Sub HTTP port.
    pub pubsub_http_port: u16,
    /// Admin API port.
    pub admin_port: u16,
    /// Bind address.
    pub bind_address: String,
}

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Backend type.
    pub backend: BackendConfig,
}

/// Backend configuration enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BackendConfig {
    /// In-memory backend.
    InMemory {
        /// Maximum number of messages.
        max_messages: usize,
        /// Eviction policy.
        eviction_policy: EvictionPolicy,
    },
    /// SQLite backend.
    Sqlite {
        /// Database file path.
        database_path: String,
    },
}

/// Eviction policy for in-memory backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least recently used.
    Lru,
    /// First in first out.
    Fifo,
    /// Reject new messages.
    RejectNew,
}

/// SQS-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsConfig {
    /// Enable AWS Signature V4 verification.
    pub enable_signature_verification: bool,
}

/// Pub/Sub-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubsubConfig {
    /// Default project ID.
    pub default_project_id: String,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level.
    pub level: String,
    /// Log format (text or json).
    pub format: LogFormat,
}

/// Log format enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Plain text format.
    Text,
    /// JSON format.
    Json,
}

/// Metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable Prometheus metrics.
    pub enabled: bool,
    /// Metrics port.
    pub port: u16,
}

impl Default for LclqConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                sqs_port: 9324,
                pubsub_grpc_port: 8085,
                pubsub_http_port: 8086,
                admin_port: 9000,
                bind_address: "127.0.0.1".to_string(),
            },
            storage: StorageConfig {
                backend: BackendConfig::InMemory {
                    max_messages: 100_000,
                    eviction_policy: EvictionPolicy::Lru,
                },
            },
            sqs: SqsConfig {
                enable_signature_verification: false,
            },
            pubsub: PubsubConfig {
                default_project_id: "local-project".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Text,
            },
            metrics: MetricsConfig {
                enabled: true,
                port: 9090,
            },
        }
    }
}

impl LclqConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(_path: &Path) -> crate::Result<Self> {
        // TODO: Implement TOML file loading
        Ok(Self::default())
    }

    /// Validate the configuration.
    pub fn validate(&self) -> crate::Result<()> {
        // TODO: Implement validation
        Ok(())
    }
}
