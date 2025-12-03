//! Common data types for lclq.

pub mod validation;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique message identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Create a new random message ID.
    pub fn new() -> Self {
        MessageId(Uuid::new_v4().to_string())
    }

    /// Create a message ID from a string.
    pub fn from_string(s: String) -> Self {
        MessageId(s)
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Message attributes.
pub type MessageAttributes = HashMap<String, MessageAttributeValue>;

/// Message attribute value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttributeValue {
    /// Data type (String, Number, Binary).
    pub data_type: String,
    /// String value.
    pub string_value: Option<String>,
    /// Binary value.
    pub binary_value: Option<Vec<u8>>,
}

/// A message in the queue system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID.
    pub id: MessageId,
    /// Message body.
    pub body: String,
    /// Message attributes.
    pub attributes: MessageAttributes,
    /// Queue ID this message belongs to.
    pub queue_id: String,
    /// Timestamp when message was sent.
    pub sent_timestamp: DateTime<Utc>,
    /// Number of times this message has been received.
    pub receive_count: u32,
    /// FIFO: Message group ID.
    pub message_group_id: Option<String>,
    /// FIFO: Message deduplication ID.
    pub deduplication_id: Option<String>,
    /// FIFO: Sequence number.
    pub sequence_number: Option<u64>,
    /// Delay seconds (0-900).
    pub delay_seconds: Option<u32>,
}

/// Queue type enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueType {
    /// AWS SQS Standard queue.
    SqsStandard,
    /// AWS SQS FIFO queue.
    SqsFifo,
    /// GCP Pub/Sub topic.
    PubSubTopic,
}

/// Queue configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Queue ID.
    pub id: String,
    /// Queue name.
    pub name: String,
    /// Queue type.
    pub queue_type: QueueType,
    /// Visibility timeout in seconds (default: 30).
    pub visibility_timeout: u32,
    /// Message retention period in seconds (default: 345600 = 4 days).
    pub message_retention_period: u32,
    /// Maximum message size in bytes (default: 262144 = 256KB).
    pub max_message_size: usize,
    /// Delay seconds (0-900).
    pub delay_seconds: u32,
    /// Dead letter queue configuration.
    pub dlq_config: Option<DlqConfig>,
    /// FIFO: Content-based deduplication enabled.
    pub content_based_deduplication: bool,
    /// Tags.
    pub tags: HashMap<String, String>,
    /// Redrive allow policy - controls which queues can use this as a DLQ.
    pub redrive_allow_policy: Option<RedriveAllowPolicy>,
}

/// Dead letter queue configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqConfig {
    /// Target DLQ ID.
    pub target_queue_id: String,
    /// Maximum receive count before moving to DLQ.
    pub max_receive_count: u32,
}

/// Redrive permission type for dead letter queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RedrivePermission {
    /// Allow all source queues to use this queue as DLQ.
    AllowAll,
    /// Deny all source queues from using this queue as DLQ.
    DenyAll,
    /// Allow only specified source queues to use this queue as DLQ.
    ByQueue {
        /// List of source queue ARNs allowed to use this as DLQ.
        source_queue_arns: Vec<String>,
    },
}

/// Redrive allow policy - controls which source queues can use this queue as a DLQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedriveAllowPolicy {
    /// The permission type.
    pub permission: RedrivePermission,
}

/// Options for receiving messages.
#[derive(Debug, Clone)]
pub struct ReceiveOptions {
    /// Maximum number of messages to receive (1-10).
    pub max_messages: u32,
    /// Visibility timeout in seconds.
    pub visibility_timeout: Option<u32>,
    /// Wait time for long polling in seconds (0-20).
    pub wait_time_seconds: u32,
    /// Attribute names to return.
    pub attribute_names: Vec<String>,
    /// Message attribute names to return.
    pub message_attribute_names: Vec<String>,
}

impl Default for ReceiveOptions {
    fn default() -> Self {
        Self {
            max_messages: 1,
            visibility_timeout: None,
            wait_time_seconds: 0,
            attribute_names: vec![],
            message_attribute_names: vec![],
        }
    }
}

/// Queue statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    /// Number of messages available.
    pub available_messages: u64,
    /// Number of messages in flight.
    pub in_flight_messages: u64,
    /// Number of messages in DLQ.
    pub dlq_messages: u64,
    /// Oldest message timestamp.
    pub oldest_message_timestamp: Option<DateTime<Utc>>,
}

