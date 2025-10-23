//! Background cleanup task manager for SQLite backend.

use std::sync::Arc;
use std::time::Duration;

use tokio::time;
use tracing::{debug, error, info};

use crate::storage::sqlite::SqliteBackend;

/// Cleanup manager that handles background maintenance tasks for SQLite.
pub struct CleanupManager {
    backend: Arc<SqliteBackend>,
    check_interval: Duration,
}

impl CleanupManager {
    /// Create a new cleanup manager.
    pub fn new(backend: Arc<SqliteBackend>) -> Self {
        Self::with_interval(backend, Duration::from_secs(60))
    }

    /// Create a cleanup manager with custom check interval.
    pub fn with_interval(backend: Arc<SqliteBackend>, check_interval: Duration) -> Self {
        Self {
            backend,
            check_interval,
        }
    }

    /// Start the background cleanup tasks.
    pub async fn start(self: Arc<Self>) {
        info!(
            interval_secs = self.check_interval.as_secs(),
            "Starting cleanup manager for SQLite backend"
        );

        let mut interval = time::interval(self.check_interval);

        loop {
            interval.tick().await;

            // Process expired visibility timeouts
            if let Err(e) = self.backend.process_expired_visibility().await {
                error!(error = %e, "Error processing expired visibility timeouts");
            }

            // Clean up deduplication cache
            if let Err(e) = self.backend.cleanup_deduplication_cache().await {
                error!(error = %e, "Error cleaning up deduplication cache");
            }

            // Delete expired messages past retention period
            if let Err(e) = self.backend.delete_expired_messages().await {
                error!(error = %e, "Error deleting expired messages");
            }

            debug!("Cleanup tasks completed");
        }
    }
}
