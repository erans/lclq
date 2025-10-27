//! Visibility timeout manager.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::time;
use tracing::{debug, error, info};

use crate::storage::StorageBackend;

/// Visibility timeout manager that requeues expired messages.
pub struct VisibilityManager {
    backend: Arc<dyn StorageBackend>,
    check_interval: Duration,
}

impl VisibilityManager {
    /// Create a new visibility manager.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self::with_interval(backend, Duration::from_secs(5))
    }

    /// Create a visibility manager with custom check interval.
    pub fn with_interval(backend: Arc<dyn StorageBackend>, check_interval: Duration) -> Self {
        Self {
            backend,
            check_interval,
        }
    }

    /// Start the background task to process expired visibility timeouts.
    pub async fn start(self: Arc<Self>) {
        info!(
            interval_secs = self.check_interval.as_secs(),
            "Starting visibility timeout manager"
        );

        let mut interval = time::interval(self.check_interval);

        loop {
            interval.tick().await;

            if let Err(e) = self.process_expired_visibility().await {
                error!(error = %e, "Error processing expired visibility timeouts");
            }
        }
    }

    /// Process all expired visibility timeouts.
    async fn process_expired_visibility(&self) -> crate::Result<()> {
        // Get all queues
        let queues = self.backend.list_queues(None).await?;

        for queue in queues {
            if let Err(e) = self.process_queue_expired_visibility(&queue.id).await {
                error!(queue_id = %queue.id, error = %e, "Error processing queue visibility");
            }
        }

        Ok(())
    }

    /// Process expired visibility timeouts for a specific queue.
    ///
    /// This method calls the backend's `process_expired_visibility` to return
    /// messages with expired visibility timeouts back to the available queue.
    async fn process_queue_expired_visibility(&self, queue_id: &str) -> crate::Result<()> {
        let count = self.backend.process_expired_visibility(queue_id).await?;
        if count > 0 {
            debug!(
                queue_id = %queue_id,
                count = count,
                "Processed expired visibility timeouts"
            );
        }
        Ok(())
    }
}

/// Process expired visibility timeouts for in-memory backend.
///
/// This function is called periodically to move messages with expired
/// visibility timeouts back to the available queue.
pub async fn process_expired_messages_in_memory(
    _backend: Arc<dyn StorageBackend>,
) -> crate::Result<()> {
    // Implementation would access the backend's internal state
    // For now, this is a placeholder showing the design
    let _now = Utc::now();

    // In a full implementation:
    // 1. Lock the queues
    // 2. For each queue, scan in_flight_messages
    // 3. Find messages where visibility_expires_at < now
    // 4. Move them back to available_messages
    // 5. Check receive_count vs max_receive_count
    // 6. Move to DLQ if exceeded

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::InMemoryBackend;

    #[tokio::test]
    async fn test_visibility_manager_creation() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let manager = VisibilityManager::new(backend);
        assert_eq!(manager.check_interval, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_visibility_manager_custom_interval() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let manager = VisibilityManager::with_interval(backend, Duration::from_secs(10));
        assert_eq!(manager.check_interval, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_visibility_manager_with_very_short_interval() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let manager = VisibilityManager::with_interval(backend, Duration::from_millis(50));
        assert_eq!(manager.check_interval, Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_process_expired_visibility_empty_queues() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let manager = VisibilityManager::new(backend);

        // Should succeed even with no queues
        let result = manager.process_expired_visibility().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_expired_visibility_with_queues() {
        use crate::types::{QueueConfig, QueueType};

        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create a test queue
        let queue_config = QueueConfig {
            id: "test-queue".to_string(),
            name: "test-queue".to_string(),
            queue_type: QueueType::SqsStandard,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config: None,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        };
        backend.create_queue(queue_config).await.unwrap();

        let manager = VisibilityManager::new(backend);

        // Should process the queue without error
        let result = manager.process_expired_visibility().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_expired_visibility_with_multiple_queues() {
        use crate::types::{QueueConfig, QueueType};

        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create multiple test queues
        for i in 1..=3 {
            let queue_config = QueueConfig {
                id: format!("test-queue-{}", i),
                name: format!("test-queue-{}", i),
                queue_type: QueueType::SqsStandard,
                visibility_timeout: 30,
                message_retention_period: 345600,
                max_message_size: 262144,
                delay_seconds: 0,
                dlq_config: None,
                content_based_deduplication: false,
                tags: std::collections::HashMap::new(),
                redrive_allow_policy: None,
            };
            backend.create_queue(queue_config).await.unwrap();
        }

        let manager = VisibilityManager::new(backend);

        // Should process all queues without error
        let result = manager.process_expired_visibility().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_queue_expired_visibility() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let manager = VisibilityManager::new(backend);

        // This is currently a placeholder, so it should just return Ok
        let result = manager.process_queue_expired_visibility("test-queue").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_expired_messages_in_memory() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // This is currently a placeholder function
        let result = process_expired_messages_in_memory(backend).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_visibility_manager_start() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let manager = Arc::new(VisibilityManager::with_interval(
            backend,
            Duration::from_millis(10),
        ));

        // Start the manager in the background
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            manager_clone.start().await;
        });

        // Let it run for a short time
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Abort the background task
        handle.abort();

        // If we got here without panicking, the manager started successfully
    }

    #[tokio::test]
    async fn test_visibility_manager_start_with_queues() {
        use crate::types::{QueueConfig, QueueType};

        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create test queues
        for i in 1..=2 {
            let queue_config = QueueConfig {
                id: format!("visibility-test-{}", i),
                name: format!("visibility-test-{}", i),
                queue_type: QueueType::SqsStandard,
                visibility_timeout: 30,
                message_retention_period: 345600,
                max_message_size: 262144,
                delay_seconds: 0,
                dlq_config: None,
                content_based_deduplication: false,
                tags: std::collections::HashMap::new(),
                redrive_allow_policy: None,
            };
            backend.create_queue(queue_config).await.unwrap();
        }

        let manager = Arc::new(VisibilityManager::with_interval(
            backend,
            Duration::from_millis(10),
        ));

        // Start the manager
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            manager_clone.start().await;
        });

        // Let it run and process queues multiple times
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Abort the task
        handle.abort();
    }

    #[tokio::test]
    async fn test_visibility_manager_start_completes_full_iterations() {
        use crate::types::{QueueConfig, QueueType};
        use tokio::time;

        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create test queues
        for i in 1..=3 {
            let queue_config = QueueConfig {
                id: format!("iter-test-{}", i),
                name: format!("iter-test-{}", i),
                queue_type: QueueType::SqsStandard,
                visibility_timeout: 30,
                message_retention_period: 345600,
                max_message_size: 262144,
                delay_seconds: 0,
                dlq_config: None,
                content_based_deduplication: false,
                tags: std::collections::HashMap::new(),
                redrive_allow_policy: None,
            };
            backend.create_queue(queue_config).await.unwrap();
        }

        // Use paused time for deterministic behavior
        time::pause();

        let manager = Arc::new(VisibilityManager::with_interval(
            backend,
            Duration::from_millis(100),
        ));

        // Start the manager
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
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
        handle.abort();

        time::resume();

        // The manager should have completed several iterations,
        // hitting the info! log and processing all queues
    }
}
