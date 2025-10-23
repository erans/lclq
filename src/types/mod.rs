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
}

/// Dead letter queue configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqConfig {
    /// Target DLQ ID.
    pub target_queue_id: String,
    /// Maximum receive count before moving to DLQ.
    pub max_receive_count: u32,
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
}

/// Dead letter policy for Pub/Sub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterPolicy {
    /// Dead letter topic ID.
    pub dead_letter_topic: String,
    /// Max delivery attempts.
    pub max_delivery_attempts: u32,
}
