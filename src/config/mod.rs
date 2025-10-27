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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ========================================================================
    // Default Configuration Tests
    // ========================================================================

    #[test]
    fn test_default_config_creation() {
        let config = LclqConfig::default();

        // Verify server defaults
        assert_eq!(config.server.sqs_port, 9324);
        assert_eq!(config.server.pubsub_grpc_port, 8085);
        assert_eq!(config.server.pubsub_http_port, 8086);
        assert_eq!(config.server.admin_port, 9000);
        assert_eq!(config.server.bind_address, "127.0.0.1");

        // Verify storage defaults
        match &config.storage.backend {
            BackendConfig::InMemory {
                max_messages,
                eviction_policy,
            } => {
                assert_eq!(*max_messages, 100_000);
                assert!(matches!(eviction_policy, EvictionPolicy::Lru));
            }
            _ => panic!("Expected InMemory backend"),
        }

        // Verify SQS defaults
        assert!(!config.sqs.enable_signature_verification);

        // Verify Pub/Sub defaults
        assert_eq!(config.pubsub.default_project_id, "local-project");

        // Verify logging defaults
        assert_eq!(config.logging.level, "info");
        assert!(matches!(config.logging.format, LogFormat::Text));

        // Verify metrics defaults
        assert!(config.metrics.enabled);
        assert_eq!(config.metrics.port, 9090);
    }

    // ========================================================================
    // Serialization/Deserialization Tests
    // ========================================================================

    #[test]
    fn test_serialize_deserialize_default_config() {
        let config = LclqConfig::default();

        // Serialize to JSON
        let json = serde_json::to_string(&config).unwrap();

        // Deserialize back
        let deserialized: LclqConfig = serde_json::from_str(&json).unwrap();

        // Verify round-trip
        assert_eq!(deserialized.server.sqs_port, config.server.sqs_port);
        assert_eq!(deserialized.server.bind_address, config.server.bind_address);
    }

    #[test]
    fn test_serialize_inmemory_backend() {
        let backend = BackendConfig::InMemory {
            max_messages: 50000,
            eviction_policy: EvictionPolicy::Fifo,
        };

        let json = serde_json::to_string(&backend).unwrap();
        assert!(json.contains("InMemory"));
        assert!(json.contains("50000"));

        let deserialized: BackendConfig = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendConfig::InMemory {
                max_messages,
                eviction_policy,
            } => {
                assert_eq!(max_messages, 50000);
                assert!(matches!(eviction_policy, EvictionPolicy::Fifo));
            }
            _ => panic!("Expected InMemory backend"),
        }
    }

    #[test]
    fn test_serialize_sqlite_backend() {
        let backend = BackendConfig::Sqlite {
            database_path: "/tmp/test.db".to_string(),
        };

        let json = serde_json::to_string(&backend).unwrap();
        assert!(json.contains("Sqlite"));
        assert!(json.contains("/tmp/test.db"));

        let deserialized: BackendConfig = serde_json::from_str(&json).unwrap();
        match deserialized {
            BackendConfig::Sqlite { database_path } => {
                assert_eq!(database_path, "/tmp/test.db");
            }
            _ => panic!("Expected Sqlite backend"),
        }
    }

    // ========================================================================
    // Eviction Policy Tests
    // ========================================================================

    #[test]
    fn test_eviction_policy_lru() {
        let policy = EvictionPolicy::Lru;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, r#""Lru""#);

        let deserialized: EvictionPolicy = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, EvictionPolicy::Lru));
    }

    #[test]
    fn test_eviction_policy_fifo() {
        let policy = EvictionPolicy::Fifo;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, r#""Fifo""#);

        let deserialized: EvictionPolicy = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, EvictionPolicy::Fifo));
    }

    #[test]
    fn test_eviction_policy_reject_new() {
        let policy = EvictionPolicy::RejectNew;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, r#""RejectNew""#);

        let deserialized: EvictionPolicy = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, EvictionPolicy::RejectNew));
    }

    // ========================================================================
    // Log Format Tests
    // ========================================================================

    #[test]
    fn test_log_format_text() {
        let format = LogFormat::Text;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, r#""text""#);

        let deserialized: LogFormat = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, LogFormat::Text));
    }

    #[test]
    fn test_log_format_json() {
        let format = LogFormat::Json;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, r#""json""#);

        let deserialized: LogFormat = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, LogFormat::Json));
    }

    // ========================================================================
    // Config Methods Tests
    // ========================================================================

    #[test]
    fn test_from_file_returns_default() {
        let path = PathBuf::from("/tmp/nonexistent.toml");
        let config = LclqConfig::from_file(&path).unwrap();

        // Should return default config (stub implementation)
        assert_eq!(config.server.sqs_port, 9324);
        assert_eq!(config.pubsub.default_project_id, "local-project");
    }

    #[test]
    fn test_validate_succeeds() {
        let config = LclqConfig::default();
        let result = config.validate();

        // Should succeed (stub implementation)
        assert!(result.is_ok());
    }

    // ========================================================================
    // Custom Configuration Tests
    // ========================================================================

    #[test]
    fn test_custom_server_config() {
        let server = ServerConfig {
            sqs_port: 8000,
            pubsub_grpc_port: 8001,
            pubsub_http_port: 8002,
            admin_port: 8003,
            bind_address: "0.0.0.0".to_string(),
        };

        assert_eq!(server.sqs_port, 8000);
        assert_eq!(server.pubsub_grpc_port, 8001);
        assert_eq!(server.pubsub_http_port, 8002);
        assert_eq!(server.admin_port, 8003);
        assert_eq!(server.bind_address, "0.0.0.0");
    }

    #[test]
    fn test_custom_sqs_config() {
        let sqs = SqsConfig {
            enable_signature_verification: true,
        };

        assert!(sqs.enable_signature_verification);
    }

    #[test]
    fn test_custom_pubsub_config() {
        let pubsub = PubsubConfig {
            default_project_id: "my-project".to_string(),
        };

        assert_eq!(pubsub.default_project_id, "my-project");
    }

    #[test]
    fn test_custom_logging_config() {
        let logging = LoggingConfig {
            level: "debug".to_string(),
            format: LogFormat::Json,
        };

        assert_eq!(logging.level, "debug");
        assert!(matches!(logging.format, LogFormat::Json));
    }

    #[test]
    fn test_custom_metrics_config() {
        let metrics = MetricsConfig {
            enabled: false,
            port: 8080,
        };

        assert!(!metrics.enabled);
        assert_eq!(metrics.port, 8080);
    }

    // ========================================================================
    // Full Custom Configuration Test
    // ========================================================================

    #[test]
    fn test_full_custom_config() {
        let config = LclqConfig {
            server: ServerConfig {
                sqs_port: 8000,
                pubsub_grpc_port: 8001,
                pubsub_http_port: 8002,
                admin_port: 8003,
                bind_address: "0.0.0.0".to_string(),
            },
            storage: StorageConfig {
                backend: BackendConfig::Sqlite {
                    database_path: "/var/lib/lclq/db.sqlite".to_string(),
                },
            },
            sqs: SqsConfig {
                enable_signature_verification: true,
            },
            pubsub: PubsubConfig {
                default_project_id: "production".to_string(),
            },
            logging: LoggingConfig {
                level: "warn".to_string(),
                format: LogFormat::Json,
            },
            metrics: MetricsConfig {
                enabled: false,
                port: 8080,
            },
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LclqConfig = serde_json::from_str(&json).unwrap();

        // Verify all fields
        assert_eq!(deserialized.server.sqs_port, 8000);
        assert_eq!(deserialized.server.bind_address, "0.0.0.0");
        assert!(deserialized.sqs.enable_signature_verification);
        assert_eq!(deserialized.pubsub.default_project_id, "production");
        assert_eq!(deserialized.logging.level, "warn");
        assert!(!deserialized.metrics.enabled);

        match &deserialized.storage.backend {
            BackendConfig::Sqlite { database_path } => {
                assert_eq!(database_path, "/var/lib/lclq/db.sqlite");
            }
            _ => panic!("Expected Sqlite backend"),
        }
    }
}
