//! Integration tests for SQLite subscription storage with push config

use lclq::storage::sqlite::{SqliteBackend, SqliteConfig};
use lclq::storage::StorageBackend;
use lclq::types::{SubscriptionConfig, PushConfig, RetryPolicy};

#[tokio::test]
async fn test_sqlite_push_subscription() {
    // Create SQLite backend with in-memory database
    let config = SqliteConfig {
        database_path: ":memory:".to_string(),
        max_connections: 5,
    };
    
    let backend = SqliteBackend::new(config).await.expect("Failed to create backend");
    
    // Create push subscription
    let push_sub = SubscriptionConfig {
        id: "test:push-sub".to_string(),
        name: "test-push-sub".to_string(),
        topic_id: "test:topic".to_string(),
        ack_deadline_seconds: 30,
        message_retention_duration: 604800,
        enable_message_ordering: false,
        filter: None,
        dead_letter_policy: None,
        push_config: Some(PushConfig {
            endpoint: "https://example.com/webhook".to_string(),
            retry_policy: Some(RetryPolicy {
                min_backoff_seconds: 10,
                max_backoff_seconds: 600,
                max_attempts: 5,
            }),
            timeout_seconds: Some(30),
        }),
    };
    
    let created = backend.create_subscription(push_sub.clone()).await.expect("Failed to create push subscription");
    assert_eq!(created.id, push_sub.id);
    assert!(created.push_config.is_some());
    
    // Retrieve push subscription
    let retrieved = backend.get_subscription(&push_sub.id).await.expect("Failed to get push subscription");
    assert!(retrieved.push_config.is_some());
    
    let push_config = retrieved.push_config.unwrap();
    assert_eq!(push_config.endpoint, "https://example.com/webhook");
    assert!(push_config.retry_policy.is_some());
    
    let retry = push_config.retry_policy.unwrap();
    assert_eq!(retry.min_backoff_seconds, 10);
    assert_eq!(retry.max_backoff_seconds, 600);
    assert_eq!(retry.max_attempts, 5);
    assert_eq!(push_config.timeout_seconds, Some(30));
}

#[tokio::test]
async fn test_sqlite_pull_subscription() {
    let config = SqliteConfig {
        database_path: ":memory:".to_string(),
        max_connections: 5,
    };
    
    let backend = SqliteBackend::new(config).await.expect("Failed to create backend");
    
    // Create pull subscription (no push config)
    let pull_sub = SubscriptionConfig {
        id: "test:pull-sub".to_string(),
        name: "test-pull-sub".to_string(),
        topic_id: "test:topic".to_string(),
        ack_deadline_seconds: 30,
        message_retention_duration: 604800,
        enable_message_ordering: false,
        filter: None,
        dead_letter_policy: None,
        push_config: None,
    };
    
    let created = backend.create_subscription(pull_sub.clone()).await.expect("Failed to create pull subscription");
    assert!(created.push_config.is_none());
    
    let retrieved = backend.get_subscription(&pull_sub.id).await.expect("Failed to get pull subscription");
    assert!(retrieved.push_config.is_none());
}

#[tokio::test]
async fn test_sqlite_list_subscriptions() {
    let config = SqliteConfig {
        database_path: ":memory:".to_string(),
        max_connections: 5,
    };
    
    let backend = SqliteBackend::new(config).await.expect("Failed to create backend");
    
    // Create both push and pull subscriptions
    let push_sub = SubscriptionConfig {
        id: "test:push-sub".to_string(),
        name: "test-push-sub".to_string(),
        topic_id: "test:topic".to_string(),
        ack_deadline_seconds: 30,
        message_retention_duration: 604800,
        enable_message_ordering: false,
        filter: None,
        dead_letter_policy: None,
        push_config: Some(PushConfig {
            endpoint: "https://example.com/webhook".to_string(),
            retry_policy: None,
            timeout_seconds: None,
        }),
    };
    
    let pull_sub = SubscriptionConfig {
        id: "test:pull-sub".to_string(),
        name: "test-pull-sub".to_string(),
        topic_id: "test:topic".to_string(),
        ack_deadline_seconds: 30,
        message_retention_duration: 604800,
        enable_message_ordering: false,
        filter: None,
        dead_letter_policy: None,
        push_config: None,
    };
    
    backend.create_subscription(push_sub).await.expect("Failed to create push subscription");
    backend.create_subscription(pull_sub).await.expect("Failed to create pull subscription");
    
    // List and verify
    let subs = backend.list_subscriptions().await.expect("Failed to list subscriptions");
    assert_eq!(subs.len(), 2);
    
    let push_subs: Vec<_> = subs.iter().filter(|s| s.push_config.is_some()).collect();
    let pull_subs: Vec<_> = subs.iter().filter(|s| s.push_config.is_none()).collect();
    assert_eq!(push_subs.len(), 1);
    assert_eq!(pull_subs.len(), 1);
}

#[tokio::test]
async fn test_sqlite_delete_subscription() {
    let config = SqliteConfig {
        database_path: ":memory:".to_string(),
        max_connections: 5,
    };
    
    let backend = SqliteBackend::new(config).await.expect("Failed to create backend");
    
    let sub = SubscriptionConfig {
        id: "test:delete-me".to_string(),
        name: "test-delete-me".to_string(),
        topic_id: "test:topic".to_string(),
        ack_deadline_seconds: 30,
        message_retention_duration: 604800,
        enable_message_ordering: false,
        filter: None,
        dead_letter_policy: None,
        push_config: None,
    };
    
    backend.create_subscription(sub.clone()).await.expect("Failed to create subscription");
    backend.delete_subscription(&sub.id).await.expect("Failed to delete subscription");
    
    let result = backend.get_subscription(&sub.id).await;
    assert!(result.is_err());
}
