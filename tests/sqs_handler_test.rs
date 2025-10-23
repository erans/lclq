//! Integration tests for SQS handler.

use lclq::config::{BackendConfig, EvictionPolicy, LclqConfig, LogFormat, PubsubConfig, SqsConfig, ServerConfig, StorageConfig};
use lclq::sqs::handler::SqsHandler;
use lclq::sqs::types::SqsAction;
use lclq::sqs::SqsRequest;
use lclq::storage::memory::InMemoryBackend;
use lclq::storage::StorageBackend; // Import trait to use trait methods
use std::collections::HashMap;
use std::sync::Arc;

/// Helper to create a test LclqConfig.
fn create_test_config() -> LclqConfig {
    LclqConfig {
        server: ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            sqs_port: 9324,
            pubsub_grpc_port: 8085,
            pubsub_http_port: 8086,
            admin_port: 9000,
        },
        storage: StorageConfig {
            backend: BackendConfig::InMemory {
                max_messages: 100000,
                eviction_policy: EvictionPolicy::Lru,
            },
        },
        sqs: SqsConfig {
            enable_signature_verification: false,
        },
        pubsub: PubsubConfig {
            default_project_id: "local-project".to_string(),
        },
        logging: lclq::config::LoggingConfig {
            level: "info".to_string(),
            format: LogFormat::Text,
        },
        metrics: lclq::config::MetricsConfig {
            enabled: false,
            port: 9090,
        },
    }
}

/// Helper to create a basic SqsRequest.
fn create_request(action: SqsAction, params: HashMap<String, String>) -> SqsRequest {
    SqsRequest {
        action,
        params,
        is_json: false,
    }
}

/// Test CreateQueue action with a standard queue.
#[tokio::test]
async fn test_handle_create_queue_standard() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    let mut params = HashMap::new();
    params.insert("QueueName".to_string(), "test-queue".to_string());

    let request = create_request(SqsAction::CreateQueue, params);
    let (response, content_type) = handler.handle_request(request).await;

    assert_eq!(content_type, "application/xml");
    assert!(response.contains("CreateQueueResponse"));
    assert!(response.contains("test-queue"));
    assert!(response.contains("http://127.0.0.1:9324/queue/test-queue"));

    // Verify queue was created in backend
    let queue = backend
        .get_queue("test-queue")
        .await
        .expect("Queue should exist");
    assert_eq!(queue.name, "test-queue");
}

/// Test CreateQueue action with a FIFO queue.
#[tokio::test]
async fn test_handle_create_queue_fifo() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    let mut params = HashMap::new();
    params.insert("QueueName".to_string(), "test-queue.fifo".to_string());
    params.insert("Attribute.1.Name".to_string(), "FifoQueue".to_string());
    params.insert("Attribute.1.Value".to_string(), "true".to_string());

    let request = create_request(SqsAction::CreateQueue, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("CreateQueueResponse"));
    assert!(response.contains("test-queue.fifo"));

    // Verify FIFO queue was created
    let queue = backend
        .get_queue("test-queue.fifo")
        .await
        .expect("FIFO queue should exist");
    assert_eq!(queue.name, "test-queue.fifo");
    assert!(matches!(
        queue.queue_type,
        lclq::types::QueueType::SqsFifo
    ));
}

/// Test CreateQueue with invalid queue name.
#[tokio::test]
async fn test_handle_create_queue_invalid_name() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend, config);

    let mut params = HashMap::new();
    params.insert("QueueName".to_string(), "invalid queue name!".to_string());

    let request = create_request(SqsAction::CreateQueue, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("ErrorResponse"));
    assert!(response.contains("InvalidParameterValue"));
}

/// Test GetQueueUrl action.
#[tokio::test]
async fn test_handle_get_queue_url() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // First create a queue
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "my-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Now get its URL
    let mut params = HashMap::new();
    params.insert("QueueName".to_string(), "my-queue".to_string());
    let request = create_request(SqsAction::GetQueueUrl, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("GetQueueUrlResponse"));
    assert!(response.contains("http://127.0.0.1:9324/queue/my-queue"));
}

/// Test GetQueueUrl for non-existent queue.
#[tokio::test]
async fn test_handle_get_queue_url_not_found() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend, config);

    let mut params = HashMap::new();
    params.insert("QueueName".to_string(), "nonexistent".to_string());
    let request = create_request(SqsAction::GetQueueUrl, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("ErrorResponse"));
    assert!(response.contains("does not exist"));
}

/// Test ListQueues action.
#[tokio::test]
async fn test_handle_list_queues() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create multiple queues
    for name in &["queue-1", "queue-2", "test-queue"] {
        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), name.to_string());
        let request = create_request(SqsAction::CreateQueue, params);
        handler.handle_request(request).await;
    }

    // List all queues
    let request = create_request(SqsAction::ListQueues, HashMap::new());
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("ListQueuesResponse"));
    assert!(response.contains("queue-1"));
    assert!(response.contains("queue-2"));
    assert!(response.contains("test-queue"));
}

