use aws_config::BehaviorVersion;
use aws_sdk_sqs::config::{Credentials, Region};
use aws_sdk_sqs::types::{
    DeleteMessageBatchRequestEntry, MessageAttributeValue, MessageSystemAttributeName,
    QueueAttributeName, SendMessageBatchRequestEntry,
};
use aws_sdk_sqs::Client;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

const SQS_ENDPOINT: &str = "http://localhost:9324";
const REGION: &str = "us-east-1";

/// Create a configured SQS client
async fn create_sqs_client() -> Client {
    let creds = Credentials::new("dummy", "dummy", None, None, "static");

    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(REGION))
        .credentials_provider(creds)
        .endpoint_url(SQS_ENDPOINT)
        .load()
        .await;

    Client::new(&config)
}

#[tokio::test]
async fn test_basic_queue_operations() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-basic-{}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    // Create queue
    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("Failed to create queue");

    let queue_url = create_result.queue_url().expect("Queue URL should be returned");
    println!("Created queue: {}", queue_url);

    // Send message
    let message_body = "Hello from Rust!";
    let send_result = client
        .send_message()
        .queue_url(queue_url)
        .message_body(message_body)
        .send()
        .await
        .expect("Failed to send message");

    assert!(send_result.message_id().is_some(), "Message ID should not be None");
    println!("Sent message: {}", send_result.message_id().unwrap());

    // Receive message
    let receive_result = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .message_system_attribute_names(MessageSystemAttributeName::All)
        .send()
        .await
        .expect("Failed to receive message");

    assert_eq!(receive_result.messages().len(), 1, "Should receive 1 message");
    let message = &receive_result.messages()[0];
    assert_eq!(message.body().unwrap(), message_body, "Message body should match");
    let attrs = message.attributes().expect("Message should have attributes");
    assert!(!attrs.is_empty(), "Message should have attributes");
    println!("Received message with {} attributes", attrs.len());

    // Delete message
    client
        .delete_message()
        .queue_url(queue_url)
        .receipt_handle(message.receipt_handle().unwrap())
        .send()
        .await
        .expect("Failed to delete message");
    println!("Deleted message");

    // Delete queue
    client
        .delete_queue()
        .queue_url(queue_url)
        .send()
        .await
        .expect("Failed to delete queue");
    println!("Deleted queue");
}

#[tokio::test]
async fn test_message_attributes() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-attrs-{}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("Failed to create queue");

    let queue_url = create_result.queue_url().unwrap();

    // Send message with attributes
    let mut attributes = HashMap::new();
    attributes.insert(
        "Author".to_string(),
        MessageAttributeValue::builder()
            .data_type("String")
            .string_value("Rust SDK")
            .build()
            .unwrap(),
    );
    attributes.insert(
        "Priority".to_string(),
        MessageAttributeValue::builder()
            .data_type("Number")
            .string_value("5")
            .build()
            .unwrap(),
    );

    client
        .send_message()
        .queue_url(queue_url)
        .message_body("Test message")
        .set_message_attributes(Some(attributes))
        .send()
        .await
        .expect("Failed to send message");
    println!("Sent message with 2 attributes");

    // Receive and verify attributes
    let receive_result = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .message_attribute_names("All")
        .send()
        .await
        .expect("Failed to receive message");

    assert_eq!(receive_result.messages().len(), 1, "Should receive 1 message");
    let attrs = receive_result.messages()[0].message_attributes();
    assert!(attrs.is_some(), "Message should have attributes");
    let attrs = attrs.unwrap();
    assert!(!attrs.is_empty(), "Message should have attributes");

    let author = attrs.get("Author").expect("Should have Author attribute");
    assert_eq!(author.string_value().unwrap(), "Rust SDK", "Author should match");

    let priority = attrs.get("Priority").expect("Should have Priority attribute");
    assert_eq!(priority.string_value().unwrap(), "5", "Priority should match");

    println!("Verified attributes: Author={}, Priority={}",
             author.string_value().unwrap(),
             priority.string_value().unwrap());

    // Cleanup
    let _ = client.delete_queue().queue_url(queue_url).send().await;
}

