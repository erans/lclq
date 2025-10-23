//! Integration tests for in-memory storage backend.

use chrono::Utc;
use lclq::storage::memory::InMemoryBackend;
use lclq::storage::{QueueFilter, StorageBackend};
use lclq::types::{Message, MessageId, QueueConfig, QueueType, ReceiveOptions};
use std::collections::HashMap;

/// Helper to create a test message
fn create_message(queue_id: &str, body: &str) -> Message {
    Message {
        id: MessageId::new(),
        body: body.to_string(),
        attributes: HashMap::new(),
        queue_id: queue_id.to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: None,
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    }
}

/// Helper to create receive options
fn default_receive_options() -> ReceiveOptions {
    ReceiveOptions {
        max_messages: 10,
        visibility_timeout: Some(30),
        wait_time_seconds: 0,
        attribute_names: vec![],
        message_attribute_names: vec![],
    }
}

/// Test basic queue operations with in-memory backend.
#[tokio::test]
async fn test_memory_queue_operations() {
    let backend = InMemoryBackend::new();

    // Create a queue
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
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };

    let created_queue = backend
        .create_queue(queue_config.clone())
        .await
        .expect("Failed to create queue");

    assert_eq!(created_queue.id, "test-queue");
    assert_eq!(created_queue.name, "test-queue");

    // Get queue
    let retrieved_queue = backend
        .get_queue("test-queue")
        .await
        .expect("Failed to get queue");

    assert_eq!(retrieved_queue.id, "test-queue");
    assert_eq!(retrieved_queue.visibility_timeout, 30);

    // List queues
    let queues = backend
        .list_queues(None)
        .await
        .expect("Failed to list queues");

    assert_eq!(queues.len(), 1);
    assert_eq!(queues[0].id, "test-queue");

    // List queues with prefix filter
    let filter = QueueFilter {
        name_prefix: Some("test-".to_string()),
    };
    let filtered_queues = backend
        .list_queues(Some(filter))
        .await
        .expect("Failed to list queues with filter");
    assert_eq!(filtered_queues.len(), 1);

    // Delete queue
    backend
        .delete_queue("test-queue")
        .await
        .expect("Failed to delete queue");

    // Verify queue is gone
    let result = backend.get_queue("test-queue").await;
    assert!(result.is_err());
}