/// Test ListQueues with prefix filter.
#[tokio::test]
async fn test_handle_list_queues_with_prefix() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queues with different prefixes
    for name in &["prod-queue-1", "prod-queue-2", "dev-queue-1"] {
        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), name.to_string());
        let request = create_request(SqsAction::CreateQueue, params);
        handler.handle_request(request).await;
    }

    // List only prod- queues
    let mut params = HashMap::new();
    params.insert("QueueNamePrefix".to_string(), "prod-".to_string());
    let request = create_request(SqsAction::ListQueues, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("ListQueuesResponse"));
    assert!(response.contains("prod-queue-1"));
    assert!(response.contains("prod-queue-2"));
    assert!(!response.contains("dev-queue-1"));
}

/// Test SendMessage action.
#[tokio::test]
async fn test_handle_send_message() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue first
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "msg-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Send a message
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/msg-queue".to_string(),
    );
    params.insert("MessageBody".to_string(), "Hello, World!".to_string());

    let request = create_request(SqsAction::SendMessage, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("SendMessageResponse"));
    assert!(response.contains("MessageId"));
    assert!(response.contains("MD5OfMessageBody"));

    // Verify message was sent
    let stats = backend
        .get_stats("msg-queue")
        .await
        .expect("Failed to get stats");
    assert_eq!(stats.available_messages, 1);
}

/// Test SendMessage with message attributes.
#[tokio::test]
async fn test_handle_send_message_with_attributes() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "attr-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Send message with attributes
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/attr-queue".to_string(),
    );
    params.insert("MessageBody".to_string(), "Test message".to_string());
    params.insert(
        "MessageAttribute.1.Name".to_string(),
        "Author".to_string(),
    );
    params.insert(
        "MessageAttribute.1.Value.DataType".to_string(),
        "String".to_string(),
    );
    params.insert(
        "MessageAttribute.1.Value.StringValue".to_string(),
        "Alice".to_string(),
    );

    let request = create_request(SqsAction::SendMessage, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("SendMessageResponse"));
    assert!(response.contains("MD5OfMessageAttributes"));
}

/// Test ReceiveMessage action.
#[tokio::test]
async fn test_handle_receive_message() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue and send message
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "recv-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    let mut send_params = HashMap::new();
    send_params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/recv-queue".to_string(),
    );
    send_params.insert("MessageBody".to_string(), "Test message".to_string());
    let send_request = create_request(SqsAction::SendMessage, send_params);
    handler.handle_request(send_request).await;

    // Receive the message
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/recv-queue".to_string(),
    );
    params.insert("MaxNumberOfMessages".to_string(), "1".to_string());

    let request = create_request(SqsAction::ReceiveMessage, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("ReceiveMessageResponse"));
    assert!(response.contains("Test message"));
    assert!(response.contains("ReceiptHandle"));
}

/// Test DeleteMessage action.
#[tokio::test]
async fn test_handle_delete_message() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue and send message
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "del-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    let mut send_params = HashMap::new();
    send_params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/del-queue".to_string(),
    );
    send_params.insert("MessageBody".to_string(), "Delete me".to_string());
    let send_request = create_request(SqsAction::SendMessage, send_params);
    handler.handle_request(send_request).await;

    // Receive the message to get receipt handle
    let mut recv_params = HashMap::new();
    recv_params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/del-queue".to_string(),
    );
    recv_params.insert("MaxNumberOfMessages".to_string(), "1".to_string());
    let recv_request = create_request(SqsAction::ReceiveMessage, recv_params);
    let (recv_response, _) = handler.handle_request(recv_request).await;

    // Extract receipt handle from response (simplified - in real test would parse XML)
    let receipt_start = recv_response.find("<ReceiptHandle>").unwrap() + 15;
    let receipt_end = recv_response.find("</ReceiptHandle>").unwrap();
    let receipt_handle = &recv_response[receipt_start..receipt_end];

    // Delete the message
    let mut delete_params = HashMap::new();
    delete_params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/del-queue".to_string(),
    );
    delete_params.insert("ReceiptHandle".to_string(), receipt_handle.to_string());
    let delete_request = create_request(SqsAction::DeleteMessage, delete_params);
    let (response, _) = handler.handle_request(delete_request).await;

    assert!(response.contains("DeleteMessageResponse"));

    // Verify message is gone
    let stats = backend
        .get_stats("del-queue")
        .await
        .expect("Failed to get stats");
    assert_eq!(stats.available_messages, 0);
}

