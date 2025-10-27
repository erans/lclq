//! Dead Letter Queue handler.

use std::sync::Arc;

use tracing::{debug, info};

use crate::Result;
use crate::storage::StorageBackend;
use crate::types::{DlqConfig, Message, QueueConfig};

/// Dead Letter Queue handler.
pub struct DlqHandler {
    backend: Arc<dyn StorageBackend>,
}

impl DlqHandler {
    /// Create a new DLQ handler.
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Check if a message should be moved to the DLQ based on receive count.
    pub async fn should_move_to_dlq(&self, message: &Message, queue_config: &QueueConfig) -> bool {
        if let Some(dlq_config) = &queue_config.dlq_config {
            if message.receive_count >= dlq_config.max_receive_count {
                debug!(
                    queue_id = %queue_config.id,
                    message_id = %message.id,
                    receive_count = message.receive_count,
                    max_receive_count = dlq_config.max_receive_count,
                    "Message exceeded max receive count"
                );
                return true;
            }
        }
        false
    }

    /// Move a message to the configured DLQ.
    pub async fn move_to_dlq(
        &self,
        message: Message,
        dlq_config: &DlqConfig,
        source_queue_id: &str,
    ) -> Result<()> {
        info!(
            source_queue_id = %source_queue_id,
            dlq_id = %dlq_config.target_queue_id,
            message_id = %message.id,
            receive_count = message.receive_count,
            "Moving message to DLQ"
        );

        // Send the message to the DLQ
        self.backend
            .send_message(&dlq_config.target_queue_id, message.clone())
            .await?;

        debug!(
            message_id = %message.id,
            dlq_id = %dlq_config.target_queue_id,
            "Message moved to DLQ successfully"
        );

        Ok(())
    }

    /// Process a message and potentially move it to DLQ if needed.
    ///
    /// Returns true if the message was moved to DLQ, false otherwise.
    pub async fn process_message(
        &self,
        message: &Message,
        queue_config: &QueueConfig,
    ) -> Result<bool> {
        if let Some(dlq_config) = &queue_config.dlq_config {
            if self.should_move_to_dlq(message, queue_config).await {
                self.move_to_dlq(message.clone(), dlq_config, &queue_config.id)
                    .await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Get DLQ statistics for a queue.
    pub async fn get_dlq_stats(&self, dlq_id: &str) -> Result<DlqStats> {
        let stats = self.backend.get_stats(dlq_id).await?;

        Ok(DlqStats {
            message_count: stats.available_messages + stats.in_flight_messages,
            oldest_message_age_seconds: stats
                .oldest_message_timestamp
                .map(|ts| (chrono::Utc::now() - ts).num_seconds() as u64),
        })
    }
}

/// Dead Letter Queue statistics.
#[derive(Debug, Clone)]
pub struct DlqStats {
    /// Total number of messages in the DLQ.
    pub message_count: u64,
    /// Age of the oldest message in seconds.
    pub oldest_message_age_seconds: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::InMemoryBackend;
    use crate::types::{MessageAttributes, MessageId, QueueType};
    use chrono::Utc;

    fn create_test_message(receive_count: u32) -> Message {
        Message {
            id: MessageId::new(),
            body: "test message".to_string(),
            attributes: MessageAttributes::new(),
            queue_id: "test-queue".to_string(),
            sent_timestamp: Utc::now(),
            receive_count,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        }
    }

    fn create_test_queue_config(dlq_config: Option<DlqConfig>) -> QueueConfig {
        QueueConfig {
            id: "test-queue".to_string(),
            name: "test-queue".to_string(),
            queue_type: QueueType::SqsStandard,
            visibility_timeout: 30,
            message_retention_period: 345600,
            max_message_size: 262144,
            delay_seconds: 0,
            dlq_config,
            content_based_deduplication: false,
            tags: std::collections::HashMap::new(),
            redrive_allow_policy: None,
        }
    }

    #[tokio::test]
    async fn test_should_move_to_dlq() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let handler = DlqHandler::new(backend);

        let dlq_config = DlqConfig {
            target_queue_id: "dlq".to_string(),
            max_receive_count: 3,
        };

        let queue_config = create_test_queue_config(Some(dlq_config));

        // Message with receive_count < max should not be moved
        let msg1 = create_test_message(2);
        assert!(!handler.should_move_to_dlq(&msg1, &queue_config).await);

        // Message with receive_count >= max should be moved
        let msg2 = create_test_message(3);
        assert!(handler.should_move_to_dlq(&msg2, &queue_config).await);

        let msg3 = create_test_message(5);
        assert!(handler.should_move_to_dlq(&msg3, &queue_config).await);
    }

    #[tokio::test]
    async fn test_no_dlq_config() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let handler = DlqHandler::new(backend);

        let queue_config = create_test_queue_config(None);

        // Without DLQ config, should never move to DLQ
        let msg = create_test_message(100);
        assert!(!handler.should_move_to_dlq(&msg, &queue_config).await);
    }

    #[tokio::test]
    async fn test_dlq_handler_creation() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let handler = DlqHandler::new(backend);

        // Verify handler was created successfully (simple smoke test)
        let queue_config = create_test_queue_config(None);
        let msg = create_test_message(1);
        assert!(!handler.should_move_to_dlq(&msg, &queue_config).await);
    }

