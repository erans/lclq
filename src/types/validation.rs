//! Validation functions for queue names, topic IDs, and other parameters.

use crate::error::ValidationError;
use crate::Result;

/// SQS queue name validation (1-80 chars, alphanumeric + - and _).
pub fn validate_sqs_queue_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 80 {
        return Err(ValidationError::InvalidQueueName(format!(
            "Queue name must be 1-80 characters, got {}",
            name.len()
        ))
        .into());
    }

    // FIFO queues must end with .fifo and base name must be ≤75 chars
    if let Some(base_name) = name.strip_suffix(".fifo") {
        if base_name.is_empty() || base_name.len() > 75 {
            return Err(ValidationError::InvalidQueueName(format!(
                "FIFO queue base name must be 1-75 characters, got {}",
                base_name.len()
            ))
            .into());
        }
        validate_queue_name_chars(base_name)?;
    } else {
        validate_queue_name_chars(name)?;
    }

    Ok(())
}

/// Validate queue name characters (alphanumeric, hyphen, underscore).
fn validate_queue_name_chars(name: &str) -> Result<()> {
    for ch in name.chars() {
        if !ch.is_alphanumeric() && ch != '-' && ch != '_' {
            return Err(ValidationError::InvalidQueueName(format!(
                "Queue name contains invalid character: '{}'",
                ch
            ))
            .into());
        }
    }
    Ok(())
}

/// Pub/Sub topic ID validation (3-255 chars, must start with letter).
pub fn validate_pubsub_topic_id(topic_id: &str) -> Result<()> {
    if topic_id.len() < 3 || topic_id.len() > 255 {
        return Err(ValidationError::InvalidTopicId(format!(
            "Topic ID must be 3-255 characters, got {}",
            topic_id.len()
        ))
        .into());
    }

    let mut chars = topic_id.chars();
    if let Some(first) = chars.next() {
        if !first.is_ascii_alphabetic() {
            return Err(ValidationError::InvalidTopicId(
                "Topic ID must start with a letter".to_string(),
            )
            .into());
        }
    }

    for ch in topic_id.chars() {
        if !ch.is_alphanumeric() && ch != '-' && ch != '_' && ch != '.' && ch != '~' && ch != '+' && ch != '%' {
            return Err(ValidationError::InvalidTopicId(format!(
                "Topic ID contains invalid character: '{}'",
                ch
            ))
            .into());
        }
    }

    Ok(())
}

/// Pub/Sub subscription ID validation (3-255 chars, must start with letter).
pub fn validate_pubsub_subscription_id(subscription_id: &str) -> Result<()> {
    if subscription_id.len() < 3 || subscription_id.len() > 255 {
        return Err(ValidationError::InvalidSubscriptionId(format!(
            "Subscription ID must be 3-255 characters, got {}",
            subscription_id.len()
        ))
        .into());
    }

    let mut chars = subscription_id.chars();
    if let Some(first) = chars.next() {
        if !first.is_ascii_alphabetic() {
            return Err(ValidationError::InvalidSubscriptionId(
                "Subscription ID must start with a letter".to_string(),
            )
            .into());
        }
    }

    for ch in subscription_id.chars() {
        if !ch.is_alphanumeric() && ch != '-' && ch != '_' && ch != '.' && ch != '~' && ch != '+' && ch != '%' {
            return Err(ValidationError::InvalidSubscriptionId(format!(
                "Subscription ID contains invalid character: '{}'",
                ch
            ))
            .into());
        }
    }

    Ok(())
}

/// Validate message size (SQS: 256KB, Pub/Sub: 10MB).
pub fn validate_message_size(size: usize, max_size: usize) -> Result<()> {
    if size > max_size {
        return Err(
            ValidationError::MessageTooLarge { size, max: max_size }.into()
        );
    }
    Ok(())
}

/// SQS maximum message size (256 KB).
pub const SQS_MAX_MESSAGE_SIZE: usize = 262_144;

/// Pub/Sub maximum message size (10 MB).
pub const PUBSUB_MAX_MESSAGE_SIZE: usize = 10_485_760;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqs_queue_name_validation() {
        // Valid names
        assert!(validate_sqs_queue_name("my-queue").is_ok());
        assert!(validate_sqs_queue_name("my_queue_123").is_ok());
        assert!(validate_sqs_queue_name("a").is_ok());
        assert!(validate_sqs_queue_name("my-queue.fifo").is_ok());

        // Invalid names
        assert!(validate_sqs_queue_name("").is_err());
        assert!(validate_sqs_queue_name(&"a".repeat(81)).is_err());
        assert!(validate_sqs_queue_name("my queue").is_err());
        assert!(validate_sqs_queue_name("my@queue").is_err());
        assert!(validate_sqs_queue_name(&format!("{}.fifo", "a".repeat(76))).is_err());
    }

    #[test]
    fn test_pubsub_topic_id_validation() {
        // Valid IDs
        assert!(validate_pubsub_topic_id("abc").is_ok());
        assert!(validate_pubsub_topic_id("my-topic").is_ok());
        assert!(validate_pubsub_topic_id("my_topic_123").is_ok());

        // Invalid IDs
        assert!(validate_pubsub_topic_id("ab").is_err()); // too short
        assert!(validate_pubsub_topic_id(&"a".repeat(256)).is_err()); // too long
        assert!(validate_pubsub_topic_id("123topic").is_err()); // must start with letter
        assert!(validate_pubsub_topic_id("_topic").is_err()); // must start with letter
    }

    #[test]
    fn test_message_size_validation() {
        assert!(validate_message_size(1024, SQS_MAX_MESSAGE_SIZE).is_ok());
        assert!(validate_message_size(SQS_MAX_MESSAGE_SIZE, SQS_MAX_MESSAGE_SIZE).is_ok());
        assert!(validate_message_size(SQS_MAX_MESSAGE_SIZE + 1, SQS_MAX_MESSAGE_SIZE).is_err());
    }
}
