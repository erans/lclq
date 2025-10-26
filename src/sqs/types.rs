//! AWS SQS-specific data types.

use std::str::FromStr;

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

impl FromStr for SqsAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CreateQueue" => Ok(Self::CreateQueue),
            "DeleteQueue" => Ok(Self::DeleteQueue),
            "GetQueueUrl" => Ok(Self::GetQueueUrl),
            "GetQueueAttributes" => Ok(Self::GetQueueAttributes),
            "SetQueueAttributes" => Ok(Self::SetQueueAttributes),
            "ListQueues" => Ok(Self::ListQueues),
            "PurgeQueue" => Ok(Self::PurgeQueue),
            "TagQueue" => Ok(Self::TagQueue),
            "UntagQueue" => Ok(Self::UntagQueue),
            "ListQueueTags" => Ok(Self::ListQueueTags),
            "SendMessage" => Ok(Self::SendMessage),
            "SendMessageBatch" => Ok(Self::SendMessageBatch),
            "ReceiveMessage" => Ok(Self::ReceiveMessage),
            "DeleteMessage" => Ok(Self::DeleteMessage),
            "DeleteMessageBatch" => Ok(Self::DeleteMessageBatch),
            "ChangeMessageVisibility" => Ok(Self::ChangeMessageVisibility),
            "ChangeMessageVisibilityBatch" => Ok(Self::ChangeMessageVisibilityBatch),
            _ => Err(format!("Unknown SQS action: {}", s)),
        }
    }
}

impl SqsAction {
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
    /// Redrive allow policy.
    RedriveAllowPolicy,
    /// FIFO queue flag.
    FifoQueue,
    /// Content-based deduplication.
    ContentBasedDeduplication,
    /// Queue ARN.
    QueueArn,
}

impl FromStr for QueueAttribute {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "All" => Ok(Self::All),
            "ApproximateNumberOfMessages" => Ok(Self::ApproximateNumberOfMessages),
            "ApproximateNumberOfMessagesNotVisible" => {
                Ok(Self::ApproximateNumberOfMessagesNotVisible)
            }
            "ApproximateNumberOfMessagesDelayed" => {
                Ok(Self::ApproximateNumberOfMessagesDelayed)
            }
            "CreatedTimestamp" => Ok(Self::CreatedTimestamp),
            "LastModifiedTimestamp" => Ok(Self::LastModifiedTimestamp),
            "VisibilityTimeout" => Ok(Self::VisibilityTimeout),
            "MaximumMessageSize" => Ok(Self::MaximumMessageSize),
            "MessageRetentionPeriod" => Ok(Self::MessageRetentionPeriod),
            "DelaySeconds" => Ok(Self::DelaySeconds),
            "RedrivePolicy" => Ok(Self::RedrivePolicy),
            "RedriveAllowPolicy" => Ok(Self::RedriveAllowPolicy),
            "FifoQueue" => Ok(Self::FifoQueue),
            "ContentBasedDeduplication" => Ok(Self::ContentBasedDeduplication),
            "QueueArn" => Ok(Self::QueueArn),
            _ => Err(format!("Unknown queue attribute: {}", s)),
        }
    }
}