#[tokio::test]
async fn test_fifo_queue() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-fifo-{}.fifo", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let mut attributes = HashMap::new();
    attributes.insert(QueueAttributeName::FifoQueue, "true".to_string());
    attributes.insert(QueueAttributeName::ContentBasedDeduplication, "true".to_string());

    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .set_attributes(Some(attributes))
        .send()
        .await
        .expect("Failed to create FIFO queue");

    let queue_url = create_result.queue_url().unwrap();
    println!("Created FIFO queue: {}", queue_url);

    // Send messages in order
    let messages = vec!["First", "Second", "Third"];
    for msg in &messages {
        client
            .send_message()
            .queue_url(queue_url)
            .message_body(*msg)
            .message_group_id("test-group")
            .send()
            .await
            .expect("Failed to send message");
    }
    println!("Sent 3 messages in order");

    // Receive and verify order
    let mut received_bodies = Vec::new();
    for _ in 0..3 {
        let receive_result = client
            .receive_message()
            .queue_url(queue_url)
            .max_number_of_messages(1)
            .send()
            .await
            .expect("Failed to receive message");

        if !receive_result.messages().is_empty() {
            let msg = &receive_result.messages()[0];
            received_bodies.push(msg.body().unwrap().to_string());
            let _ = client
                .delete_message()
                .queue_url(queue_url)
                .receipt_handle(msg.receipt_handle().unwrap())
                .send()
                .await;
        }
    }

    assert_eq!(received_bodies, messages, "Messages should be in order");
    println!("Verified FIFO ordering: {:?}", received_bodies);

    // Cleanup
    let _ = client.delete_queue().queue_url(queue_url).send().await;
}

#[tokio::test]
async fn test_batch_operations() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-batch-{}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("Failed to create queue");

    let queue_url = create_result.queue_url().unwrap();

    // Send batch
    let entries = vec![
        SendMessageBatchRequestEntry::builder()
            .id("1")
            .message_body("Message 1")
            .build()
            .unwrap(),
        SendMessageBatchRequestEntry::builder()
            .id("2")
            .message_body("Message 2")
            .build()
            .unwrap(),
        SendMessageBatchRequestEntry::builder()
            .id("3")
            .message_body("Message 3")
            .build()
            .unwrap(),
    ];

    let batch_result = client
        .send_message_batch()
        .queue_url(queue_url)
        .set_entries(Some(entries))
        .send()
        .await
        .expect("Failed to send batch");

    assert_eq!(batch_result.successful().len(), 3, "All 3 messages should succeed");
    println!("Sent batch of 3 messages");

    // Receive messages
    let receive_result = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(10)
        .send()
        .await
        .expect("Failed to receive messages");

    assert_eq!(receive_result.messages().len(), 3, "Should receive 3 messages");
    println!("Received 3 messages");

    // Delete batch
    let delete_entries: Vec<_> = receive_result
        .messages()
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            DeleteMessageBatchRequestEntry::builder()
                .id((i + 1).to_string())
                .receipt_handle(msg.receipt_handle().unwrap())
                .build()
                .unwrap()
        })
        .collect();

    let delete_result = client
        .delete_message_batch()
        .queue_url(queue_url)
        .set_entries(Some(delete_entries))
        .send()
        .await
        .expect("Failed to delete batch");

    assert_eq!(delete_result.successful().len(), 3, "All 3 deletes should succeed");
    println!("Deleted batch of 3 messages");

    // Cleanup
    let _ = client.delete_queue().queue_url(queue_url).send().await;
}

