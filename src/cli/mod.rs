// CLI module for lclq
/// Command execution handlers
pub mod commands;
/// Output formatting utilities
pub mod output;

use clap::{Parser, Subcommand};

/// Command-line interface for lclq
#[derive(Parser)]
#[command(name = "lclq")]
#[command(author, version, about = "Local Cloud Queue - AWS SQS & GCP Pub/Sub compatible queue service", long_about = None)]
pub struct Cli {
    /// The command to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand)]
pub enum Commands {
    /// Start the lclq server
    Start {
        /// SQS HTTP port
        #[arg(long, env = "LCLQ_SQS_PORT", default_value = "9324")]
        sqs_port: u16,

        /// Pub/Sub gRPC port
        #[arg(long, env = "LCLQ_PUBSUB_PORT", default_value = "8085")]
        pubsub_port: u16,

        /// Admin API port
        #[arg(long, env = "LCLQ_ADMIN_PORT", default_value = "9000")]
        admin_port: u16,

        /// Metrics port
        #[arg(long, env = "LCLQ_METRICS_PORT", default_value = "9090")]
        metrics_port: u16,

        /// Bind address (use 0.0.0.0 for all interfaces)
        #[arg(long, env = "LCLQ_BIND_ADDRESS", default_value = "127.0.0.1")]
        bind_address: String,

        /// Storage backend (memory | sqlite)
        #[arg(long, env = "LCLQ_BACKEND", default_value = "memory")]
        backend: String,

        /// SQLite database path (only used with sqlite backend)
        #[arg(long, env = "LCLQ_DB_PATH", default_value = "lclq.db")]
        db_path: String,
    },

    /// Queue management commands
    #[command(subcommand)]
    Queue(QueueCommands),

    /// Health check
    Health {
        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,
    },

    /// Show overall statistics
    Stats {
        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,
    },

    /// Show current configuration
    Config,
}

/// Queue management subcommands
#[derive(Subcommand)]
pub enum QueueCommands {
    /// List all queues
    List {
        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,

        /// Output format (table | json)
        #[arg(long, short, default_value = "table")]
        format: String,
    },

    /// Create a new queue
    Create {
        /// Queue name
        name: String,

        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,
    },

    /// Delete a queue
    Delete {
        /// Queue name
        name: String,

        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,
    },

    /// Purge all messages from a queue
    Purge {
        /// Queue name
        name: String,

        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,
    },

    /// Show queue statistics
    Stats {
        /// Queue name
        name: String,

        /// Admin API URL
        #[arg(long, env = "LCLQ_ADMIN_URL", default_value = "http://localhost:9000")]
        admin_url: String,
    },
}

impl Cli {
    /// Parse command-line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
