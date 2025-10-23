//! AWS SQS-specific data types.

use base64::Engine;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SQS action types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqsAction {
    /// Create a new queue.
    CreateQueue,
    /// Delete a queue.
    DeleteQueue,
    /// Get queue URL from name.
    GetQueueUrl,
    /// Get queue attributes.
    GetQueueAttributes,
    /// Set queue attributes.
    SetQueueAttributes,
    /// List queues.
    ListQueues,
    /// Purge queue.
    PurgeQueue,
    /// Tag queue.
    TagQueue,
    /// Untag queue.
    UntagQueue,
    /// List queue tags.
    ListQueueTags,
    /// Send a message.
    SendMessage,
    /// Send multiple messages.
    SendMessageBatch,
    /// Receive messages.
    ReceiveMessage,
    /// Delete a message.
    DeleteMessage,
    /// Delete multiple messages.
    DeleteMessageBatch,
    /// Change message visibility.
    ChangeMessageVisibility,
    /// Change message visibility for multiple messages.
    ChangeMessageVisibilityBatch,
}

impl SqsAction {
    /// Parse action from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "CreateQueue" => Some(Self::CreateQueue),
            "DeleteQueue" => Some(Self::DeleteQueue),
            "GetQueueUrl" => Some(Self::GetQueueUrl),
            "GetQueueAttributes" => Some(Self::GetQueueAttributes),
            "SetQueueAttributes" => Some(Self::SetQueueAttributes),
            "ListQueues" => Some(Self::ListQueues),
            "PurgeQueue" => Some(Self::PurgeQueue),
            "TagQueue" => Some(Self::TagQueue),
            "UntagQueue" => Some(Self::UntagQueue),
            "ListQueueTags" => Some(Self::ListQueueTags),
            "SendMessage" => Some(Self::SendMessage),
            "SendMessageBatch" => Some(Self::SendMessageBatch),
            "ReceiveMessage" => Some(Self::ReceiveMessage),
            "DeleteMessage" => Some(Self::DeleteMessage),
            "DeleteMessageBatch" => Some(Self::DeleteMessageBatch),
            "ChangeMessageVisibility" => Some(Self::ChangeMessageVisibility),
            "ChangeMessageVisibilityBatch" => Some(Self::ChangeMessageVisibilityBatch),
            _ => None,
        }
    }

    /// Convert action to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CreateQueue => "CreateQueue",
            Self::DeleteQueue => "DeleteQueue",
            Self::GetQueueUrl => "GetQueueUrl",
            Self::GetQueueAttributes => "GetQueueAttributes",
            Self::SetQueueAttributes => "SetQueueAttributes",
            Self::ListQueues => "ListQueues",
            Self::PurgeQueue => "PurgeQueue",
            Self::TagQueue => "TagQueue",
            Self::UntagQueue => "UntagQueue",
            Self::ListQueueTags => "ListQueueTags",
            Self::SendMessage => "SendMessage",
            Self::SendMessageBatch => "SendMessageBatch",
            Self::ReceiveMessage => "ReceiveMessage",
            Self::DeleteMessage => "DeleteMessage",
            Self::DeleteMessageBatch => "DeleteMessageBatch",
            Self::ChangeMessageVisibility => "ChangeMessageVisibility",
            Self::ChangeMessageVisibilityBatch => "ChangeMessageVisibilityBatch",
        }
    }
}

/// SQS message attribute value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsMessageAttributeValue {
    /// Data type (String, Number, Binary).
    #[serde(rename = "DataType")]
    pub data_type: String,
    /// String value.
    #[serde(rename = "StringValue", skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    /// Binary value (base64 encoded).
    #[serde(rename = "BinaryValue", skip_serializing_if = "Option::is_none")]
    pub binary_value: Option<String>,
}

/// SQS message attributes.
pub type SqsMessageAttributes = HashMap<String, SqsMessageAttributeValue>;

/// SQS queue attribute names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueAttribute {
    /// All attributes.
    All,
    /// Approximate number of messages.
    ApproximateNumberOfMessages,
    /// Approximate number of messages not visible.
    ApproximateNumberOfMessagesNotVisible,
    /// Approximate number of messages delayed.
    ApproximateNumberOfMessagesDelayed,
    /// Created timestamp.
    CreatedTimestamp,
    /// Last modified timestamp.
    LastModifiedTimestamp,
    /// Visibility timeout.
    VisibilityTimeout,
    /// Maximum message size.
    MaximumMessageSize,
    /// Message retention period.
    MessageRetentionPeriod,
    /// Delay seconds.
    DelaySeconds,
    /// Redrive policy.
    RedrivePolicy,
    /// FIFO queue flag.
    FifoQueue,
    /// Content-based deduplication.
    ContentBasedDeduplication,
    /// Queue ARN.
    QueueArn,
}

