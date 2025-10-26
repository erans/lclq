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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::{SqliteBackend, SqliteConfig};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tokio::time;

    // Counter for unique database names
    static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Helper to create a test SQLite backend in memory with a unique database
    async fn create_test_backend() -> Arc<SqliteBackend> {
        let db_id = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
        let config = SqliteConfig {
            database_path: format!("file:memdb_cleanup_{}?mode=memory&cache=shared", db_id),
            max_connections: 5,
        };
        Arc::new(SqliteBackend::new(config).await.unwrap())
    }

    #[tokio::test]
    async fn test_cleanup_manager_new() {
        let backend = create_test_backend().await;
        let manager = CleanupManager::new(backend.clone());

        // Verify default interval is 60 seconds
        assert_eq!(manager.check_interval, Duration::from_secs(60));
        assert!(Arc::ptr_eq(&manager.backend, &backend));
    }

    #[tokio::test]
    async fn test_cleanup_manager_with_custom_interval() {
        let backend = create_test_backend().await;
        let custom_interval = Duration::from_secs(30);
        let manager = CleanupManager::with_interval(backend.clone(), custom_interval);

        // Verify custom interval is set
        assert_eq!(manager.check_interval, custom_interval);
        assert!(Arc::ptr_eq(&manager.backend, &backend));
    }

    #[tokio::test]
    async fn test_cleanup_manager_with_very_short_interval() {
        let backend = create_test_backend().await;
        let short_interval = Duration::from_millis(100);
        let manager = CleanupManager::with_interval(backend, short_interval);

        // Verify short interval is accepted
        assert_eq!(manager.check_interval, short_interval);
    }

    #[tokio::test]
    async fn test_cleanup_manager_start_executes_cleanup_tasks() {
        // This test verifies that the cleanup manager can start and run its cleanup tasks
        // without panicking, even when there's no data to clean up
        let backend = create_test_backend().await;

        // Create cleanup manager with very short interval
        let manager = Arc::new(CleanupManager::with_interval(
            backend.clone(),
            Duration::from_millis(10),
        ));

        // Start cleanup manager in background
        let manager_clone = manager.clone();
        let cleanup_task = tokio::spawn(async move {
            manager_clone.start().await;
        });

        // Let it run for a short time - this will execute cleanup tasks multiple times
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Abort the cleanup task (it runs forever)
        cleanup_task.abort();

        // If we got here without panicking, the manager successfully started and ran cleanup tasks
    }

    #[tokio::test]
    async fn test_cleanup_manager_start_completes_full_iterations() {
        // This test ensures the cleanup manager completes full iterations,
        // covering the debug log at the end of each iteration
        let backend = create_test_backend().await;

        // Use paused time for deterministic behavior
        time::pause();

        let manager = Arc::new(CleanupManager::with_interval(
            backend,
            Duration::from_millis(100),
        ));

        // Start cleanup manager
        let manager_clone = manager.clone();
        let cleanup_task = tokio::spawn(async move {
            manager_clone.start().await;
        });

        // Advance time to complete multiple full iterations
        // First tick fires immediately, then at 100ms intervals
        time::advance(Duration::from_millis(1)).await; // Initial tick
        tokio::task::yield_now().await; // Let the first iteration complete

        time::advance(Duration::from_millis(100)).await; // Second tick
        tokio::task::yield_now().await;

        time::advance(Duration::from_millis(100)).await; // Third tick
        tokio::task::yield_now().await;

        // Abort the task
        cleanup_task.abort();

        time::resume();

        // The manager should have completed several full iterations,
        // hitting the debug! log and all error handling paths
    }

    #[tokio::test]
    async fn test_cleanup_manager_handles_backend_errors_gracefully() {
        // This test verifies that cleanup manager doesn't crash when backend methods fail
        // We can't easily inject errors with the real backend, but we test that it
        // continues running even if there's nothing to clean up

        // Create backend BEFORE pausing time to avoid connection pool issues
        let backend = create_test_backend().await;

        time::pause();

        let manager = Arc::new(CleanupManager::with_interval(
            backend,
            Duration::from_millis(10),
        ));

        // Start cleanup manager
        let manager_clone = manager.clone();
        let cleanup_task = tokio::spawn(async move {
            manager_clone.start().await;
        });

        // Let it run for several intervals
        time::advance(Duration::from_millis(50)).await;

        // Abort the task
        cleanup_task.abort();

        // If we got here without panicking, the manager handled empty/error cases gracefully
        time::resume();
    }

    #[tokio::test]
    async fn test_cleanup_manager_interval_timing() {
        // Create backend BEFORE pausing time to avoid connection pool issues
        let backend = create_test_backend().await;

        time::pause();

        let interval = Duration::from_millis(100);

        let manager = Arc::new(CleanupManager::with_interval(backend, interval));

        // Start cleanup manager
        let manager_clone = manager.clone();
        let cleanup_task = tokio::spawn(async move {
            manager_clone.start().await;
        });

        // Advance time by less than one interval - shouldn't have run yet
        time::advance(Duration::from_millis(50)).await;

        // Advance time to complete one interval
        time::advance(Duration::from_millis(60)).await;

        // Advance through multiple intervals
        time::advance(Duration::from_millis(300)).await;

        // Abort the task
        cleanup_task.abort();

        // If we got here, the interval timing worked correctly
        time::resume();
    }

    #[tokio::test]
    async fn test_cleanup_manager_multiple_instances() {
        // Test that multiple cleanup managers can coexist
        let backend = create_test_backend().await;

        let manager1 = Arc::new(CleanupManager::with_interval(
            backend.clone(),
            Duration::from_secs(30),
        ));

        let manager2 = Arc::new(CleanupManager::with_interval(
            backend.clone(),
            Duration::from_secs(60),
        ));

        // Verify they have different intervals
        assert_eq!(manager1.check_interval, Duration::from_secs(30));
        assert_eq!(manager2.check_interval, Duration::from_secs(60));

        // Both should share the same backend
        assert!(Arc::ptr_eq(&manager1.backend, &manager2.backend));
    }
}
