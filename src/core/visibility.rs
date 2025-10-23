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
    async fn process_queue_expired_visibility(&self, queue_id: &str) -> crate::Result<()> {
        // This is a placeholder - in a real implementation, we would:
        // 1. Get all in-flight messages for the queue
        // 2. Check which ones have expired visibility
        // 3. Move them back to available messages
        // 4. Check receive count and move to DLQ if needed

        // For now, we'll note that this functionality is integrated into
        // the InMemoryBackend's receive_messages method

        debug!(queue_id = %queue_id, "Checking for expired visibility timeouts");

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
}