    #[tokio::test]
    async fn test_move_to_dlq_success() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create DLQ first
        let dlq_config_obj = QueueConfig {
            id: "test-dlq".to_string(),
            name: "test-dlq".to_string(),
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
        backend.create_queue(dlq_config_obj).await.unwrap();

        let handler = DlqHandler::new(backend.clone());

        let dlq_config = DlqConfig {
            target_queue_id: "test-dlq".to_string(),
            max_receive_count: 3,
        };

        let message = create_test_message(5);

        // Move message to DLQ
        let result = handler
            .move_to_dlq(message.clone(), &dlq_config, "source-queue")
            .await;

        assert!(result.is_ok());

        // Verify message was added to DLQ
        let stats = backend.get_stats("test-dlq").await.unwrap();
        assert_eq!(stats.available_messages, 1);
    }

    #[tokio::test]
    async fn test_move_to_dlq_nonexistent_queue() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let handler = DlqHandler::new(backend);

        let dlq_config = DlqConfig {
            target_queue_id: "nonexistent-dlq".to_string(),
            max_receive_count: 3,
        };

        let message = create_test_message(5);

        // Should fail because DLQ doesn't exist
        let result = handler
            .move_to_dlq(message, &dlq_config, "source-queue")
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_message_moves_to_dlq() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create DLQ
        let dlq_config_obj = QueueConfig {
            id: "process-dlq".to_string(),
            name: "process-dlq".to_string(),
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
        backend.create_queue(dlq_config_obj).await.unwrap();

        let handler = DlqHandler::new(backend.clone());

        let dlq_config = DlqConfig {
            target_queue_id: "process-dlq".to_string(),
            max_receive_count: 3,
        };

        let queue_config = create_test_queue_config(Some(dlq_config));

        // Message exceeding max receive count
        let message = create_test_message(5);

        let moved = handler
            .process_message(&message, &queue_config)
            .await
            .unwrap();

        assert!(moved); // Should return true
        let stats = backend.get_stats("process-dlq").await.unwrap();
        assert_eq!(stats.available_messages, 1);
    }

    #[tokio::test]
    async fn test_process_message_does_not_move() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create DLQ
        let dlq_config_obj = QueueConfig {
            id: "process-dlq2".to_string(),
            name: "process-dlq2".to_string(),
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
        backend.create_queue(dlq_config_obj).await.unwrap();

        let handler = DlqHandler::new(backend.clone());

        let dlq_config = DlqConfig {
            target_queue_id: "process-dlq2".to_string(),
            max_receive_count: 3,
        };

        let queue_config = create_test_queue_config(Some(dlq_config));

        // Message below max receive count
        let message = create_test_message(2);

        let moved = handler
            .process_message(&message, &queue_config)
            .await
            .unwrap();

        assert!(!moved); // Should return false
        let stats = backend.get_stats("process-dlq2").await.unwrap();
        assert_eq!(stats.available_messages, 0); // No messages in DLQ
    }

    #[tokio::test]
    async fn test_process_message_without_dlq_config() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let handler = DlqHandler::new(backend);

        let queue_config = create_test_queue_config(None);

        // Even with high receive count, should not move without DLQ config
        let message = create_test_message(100);

        let moved = handler
            .process_message(&message, &queue_config)
            .await
            .unwrap();

        assert!(!moved);
    }

    #[tokio::test]
    async fn test_get_dlq_stats_empty_queue() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create empty DLQ
        let dlq_config = QueueConfig {
            id: "stats-dlq".to_string(),
            name: "stats-dlq".to_string(),
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
        backend.create_queue(dlq_config).await.unwrap();

        let handler = DlqHandler::new(backend);

        let stats = handler.get_dlq_stats("stats-dlq").await.unwrap();

        assert_eq!(stats.message_count, 0);
        assert!(stats.oldest_message_age_seconds.is_none());
    }

    #[tokio::test]
    async fn test_get_dlq_stats_with_messages() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;

        // Create DLQ and add a message
        let dlq_config = QueueConfig {
            id: "stats-dlq2".to_string(),
            name: "stats-dlq2".to_string(),
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
        backend.create_queue(dlq_config).await.unwrap();

        // Add a message
        let message = create_test_message(5);
        backend.send_message("stats-dlq2", message).await.unwrap();

        let handler = DlqHandler::new(backend);

        let stats = handler.get_dlq_stats("stats-dlq2").await.unwrap();

        assert_eq!(stats.message_count, 1);
        assert!(stats.oldest_message_age_seconds.is_some());
    }

    #[tokio::test]
    async fn test_get_dlq_stats_nonexistent_queue() {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let handler = DlqHandler::new(backend);

        let result = handler.get_dlq_stats("nonexistent-queue").await;

        assert!(result.is_err());
    }
}