/// Test PurgeQueue action.
#[tokio::test]
async fn test_handle_purge_queue() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue and send multiple messages
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "purge-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    for i in 1..=5 {
        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/purge-queue".to_string(),
        );
        send_params.insert("MessageBody".to_string(), format!("Message {}", i));
        let send_request = create_request(SqsAction::SendMessage, send_params);
        handler.handle_request(send_request).await;
    }

    // Verify messages are there
    let stats_before = backend
        .get_stats("purge-queue")
        .await
        .expect("Failed to get stats");
    assert_eq!(stats_before.available_messages, 5);

    // Purge the queue
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/purge-queue".to_string(),
    );
    let request = create_request(SqsAction::PurgeQueue, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("DeleteMessageResponse")); // PurgeQueue uses same response format as DeleteMessage

    // Verify queue is empty
    let stats_after = backend
        .get_stats("purge-queue")
        .await
        .expect("Failed to get stats");
    assert_eq!(stats_after.available_messages, 0);
}

/// Test DeleteQueue action.
#[tokio::test]
async fn test_handle_delete_queue() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create a queue
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "temp-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Verify it exists
    assert!(backend.get_queue("temp-queue").await.is_ok());

    // Delete the queue
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/temp-queue".to_string(),
    );
    let request = create_request(SqsAction::DeleteQueue, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("DeleteMessageResponse")); // DeleteQueue uses same response format as DeleteMessage

    // Verify it's gone
    assert!(backend.get_queue("temp-queue").await.is_err());
}

/// Test GetQueueAttributes action.
#[tokio::test]
async fn test_handle_get_queue_attributes() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue with custom attributes
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "attr-test-queue".to_string());
    create_params.insert(
        "Attribute.1.Name".to_string(),
        "VisibilityTimeout".to_string(),
    );
    create_params.insert("Attribute.1.Value".to_string(), "60".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Get queue attributes
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/attr-test-queue".to_string(),
    );
    params.insert("AttributeName.1".to_string(), "All".to_string());
    let request = create_request(SqsAction::GetQueueAttributes, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("GetQueueAttributesResponse"));
    assert!(response.contains("VisibilityTimeout"));
    assert!(response.contains("60"));
}

/// Test SetQueueAttributes action.
#[tokio::test]
async fn test_handle_set_queue_attributes() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "set-attr-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Set new attributes
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/set-attr-queue".to_string(),
    );
    params.insert(
        "Attribute.1.Name".to_string(),
        "VisibilityTimeout".to_string(),
    );
    params.insert("Attribute.1.Value".to_string(), "120".to_string());
    let request = create_request(SqsAction::SetQueueAttributes, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("DeleteMessageResponse")); // SetQueueAttributes uses same response format as DeleteMessage

    // Verify attributes were updated
    let queue = backend
        .get_queue("set-attr-queue")
        .await
        .expect("Queue should exist");
    assert_eq!(queue.visibility_timeout, 120);
}

/// Test SendMessageBatch action.
#[tokio::test]
async fn test_handle_send_message_batch() {
    let backend = Arc::new(InMemoryBackend::new());
    let config = create_test_config();
    let handler = SqsHandler::new(backend.clone(), config);

    // Create queue
    let mut create_params = HashMap::new();
    create_params.insert("QueueName".to_string(), "batch-queue".to_string());
    let create_queue_request = create_request(SqsAction::CreateQueue, create_params);
    handler.handle_request(create_queue_request).await;

    // Send batch of messages
    let mut params = HashMap::new();
    params.insert(
        "QueueUrl".to_string(),
        "http://127.0.0.1:9324/queue/batch-queue".to_string(),
    );
    params.insert("SendMessageBatchRequestEntry.1.Id".to_string(), "msg1".to_string());
    params.insert(
        "SendMessageBatchRequestEntry.1.MessageBody".to_string(),
        "Message 1".to_string(),
    );
    params.insert("SendMessageBatchRequestEntry.2.Id".to_string(), "msg2".to_string());
    params.insert(
        "SendMessageBatchRequestEntry.2.MessageBody".to_string(),
        "Message 2".to_string(),
    );
    params.insert("SendMessageBatchRequestEntry.3.Id".to_string(), "msg3".to_string());
    params.insert(
        "SendMessageBatchRequestEntry.3.MessageBody".to_string(),
        "Message 3".to_string(),
    );

    let request = create_request(SqsAction::SendMessageBatch, params);
    let (response, _) = handler.handle_request(request).await;

    assert!(response.contains("SendMessageBatchResponse"));
    assert!(response.contains("msg1"));
    assert!(response.contains("msg2"));
    assert!(response.contains("msg3"));

    // Verify all messages were sent
    let stats = backend
        .get_stats("batch-queue")
        .await
        .expect("Failed to get stats");
    assert_eq!(stats.available_messages, 3);
}