#[tokio::test]
async fn test_queue_attributes() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-qattrs-{}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let mut attributes = HashMap::new();
    attributes.insert(QueueAttributeName::VisibilityTimeout, "60".to_string());
    attributes.insert(QueueAttributeName::MessageRetentionPeriod, "86400".to_string());

    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .set_attributes(Some(attributes))
        .send()
        .await
        .expect("Failed to create queue");

    let queue_url = create_result.queue_url().unwrap();

    // Get attributes
    let get_result = client
        .get_queue_attributes()
        .queue_url(queue_url)
        .attribute_names(QueueAttributeName::All)
        .send()
        .await
        .expect("Failed to get attributes");

    let attrs = get_result.attributes().unwrap();
    assert!(!attrs.is_empty(), "Should have attributes");
    assert_eq!(attrs.get(&QueueAttributeName::VisibilityTimeout).unwrap(), "60", "VisibilityTimeout should be 60");
    assert_eq!(attrs.get(&QueueAttributeName::MessageRetentionPeriod).unwrap(), "86400", "MessageRetentionPeriod should be 86400");
    println!("Retrieved attributes: VisibilityTimeout={}", attrs.get(&QueueAttributeName::VisibilityTimeout).unwrap());

    // Set attributes
    let mut new_attributes = HashMap::new();
    new_attributes.insert(QueueAttributeName::VisibilityTimeout, "120".to_string());

    client
        .set_queue_attributes()
        .queue_url(queue_url)
        .set_attributes(Some(new_attributes))
        .send()
        .await
        .expect("Failed to set attributes");
    println!("Updated VisibilityTimeout to 120");

    // Verify update
    let verify_result = client
        .get_queue_attributes()
        .queue_url(queue_url)
        .attribute_names(QueueAttributeName::VisibilityTimeout)
        .send()
        .await
        .expect("Failed to verify attributes");

    let verify_attrs = verify_result.attributes().unwrap();
    assert_eq!(verify_attrs.get(&QueueAttributeName::VisibilityTimeout).unwrap(), "120", "VisibilityTimeout should be updated to 120");
    println!("Verified update: VisibilityTimeout={}", verify_attrs.get(&QueueAttributeName::VisibilityTimeout).unwrap());

    // Cleanup
    let _ = client.delete_queue().queue_url(queue_url).send().await;
}

#[tokio::test]
async fn test_change_message_visibility() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-visibility-{}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("Failed to create queue");

    let queue_url = create_result.queue_url().unwrap();

    // Send message
    client
        .send_message()
        .queue_url(queue_url)
        .message_body("Test visibility")
        .send()
        .await
        .expect("Failed to send message");

    // Receive message
    let receive_result = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .send()
        .await
        .expect("Failed to receive message");

    assert_eq!(receive_result.messages().len(), 1, "Should receive 1 message");
    let receipt_handle = receive_result.messages()[0].receipt_handle().unwrap();
    println!("Received message");

    // Change visibility to 0 (return to queue immediately)
    client
        .change_message_visibility()
        .queue_url(queue_url)
        .receipt_handle(receipt_handle)
        .visibility_timeout(0)
        .send()
        .await
        .expect("Failed to change visibility");
    println!("Changed visibility timeout to 0");

    // Should be able to receive again immediately
    let receive_again = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .send()
        .await
        .expect("Failed to receive message again");

    assert_eq!(receive_again.messages().len(), 1, "Should receive message again");
    println!("Received message again immediately");

    // Cleanup
    let _ = client.delete_queue().queue_url(queue_url).send().await;
}

#[tokio::test]
async fn test_delay_queue() {
    let client = create_sqs_client().await;
    let queue_name = format!("test-delay-{}", SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos());

    let mut attributes = HashMap::new();
    attributes.insert(QueueAttributeName::DelaySeconds, "2".to_string());

    let create_result = client
        .create_queue()
        .queue_name(&queue_name)
        .set_attributes(Some(attributes))
        .send()
        .await
        .expect("Failed to create delay queue");

    let queue_url = create_result.queue_url().unwrap();
    println!("Created delay queue (2 second delay)");

    // Send message
    let send_time = SystemTime::now();
    client
        .send_message()
        .queue_url(queue_url)
        .message_body("Delayed message")
        .send()
        .await
        .expect("Failed to send message");
    println!("Sent message");

    // Try immediate receive (should be empty)
    let immediate = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .send()
        .await
        .expect("Failed immediate receive");

    assert_eq!(immediate.messages().len(), 0, "Should not receive message immediately");
    println!("No message received immediately (correct)");

    // Wait for delay
    println!("Waiting 3 seconds...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Receive should work now
    let delayed = client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .send()
        .await
        .expect("Failed delayed receive");

    let receive_time = SystemTime::now();
    assert_eq!(delayed.messages().len(), 1, "Should receive 1 message after delay");

    let elapsed = receive_time.duration_since(send_time).unwrap().as_secs_f64();
    println!("Received message after {:.2} seconds", elapsed);

    // Cleanup
    let _ = client.delete_queue().queue_url(queue_url).send().await;
}
