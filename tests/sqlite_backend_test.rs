//! Integration tests for SQLite storage backend.

use chrono::Utc;
use lclq::storage::sqlite::{SqliteBackend, SqliteConfig};
use lclq::storage::StorageBackend;
use lclq::types::{Message, MessageId, QueueConfig, QueueType, ReceiveOptions};

/// Test basic queue operations with SQLite backend.
#[tokio::test]
async fn test_sqlite_queue_operations() {
    // Use file:memdb1?mode=memory&cache=shared for shared in-memory database
    let config = SqliteConfig {
        database_path: "file:memdb_queue_ops?mode=memory&cache=shared".to_string(),
        max_connections: 5,
    };

    let backend = SqliteBackend::new(config)
        .await
        .expect("Failed to create SQLite backend");

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
        tags: std::collections::HashMap::new(),
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

    // Delete queue
    backend
        .delete_queue("test-queue")
        .await
        .expect("Failed to delete queue");

    // Verify queue is gone
    let result = backend.get_queue("test-queue").await;
    assert!(result.is_err());
}

/// Test message send and receive with SQLite backend.
#[tokio::test]
async fn test_sqlite_message_operations() {
    // Use shared in-memory database
    let config = SqliteConfig {
        database_path: "file:memdb_msg_ops?mode=memory&cache=shared".to_string(),
        max_connections: 5,
    };

    let backend = SqliteBackend::new(config)
        .await
        .expect("Failed to create SQLite backend");

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
        tags: std::collections::HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send a message
    let message = Message {
        id: MessageId::new(),
        body: "Hello, SQLite!".to_string(),
        attributes: std::collections::HashMap::new(),
        queue_id: "msg-queue".to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: None,
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    };

    let sent_message = backend
        .send_message("msg-queue", message.clone())
        .await
        .expect("Failed to send message");

    assert_eq!(sent_message.body, "Hello, SQLite!");

    // Receive messages
    let options = ReceiveOptions {
        max_messages: 10,
        visibility_timeout: Some(30),
        wait_time_seconds: 0,
        attribute_names: vec![],
        message_attribute_names: vec![],
    };

    let received = backend
        .receive_messages("msg-queue", options)
        .await
        .expect("Failed to receive messages");

    assert_eq!(received.len(), 1);
    assert_eq!(received[0].message.body, "Hello, SQLite!");
    assert_eq!(received[0].message.receive_count, 1);

    // Get queue stats
    let stats = backend
        .get_stats("msg-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats.available_messages, 0); // Message is in-flight
    assert_eq!(stats.in_flight_messages, 1);

    // Delete message
    backend
        .delete_message("msg-queue", &received[0].receipt_handle)
        .await
        .expect("Failed to delete message");

    // Verify message is gone
    let stats = backend
        .get_stats("msg-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats.available_messages, 0);
    assert_eq!(stats.in_flight_messages, 0);
}

/// Test FIFO queue with deduplication.
#[tokio::test]
async fn test_sqlite_fifo_queue() {
    // Use shared in-memory database
    let config = SqliteConfig {
        database_path: "file:memdb_fifo?mode=memory&cache=shared".to_string(),
        max_connections: 5,
    };

    let backend = SqliteBackend::new(config)
        .await
        .expect("Failed to create SQLite backend");

    // Create a FIFO queue with content-based deduplication
    let queue_config = QueueConfig {
        id: "fifo-queue.fifo".to_string(),
        name: "fifo-queue.fifo".to_string(),
        queue_type: QueueType::SqsFifo,
        visibility_timeout: 30,
        message_retention_period: 345600,
        max_message_size: 262144,
        delay_seconds: 0,
        dlq_config: None,
        content_based_deduplication: true,
        tags: std::collections::HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send a message
    let message = Message {
        id: MessageId::new(),
        body: "FIFO message".to_string(),
        attributes: std::collections::HashMap::new(),
        queue_id: "fifo-queue.fifo".to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: Some("group1".to_string()),
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    };

    backend
        .send_message("fifo-queue.fifo", message.clone())
        .await
        .expect("Failed to send message");

    // Send duplicate message (should be deduplicated)
    backend
        .send_message("fifo-queue.fifo", message.clone())
        .await
        .expect("Failed to send duplicate message");

    // Receive messages - should only get one
    let options = ReceiveOptions {
        max_messages: 10,
        visibility_timeout: Some(30),
        wait_time_seconds: 0,
        attribute_names: vec![],
        message_attribute_names: vec![],
    };

    let received = backend
        .receive_messages("fifo-queue.fifo", options)
        .await
        .expect("Failed to receive messages");

    assert_eq!(received.len(), 1);
    assert_eq!(received[0].message.sequence_number, Some(0));
}

/// Test change visibility operation.
#[tokio::test]
async fn test_sqlite_change_visibility() {
    // Use shared in-memory database
    let config = SqliteConfig {
        database_path: "file:memdb_visibility?mode=memory&cache=shared".to_string(),
        max_connections: 5,
    };

    let backend = SqliteBackend::new(config)
        .await
        .expect("Failed to create SQLite backend");

    // Create a queue
    let queue_config = QueueConfig {
        id: "vis-queue".to_string(),
        name: "vis-queue".to_string(),
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

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send a message
    let message = Message {
        id: MessageId::new(),
        body: "Test visibility".to_string(),
        attributes: std::collections::HashMap::new(),
        queue_id: "vis-queue".to_string(),
        sent_timestamp: Utc::now(),
        receive_count: 0,
        message_group_id: None,
        deduplication_id: None,
        sequence_number: None,
        delay_seconds: None,
    };

    backend
        .send_message("vis-queue", message.clone())
        .await
        .expect("Failed to send message");

    // Receive message
    let options = ReceiveOptions {
        max_messages: 1,
        visibility_timeout: Some(30),
        wait_time_seconds: 0,
        attribute_names: vec![],
        message_attribute_names: vec![],
    };

    let received = backend
        .receive_messages("vis-queue", options)
        .await
        .expect("Failed to receive messages");

    assert_eq!(received.len(), 1);

    // Change visibility to 0 (return to queue)
    backend
        .change_visibility("vis-queue", &received[0].receipt_handle, 0)
        .await
        .expect("Failed to change visibility");

    // Message should be available again
    let stats = backend
        .get_stats("vis-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats.available_messages, 1);
    assert_eq!(stats.in_flight_messages, 0);
}

/// Test purge queue operation.
#[tokio::test]
async fn test_sqlite_purge_queue() {
    // Use shared in-memory database
    let config = SqliteConfig {
        database_path: "file:memdb_purge?mode=memory&cache=shared".to_string(),
        max_connections: 5,
    };

    let backend = SqliteBackend::new(config)
        .await
        .expect("Failed to create SQLite backend");

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
        tags: std::collections::HashMap::new(),
        redrive_allow_policy: None,
    };

    backend
        .create_queue(queue_config)
        .await
        .expect("Failed to create queue");

    // Send multiple messages
    for i in 0..5 {
        let message = Message {
            id: MessageId::new(),
            body: format!("Message {}", i),
            attributes: std::collections::HashMap::new(),
            queue_id: "purge-queue".to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        };

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

    // Purge queue
    backend
        .purge_queue("purge-queue")
        .await
        .expect("Failed to purge queue");

    // Verify all messages are gone
    let stats = backend
        .get_stats("purge-queue")
        .await
        .expect("Failed to get stats");

    assert_eq!(stats.available_messages, 0);
    assert_eq!(stats.in_flight_messages, 0);
}