impl QueueAttribute {
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
            Self::RedriveAllowPolicy => "RedriveAllowPolicy",
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
            "CreateQueue".parse::<SqsAction>(),
            Ok(SqsAction::CreateQueue)
        );
        assert_eq!(
            "SendMessage".parse::<SqsAction>(),
            Ok(SqsAction::SendMessage)
        );
        assert!("InvalidAction".parse::<SqsAction>().is_err());
    }

    #[test]
    fn test_queue_attribute_parsing() {
        assert_eq!(
            "VisibilityTimeout".parse::<QueueAttribute>(),
            Ok(QueueAttribute::VisibilityTimeout)
        );
        assert_eq!("All".parse::<QueueAttribute>(), Ok(QueueAttribute::All));
        assert!("InvalidAttr".parse::<QueueAttribute>().is_err());
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

    // ========================================================================
    // SqsAction Tests
    // ========================================================================

    #[test]
    fn test_sqs_action_as_str_all_variants() {
        // Test all action variants to ensure as_str() works correctly
        assert_eq!(SqsAction::CreateQueue.as_str(), "CreateQueue");
        assert_eq!(SqsAction::DeleteQueue.as_str(), "DeleteQueue");
        assert_eq!(SqsAction::GetQueueUrl.as_str(), "GetQueueUrl");
        assert_eq!(SqsAction::GetQueueAttributes.as_str(), "GetQueueAttributes");
        assert_eq!(SqsAction::SetQueueAttributes.as_str(), "SetQueueAttributes");
        assert_eq!(SqsAction::ListQueues.as_str(), "ListQueues");
        assert_eq!(SqsAction::PurgeQueue.as_str(), "PurgeQueue");
        assert_eq!(SqsAction::TagQueue.as_str(), "TagQueue");
        assert_eq!(SqsAction::UntagQueue.as_str(), "UntagQueue");
        assert_eq!(SqsAction::ListQueueTags.as_str(), "ListQueueTags");
        assert_eq!(SqsAction::SendMessage.as_str(), "SendMessage");
        assert_eq!(SqsAction::SendMessageBatch.as_str(), "SendMessageBatch");
        assert_eq!(SqsAction::ReceiveMessage.as_str(), "ReceiveMessage");
        assert_eq!(SqsAction::DeleteMessage.as_str(), "DeleteMessage");
        assert_eq!(SqsAction::DeleteMessageBatch.as_str(), "DeleteMessageBatch");
        assert_eq!(SqsAction::ChangeMessageVisibility.as_str(), "ChangeMessageVisibility");
        assert_eq!(SqsAction::ChangeMessageVisibilityBatch.as_str(), "ChangeMessageVisibilityBatch");
    }

    #[test]
    fn test_sqs_action_from_str_all_variants() {
        // Test all action variants parsing
        assert_eq!("CreateQueue".parse::<SqsAction>().unwrap(), SqsAction::CreateQueue);
        assert_eq!("DeleteQueue".parse::<SqsAction>().unwrap(), SqsAction::DeleteQueue);
        assert_eq!("GetQueueUrl".parse::<SqsAction>().unwrap(), SqsAction::GetQueueUrl);
        assert_eq!("GetQueueAttributes".parse::<SqsAction>().unwrap(), SqsAction::GetQueueAttributes);
        assert_eq!("SetQueueAttributes".parse::<SqsAction>().unwrap(), SqsAction::SetQueueAttributes);
        assert_eq!("ListQueues".parse::<SqsAction>().unwrap(), SqsAction::ListQueues);
        assert_eq!("PurgeQueue".parse::<SqsAction>().unwrap(), SqsAction::PurgeQueue);
        assert_eq!("TagQueue".parse::<SqsAction>().unwrap(), SqsAction::TagQueue);
        assert_eq!("UntagQueue".parse::<SqsAction>().unwrap(), SqsAction::UntagQueue);
        assert_eq!("ListQueueTags".parse::<SqsAction>().unwrap(), SqsAction::ListQueueTags);
        assert_eq!("SendMessage".parse::<SqsAction>().unwrap(), SqsAction::SendMessage);
        assert_eq!("SendMessageBatch".parse::<SqsAction>().unwrap(), SqsAction::SendMessageBatch);
        assert_eq!("ReceiveMessage".parse::<SqsAction>().unwrap(), SqsAction::ReceiveMessage);
        assert_eq!("DeleteMessage".parse::<SqsAction>().unwrap(), SqsAction::DeleteMessage);
        assert_eq!("DeleteMessageBatch".parse::<SqsAction>().unwrap(), SqsAction::DeleteMessageBatch);
        assert_eq!("ChangeMessageVisibility".parse::<SqsAction>().unwrap(), SqsAction::ChangeMessageVisibility);
        assert_eq!("ChangeMessageVisibilityBatch".parse::<SqsAction>().unwrap(), SqsAction::ChangeMessageVisibilityBatch);
    }

    #[test]
    fn test_sqs_action_from_str_error() {
        // Test unknown action parsing
        let result = "UnknownAction".parse::<SqsAction>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown SQS action"));
    }

    // ========================================================================
    // QueueAttribute Tests
    // ========================================================================

    #[test]
    fn test_queue_attribute_as_str_all_variants() {
        // Test all queue attribute variants
        assert_eq!(QueueAttribute::All.as_str(), "All");
        assert_eq!(QueueAttribute::ApproximateNumberOfMessages.as_str(), "ApproximateNumberOfMessages");
        assert_eq!(QueueAttribute::ApproximateNumberOfMessagesNotVisible.as_str(), "ApproximateNumberOfMessagesNotVisible");
        assert_eq!(QueueAttribute::ApproximateNumberOfMessagesDelayed.as_str(), "ApproximateNumberOfMessagesDelayed");
        assert_eq!(QueueAttribute::CreatedTimestamp.as_str(), "CreatedTimestamp");
        assert_eq!(QueueAttribute::LastModifiedTimestamp.as_str(), "LastModifiedTimestamp");
        assert_eq!(QueueAttribute::VisibilityTimeout.as_str(), "VisibilityTimeout");
        assert_eq!(QueueAttribute::MaximumMessageSize.as_str(), "MaximumMessageSize");
        assert_eq!(QueueAttribute::MessageRetentionPeriod.as_str(), "MessageRetentionPeriod");
        assert_eq!(QueueAttribute::DelaySeconds.as_str(), "DelaySeconds");
        assert_eq!(QueueAttribute::RedrivePolicy.as_str(), "RedrivePolicy");
        assert_eq!(QueueAttribute::RedriveAllowPolicy.as_str(), "RedriveAllowPolicy");
        assert_eq!(QueueAttribute::FifoQueue.as_str(), "FifoQueue");
        assert_eq!(QueueAttribute::ContentBasedDeduplication.as_str(), "ContentBasedDeduplication");
        assert_eq!(QueueAttribute::QueueArn.as_str(), "QueueArn");
    }

    #[test]
    fn test_queue_attribute_from_str_all_variants() {
        // Test all queue attribute variants parsing
        assert_eq!("All".parse::<QueueAttribute>().unwrap(), QueueAttribute::All);
        assert_eq!("ApproximateNumberOfMessages".parse::<QueueAttribute>().unwrap(), QueueAttribute::ApproximateNumberOfMessages);
        assert_eq!("ApproximateNumberOfMessagesNotVisible".parse::<QueueAttribute>().unwrap(), QueueAttribute::ApproximateNumberOfMessagesNotVisible);
        assert_eq!("ApproximateNumberOfMessagesDelayed".parse::<QueueAttribute>().unwrap(), QueueAttribute::ApproximateNumberOfMessagesDelayed);
        assert_eq!("CreatedTimestamp".parse::<QueueAttribute>().unwrap(), QueueAttribute::CreatedTimestamp);
        assert_eq!("LastModifiedTimestamp".parse::<QueueAttribute>().unwrap(), QueueAttribute::LastModifiedTimestamp);
        assert_eq!("VisibilityTimeout".parse::<QueueAttribute>().unwrap(), QueueAttribute::VisibilityTimeout);
        assert_eq!("MaximumMessageSize".parse::<QueueAttribute>().unwrap(), QueueAttribute::MaximumMessageSize);
        assert_eq!("MessageRetentionPeriod".parse::<QueueAttribute>().unwrap(), QueueAttribute::MessageRetentionPeriod);
        assert_eq!("DelaySeconds".parse::<QueueAttribute>().unwrap(), QueueAttribute::DelaySeconds);
        assert_eq!("RedrivePolicy".parse::<QueueAttribute>().unwrap(), QueueAttribute::RedrivePolicy);
        assert_eq!("RedriveAllowPolicy".parse::<QueueAttribute>().unwrap(), QueueAttribute::RedriveAllowPolicy);
        assert_eq!("FifoQueue".parse::<QueueAttribute>().unwrap(), QueueAttribute::FifoQueue);
        assert_eq!("ContentBasedDeduplication".parse::<QueueAttribute>().unwrap(), QueueAttribute::ContentBasedDeduplication);
        assert_eq!("QueueArn".parse::<QueueAttribute>().unwrap(), QueueAttribute::QueueArn);
    }

    #[test]
    fn test_queue_attribute_from_str_error() {
        // Test unknown attribute parsing
        let result = "UnknownAttribute".parse::<QueueAttribute>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown queue attribute"));
    }

    // ========================================================================
    // SqsErrorCode Tests
    // ========================================================================

    #[test]
    fn test_sqs_error_code_as_str_all_variants() {
        // Test all error code variants
        assert_eq!(SqsErrorCode::QueueDoesNotExist.as_str(), "AWS.SimpleQueueService.NonExistentQueue");
        assert_eq!(SqsErrorCode::QueueAlreadyExists.as_str(), "QueueAlreadyExists");
        assert_eq!(SqsErrorCode::InvalidParameterValue.as_str(), "InvalidParameterValue");
        assert_eq!(SqsErrorCode::MissingParameter.as_str(), "MissingParameter");
        assert_eq!(SqsErrorCode::InvalidAttributeName.as_str(), "InvalidAttributeName");
        assert_eq!(SqsErrorCode::ReceiptHandleIsInvalid.as_str(), "ReceiptHandleIsInvalid");
        assert_eq!(SqsErrorCode::BatchRequestTooLong.as_str(), "AWS.SimpleQueueService.BatchRequestTooLong");
        assert_eq!(SqsErrorCode::InternalError.as_str(), "InternalError");
    }

    // ========================================================================
    // MD5 Calculation Tests
    // ========================================================================

    #[test]
    fn test_md5_of_body_various_inputs() {
        // Test empty string
        assert_eq!(calculate_md5_of_body(""), "d41d8cd98f00b204e9800998ecf8427e");

        // Test known MD5 values
        assert_eq!(calculate_md5_of_body("Hello, World!"), "65a8e27d8879283831b664bd8b7f0ad4");
        assert_eq!(calculate_md5_of_body("test"), "098f6bcd4621d373cade4e832627b4f6");
    }

    #[test]
    fn test_md5_of_attributes_with_binary_value() {
        // Test MD5 calculation with binary value (base64 encoded)
        let mut attrs = SqsMessageAttributes::new();

        // Base64 encoded "Hello"
        let binary_data = base64::engine::general_purpose::STANDARD.encode(b"Hello");
        attrs.insert(
            "binary_attr".to_string(),
            SqsMessageAttributeValue {
                data_type: "Binary".to_string(),
                string_value: None,
                binary_value: Some(binary_data),
            },
        );

        let md5 = calculate_md5_of_attributes(&attrs);
        assert!(!md5.is_empty());
        assert_eq!(md5.len(), 32); // MD5 is 32 hex chars
    }

    #[test]
    fn test_md5_of_attributes_with_invalid_base64() {
        // Test MD5 calculation with invalid base64 (should be skipped)
        let mut attrs = SqsMessageAttributes::new();

        attrs.insert(
            "invalid_binary".to_string(),
            SqsMessageAttributeValue {
                data_type: "Binary".to_string(),
                string_value: None,
                binary_value: Some("!!!invalid-base64!!!".to_string()),
            },
        );

        let md5 = calculate_md5_of_attributes(&attrs);
        // Should still calculate MD5, just skip the invalid value
        assert!(!md5.is_empty());
        assert_eq!(md5.len(), 32);
    }

    #[test]
    fn test_md5_of_attributes_sorting() {
        // Test that attribute names are sorted before MD5 calculation
        let mut attrs1 = SqsMessageAttributes::new();
        attrs1.insert("zebra".to_string(), SqsMessageAttributeValue {
            data_type: "String".to_string(),
            string_value: Some("value1".to_string()),
            binary_value: None,
        });
        attrs1.insert("alpha".to_string(), SqsMessageAttributeValue {
            data_type: "String".to_string(),
            string_value: Some("value2".to_string()),
            binary_value: None,
        });

        let mut attrs2 = SqsMessageAttributes::new();
        attrs2.insert("alpha".to_string(), SqsMessageAttributeValue {
            data_type: "String".to_string(),
            string_value: Some("value2".to_string()),
            binary_value: None,
        });
        attrs2.insert("zebra".to_string(), SqsMessageAttributeValue {
            data_type: "String".to_string(),
            string_value: Some("value1".to_string()),
            binary_value: None,
        });

        // MD5 should be the same regardless of insertion order
        assert_eq!(
            calculate_md5_of_attributes(&attrs1),
            calculate_md5_of_attributes(&attrs2)
        );
    }

    #[test]
    fn test_md5_of_attributes_empty() {
        // Test MD5 of empty attributes
        let attrs = SqsMessageAttributes::new();
        let md5 = calculate_md5_of_attributes(&attrs);
        assert!(!md5.is_empty());
        assert_eq!(md5.len(), 32);
        // Empty attributes should have a specific MD5
        assert_eq!(md5, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_md5_of_attributes_multiple_types() {
        // Test MD5 with multiple attribute types
        let mut attrs = SqsMessageAttributes::new();

        attrs.insert("string_attr".to_string(), SqsMessageAttributeValue {
            data_type: "String".to_string(),
            string_value: Some("test_value".to_string()),
            binary_value: None,
        });

        attrs.insert("number_attr".to_string(), SqsMessageAttributeValue {
            data_type: "Number".to_string(),
            string_value: Some("123".to_string()),
            binary_value: None,
        });

        let binary_data = base64::engine::general_purpose::STANDARD.encode(b"binary_data");
        attrs.insert("binary_attr".to_string(), SqsMessageAttributeValue {
            data_type: "Binary".to_string(),
            string_value: None,
            binary_value: Some(binary_data),
        });

        let md5 = calculate_md5_of_attributes(&attrs);
        assert!(!md5.is_empty());
        assert_eq!(md5.len(), 32);
    }
}