/// Subscription configuration (for Pub/Sub).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    /// Subscription ID.
    pub id: String,
    /// Subscription name.
    pub name: String,
    /// Topic ID.
    pub topic_id: String,
    /// Ack deadline in seconds (10-600).
    pub ack_deadline_seconds: u32,
    /// Message retention duration in seconds.
    pub message_retention_duration: u32,
    /// Enable message ordering.
    pub enable_message_ordering: bool,
    /// Filter expression.
    pub filter: Option<String>,
    /// Dead letter policy.
    pub dead_letter_policy: Option<DeadLetterPolicy>,
    /// Push configuration (None = pull subscription).
    pub push_config: Option<PushConfig>,
}

/// Dead letter policy for Pub/Sub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterPolicy {
    /// Dead letter topic ID.
    pub dead_letter_topic: String,
    /// Max delivery attempts.
    pub max_delivery_attempts: u32,
}

/// Push configuration for push subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// Push endpoint URL (HTTP or HTTPS).
    pub endpoint: String,
    /// Retry policy for failed deliveries.
    pub retry_policy: Option<RetryPolicy>,
    /// HTTP request timeout in seconds.
    pub timeout_seconds: Option<u32>,
}

/// Retry policy for push deliveries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Minimum backoff delay in seconds (default: 10).
    pub min_backoff_seconds: u32,
    /// Maximum backoff delay in seconds (default: 600).
    pub max_backoff_seconds: u32,
    /// Maximum delivery attempts (default: 5).
    pub max_attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            min_backoff_seconds: 10,
            max_backoff_seconds: 600,
            max_attempts: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_new() {
        let id1 = MessageId::new();
        let id2 = MessageId::new();

        // Each ID should be unique
        assert_ne!(id1, id2);

        // Should be valid UUID format (36 chars with dashes)
        assert_eq!(id1.0.len(), 36);
    }

    #[test]
    fn test_message_id_from_string() {
        let id_str = "test-message-id".to_string();
        let id = MessageId::from_string(id_str.clone());

        assert_eq!(id.0, id_str);
    }

    #[test]
    fn test_message_id_default() {
        let id = MessageId::default();

        // Default should create a new random ID
        assert_eq!(id.0.len(), 36);
    }

    #[test]
    fn test_message_id_display() {
        let id_str = "test-display-id".to_string();
        let id = MessageId::from_string(id_str.clone());

        assert_eq!(format!("{}", id), id_str);
    }

    #[test]
    fn test_message_id_clone_and_eq() {
        let id = MessageId::from_string("test-id".to_string());
        let cloned = id.clone();

        assert_eq!(id, cloned);
    }

    #[test]
    fn test_receive_options_default() {
        let opts = ReceiveOptions::default();

        assert_eq!(opts.max_messages, 1);
        assert_eq!(opts.visibility_timeout, None);
        assert_eq!(opts.wait_time_seconds, 0);
        assert_eq!(opts.attribute_names.len(), 0);
        assert_eq!(opts.message_attribute_names.len(), 0);
    }

    #[test]
    fn test_push_config_validation() {
        let push_config = PushConfig {
            endpoint: "https://example.com/webhook".to_string(),
            retry_policy: Some(RetryPolicy::default()),
            timeout_seconds: Some(30),
        };

        assert_eq!(push_config.endpoint, "https://example.com/webhook");
        assert!(push_config.retry_policy.is_some());
        assert_eq!(push_config.timeout_seconds, Some(30));
    }

    #[test]
    fn test_retry_policy_defaults() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.min_backoff_seconds, 10);
        assert_eq!(policy.max_backoff_seconds, 600);
        assert_eq!(policy.max_attempts, 5);
    }

    #[test]
    fn test_subscription_config_with_push() {
        let config = SubscriptionConfig {
            id: "test-sub".to_string(),
            name: "test-sub".to_string(),
            topic_id: "test-topic".to_string(),
            ack_deadline_seconds: 30,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: Some(PushConfig {
                endpoint: "https://example.com/hook".to_string(),
                retry_policy: None,
                timeout_seconds: None,
            }),
        };

        assert!(config.push_config.is_some());
        assert_eq!(
            config.push_config.unwrap().endpoint,
            "https://example.com/hook"
        );
    }
}