/// Test message send and receive operations.
#[tokio::test]
async fn test_memory_message_operations() {
    let backend = InMemoryBackend::new();

    // Create a queue
    let queue_config = QueueConfig {
        id: "msg-queue".to_string(),
        name: "msg-queue".to_string(),
        queue_type: QueueType::SqsStandard,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send a message
    let message = create_message("msg-queue", "Hello, World!");

    let sent_message = backend
        .send_message("msg-queue", message.clone())
        .await
        .expect("Failed to send message");

    assert_eq!(sent_message.body, "Hello, World!");

    // Receive messages
    let received = backend
        .receive_messages("msg-queue", default_receive_options())
        .await
        .expect("Failed to receive messages");

    assert_eq!(received.len(), 1);
    assert_eq!(received[0].message.body, "Hello, World!");
    assert!(!received[0].receipt_handle.is_empty());

    // Delete the message
    backend
        .delete_message("msg-queue", &received[0].receipt_handle)
        .await
        .expect("Failed to delete message");

    // Verify message is gone
    let no_messages = backend
        .receive_messages("msg-queue", default_receive_options())
        .await
        .expect("Failed to receive messages");

    assert_eq!(no_messages.len(), 0);
}

/// Test FIFO queue ordering and message groups.
#[tokio::test]
async fn test_memory_fifo_queue() {
    let backend = InMemoryBackend::new();

    // Create a FIFO queue
    let queue_config = QueueConfig {
        id: "test-fifo.fifo".to_string(),
        name: "test-fifo.fifo".to_string(),
        queue_type: QueueType::SqsFifo,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create FIFO queue");

    // Send messages to different message groups
    for i in 1..=3 {
        let mut message = create_message("test-fifo.fifo", &format!("Message {} in group A", i));
        message.message_group_id = Some("group-a".to_string());
        message.deduplication_id = Some(format!("dedup-a-{}", i));

        backend
            .send_message("test-fifo.fifo", message)
            .await
            .expect("Failed to send message");
    }

    for i in 1..=3 {
        let mut message = create_message("test-fifo.fifo", &format!("Message {} in group B", i));
        message.message_group_id = Some("group-b".to_string());
        message.deduplication_id = Some(format!("dedup-b-{}", i));

        backend
            .send_message("test-fifo.fifo", message)
            .await
            .expect("Failed to send message");
    }

    // Receive messages - should respect FIFO ordering within groups
    let received = backend
        .receive_messages("test-fifo.fifo", default_receive_options())
        .await
        .expect("Failed to receive messages");

    assert_eq!(received.len(), 6);

    // Verify messages from each group are in order
    let group_a: Vec<_> = received
        .iter()
        .filter(|m| m.message.message_group_id.as_deref() == Some("group-a"))
        .collect();
    assert_eq!(group_a.len(), 3);
    assert_eq!(group_a[0].message.body, "Message 1 in group A");
    assert_eq!(group_a[1].message.body, "Message 2 in group A");
    assert_eq!(group_a[2].message.body, "Message 3 in group A");
}

/// Test batch message operations.
#[tokio::test]
async fn test_memory_batch_operations() {
    let backend = InMemoryBackend::new();

    // Create a queue
    let queue_config = QueueConfig {
        id: "batch-queue".to_string(),
        name: "batch-queue".to_string(),
        queue_type: QueueType::SqsStandard,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send messages in batch
    let messages: Vec<Message> = (1..=10)
        .map(|i| create_message("batch-queue", &format!("Batch message {}", i)))
        .collect();

    let sent = backend
        .send_messages("batch-queue", messages)
        .await
        .expect("Failed to send batch messages");

    assert_eq!(sent.len(), 10);

    // Receive all messages
    let received = backend
        .receive_messages("batch-queue", default_receive_options())
        .await
        .expect("Failed to receive messages");

    assert_eq!(received.len(), 10);

    // Verify all messages were received
    for i in 1..=10 {
        assert!(received
            .iter()
            .any(|m| m.message.body == format!("Batch message {}", i)));
    }
}

/// Test queue purge operation.
#[tokio::test]
async fn test_memory_purge_queue() {
    let backend = InMemoryBackend::new();

    // Create a queue
    let queue_config = QueueConfig {
        id: "purge-queue".to_string(),
        name: "purge-queue".to_string(),
        queue_type: QueueType::SqsStandard,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send multiple messages
    for i in 1..=5 {
        let message = create_message("purge-queue", &format!("Message {}", i));

        backend
            .send_message("purge-queue", message)
            .await
            .expect("Failed to send message");
    }

    // Verify messages are there
    let stats = backend
        .get_stats("purge-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats.available_messages, 5);

    // Purge the queue
    backend
        .purge_queue("purge-queue")
        .await
        .expect("Failed to purge queue");

    // Verify queue is empty
    let stats_after = backend
        .get_stats("purge-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats_after.available_messages, 0);

    // Try to receive messages - should get none
    let no_messages = backend
        .receive_messages("purge-queue", default_receive_options())
        .await
        .expect("Failed to receive messages");

    assert_eq!(no_messages.len(), 0);
}

/// Test health check.
#[tokio::test]
async fn test_memory_health_check() {
    let backend = InMemoryBackend::new();

    let health = backend
        .health_check()
        .await
        .expect("Failed to check health");

    match health {
        lclq::storage::HealthStatus::Healthy => {
            // Expected
        }
        lclq::storage::HealthStatus::Unhealthy(msg) => {
            panic!("Backend should be healthy, got: {}", msg);
        }
    }
}

/// Test concurrent access to in-memory backend.
#[tokio::test]
async fn test_memory_concurrent_access() {
    use std::sync::Arc;

    let backend = Arc::new(InMemoryBackend::new());

    // Create a queue
    let queue_config = QueueConfig {
        id: "concurrent-queue".to_string(),
        name: "concurrent-queue".to_string(),
        queue_type: QueueType::SqsStandard,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: false,
        tags: HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Spawn multiple tasks to send messages concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let backend_clone = backend.clone();
        let handle = tokio::spawn(async move {
            let message = create_message("concurrent-queue", &format!("Concurrent message {}", i));

            backend_clone
                .send_message("concurrent-queue", message)
                .await
                .expect("Failed to send message");
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify all messages were sent
    let stats = backend
        .get_stats("concurrent-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats.available_messages, 10);
}