impl QueueAttribute {
    /// Parse attribute from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "All" => Some(Self::All),
            "ApproximateNumberOfMessages" => Some(Self::ApproximateNumberOfMessages),
            "ApproximateNumberOfMessagesNotVisible" => {
                Some(Self::ApproximateNumberOfMessagesNotVisible)
            }
            "ApproximateNumberOfMessagesDelayed" => {
                Some(Self::ApproximateNumberOfMessagesDelayed)
            }
            "CreatedTimestamp" => Some(Self::CreatedTimestamp),
            "LastModifiedTimestamp" => Some(Self::LastModifiedTimestamp),
            "VisibilityTimeout" => Some(Self::VisibilityTimeout),
            "MaximumMessageSize" => Some(Self::MaximumMessageSize),
            "MessageRetentionPeriod" => Some(Self::MessageRetentionPeriod),
            "DelaySeconds" => Some(Self::DelaySeconds),
            "RedrivePolicy" => Some(Self::RedrivePolicy),
            "FifoQueue" => Some(Self::FifoQueue),
            "ContentBasedDeduplication" => Some(Self::ContentBasedDeduplication),
            "QueueArn" => Some(Self::QueueArn),
            _ => None,
        }
    }

    /// Convert attribute to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::ApproximateNumberOfMessages => "ApproximateNumberOfMessages",
            Self::ApproximateNumberOfMessagesNotVisible => {
                "ApproximateNumberOfMessagesNotVisible"
            }
            Self::ApproximateNumberOfMessagesDelayed => "ApproximateNumberOfMessagesDelayed",
            Self::CreatedTimestamp => "CreatedTimestamp",
            Self::LastModifiedTimestamp => "LastModifiedTimestamp",
            Self::VisibilityTimeout => "VisibilityTimeout",
            Self::MaximumMessageSize => "MaximumMessageSize",
            Self::MessageRetentionPeriod => "MessageRetentionPeriod",
            Self::DelaySeconds => "DelaySeconds",
            Self::RedrivePolicy => "RedrivePolicy",
            Self::FifoQueue => "FifoQueue",
            Self::ContentBasedDeduplication => "ContentBasedDeduplication",
            Self::QueueArn => "QueueArn",
        }
    }
}

/// SQS error codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqsErrorCode {
    /// Queue does not exist.
    QueueDoesNotExist,
    /// Queue already exists.
    QueueAlreadyExists,
    /// Invalid parameter value.
    InvalidParameterValue,
    /// Missing parameter.
    MissingParameter,
    /// Invalid attribute name.
    InvalidAttributeName,
    /// Message not found.
    ReceiptHandleIsInvalid,
    /// Batch request too long.
    BatchRequestTooLong,
    /// Internal error.
    InternalError,
}

impl SqsErrorCode {
    /// Convert error code to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QueueDoesNotExist => "AWS.SimpleQueueService.NonExistentQueue",
            Self::QueueAlreadyExists => "QueueAlreadyExists",
            Self::InvalidParameterValue => "InvalidParameterValue",
            Self::MissingParameter => "MissingParameter",
            Self::InvalidAttributeName => "InvalidAttributeName",
            Self::ReceiptHandleIsInvalid => "ReceiptHandleIsInvalid",
            Self::BatchRequestTooLong => "AWS.SimpleQueueService.BatchRequestTooLong",
            Self::InternalError => "InternalError",
        }
    }
}

/// Calculate MD5 hash of message body.
pub fn calculate_md5_of_body(body: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Calculate MD5 hash of message attributes.
pub fn calculate_md5_of_attributes(attributes: &SqsMessageAttributes) -> String {
    // AWS SQS calculates MD5 of attributes in a specific way:
    // 1. Sort attribute names
    // 2. For each attribute: name_length + name + type_length + type + value_length + value
    // 3. MD5 hash of the concatenated bytes

    let mut sorted_attrs: Vec<_> = attributes.iter().collect();
    sorted_attrs.sort_by_key(|(name, _)| *name);

    let mut buffer = Vec::new();

    for (name, value) in sorted_attrs {
        // Name length (4 bytes)
        buffer.extend_from_slice(&(name.len() as u32).to_be_bytes());
        // Name
        buffer.extend_from_slice(name.as_bytes());

        // Type length (4 bytes)
        buffer.extend_from_slice(&(value.data_type.len() as u32).to_be_bytes());
        // Type
        buffer.extend_from_slice(value.data_type.as_bytes());

        // Value
        if let Some(ref string_value) = value.string_value {
            // String value
            buffer.extend_from_slice(&(string_value.len() as u32).to_be_bytes());
            buffer.extend_from_slice(string_value.as_bytes());
        } else if let Some(ref binary_value) = value.binary_value {
            // Binary value (base64 decoded)
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(binary_value) {
                buffer.extend_from_slice(&(decoded.len() as u32).to_be_bytes());
                buffer.extend_from_slice(&decoded);
            }
        }
    }

    let mut hasher = Md5::new();
    hasher.update(&buffer);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqs_action_parsing() {
        assert_eq!(
            SqsAction::from_str("CreateQueue"),
            Some(SqsAction::CreateQueue)
        );
        assert_eq!(
            SqsAction::from_str("SendMessage"),
            Some(SqsAction::SendMessage)
        );
        assert_eq!(SqsAction::from_str("InvalidAction"), None);
    }

    #[test]
    fn test_queue_attribute_parsing() {
        assert_eq!(
            QueueAttribute::from_str("VisibilityTimeout"),
            Some(QueueAttribute::VisibilityTimeout)
        );
        assert_eq!(QueueAttribute::from_str("All"), Some(QueueAttribute::All));
        assert_eq!(QueueAttribute::from_str("InvalidAttr"), None);
    }

    #[test]
    fn test_md5_calculation() {
        let body = "Hello, World!";
        let md5 = calculate_md5_of_body(body);
        assert_eq!(md5, "65a8e27d8879283831b664bd8b7f0ad4");
    }

    #[test]
    fn test_md5_of_attributes() {
        let mut attrs = SqsMessageAttributes::new();
        attrs.insert(
            "test".to_string(),
            SqsMessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some("value".to_string()),
                binary_value: None,
            },
        );

        let md5 = calculate_md5_of_attributes(&attrs);
        assert!(!md5.is_empty());
        assert_eq!(md5.len(), 32); // MD5 is 32 hex chars
    }
}
