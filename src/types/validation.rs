//! Validation functions for queue names, topic IDs, and other parameters.

use crate::Result;
use crate::error::ValidationError;

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
        if !ch.is_alphanumeric()
            && ch != '-'
            && ch != '_'
            && ch != '.'
            && ch != '~'
            && ch != '+'
            && ch != '%'
        {
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
        if !ch.is_alphanumeric()
            && ch != '-'
            && ch != '_'
            && ch != '.'
            && ch != '~'
            && ch != '+'
            && ch != '%'
        {
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
        return Err(ValidationError::MessageTooLarge {
            size,
            max: max_size,
        }
        .into());
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

    // ========================================================================
    // Additional SQS Queue Name Tests
    // ========================================================================

    #[test]
    fn test_sqs_queue_name_edge_cases() {
        // Exactly 80 characters (valid)
        assert!(validate_sqs_queue_name(&"a".repeat(80)).is_ok());

        // FIFO queue with exactly 75 character base name
        assert!(validate_sqs_queue_name(&format!("{}.fifo", "a".repeat(75))).is_ok());

        // FIFO queue with empty base name
        assert!(validate_sqs_queue_name(".fifo").is_err());

        // Valid characters (alphanumeric, hyphen, underscore)
        assert!(validate_sqs_queue_name("Queue-Name_123").is_ok());

        // Invalid special characters
        assert!(validate_sqs_queue_name("queue.name").is_err());
        assert!(validate_sqs_queue_name("queue#name").is_err());
        assert!(validate_sqs_queue_name("queue!name").is_err());
    }

    #[test]
    fn test_sqs_fifo_queue_validation() {
        // Valid FIFO queues
        assert!(validate_sqs_queue_name("my-queue.fifo").is_ok());
        assert!(validate_sqs_queue_name("a.fifo").is_ok());

        // FIFO base name too long (76 chars + .fifo = 81 total)
        assert!(validate_sqs_queue_name(&format!("{}.fifo", "a".repeat(76))).is_err());

        // FIFO base name exactly at limit (75 chars)
        assert!(validate_sqs_queue_name(&format!("{}.fifo", "a".repeat(75))).is_ok());
    }

    // ========================================================================
    // Pub/Sub Topic ID Additional Tests
    // ========================================================================

    #[test]
    fn test_pubsub_topic_id_special_characters() {
        // Valid special characters (-, _, ., ~, +, %)
        assert!(validate_pubsub_topic_id("topic-name").is_ok());
        assert!(validate_pubsub_topic_id("topic_name").is_ok());
        assert!(validate_pubsub_topic_id("topic.name").is_ok());
        assert!(validate_pubsub_topic_id("topic~name").is_ok());
        assert!(validate_pubsub_topic_id("topic+name").is_ok());
        assert!(validate_pubsub_topic_id("topic%name").is_ok());

        // Invalid special characters
        assert!(validate_pubsub_topic_id("topic@name").is_err());
        assert!(validate_pubsub_topic_id("topic#name").is_err());
        assert!(validate_pubsub_topic_id("topic$name").is_err());
        assert!(validate_pubsub_topic_id("topic name").is_err());
    }

    #[test]
    fn test_pubsub_topic_id_length_boundaries() {
        // Exactly 3 characters (minimum valid)
        assert!(validate_pubsub_topic_id("abc").is_ok());

        // Exactly 255 characters (maximum valid)
        let max_valid = format!("a{}", "b".repeat(254));
        assert!(validate_pubsub_topic_id(&max_valid).is_ok());

        // 2 characters (too short)
        assert!(validate_pubsub_topic_id("ab").is_err());

        // 256 characters (too long)
        let too_long = format!("a{}", "b".repeat(255));
        assert!(validate_pubsub_topic_id(&too_long).is_err());
    }

    // ========================================================================
    // Pub/Sub Subscription ID Tests (previously untested!)
    // ========================================================================

    #[test]
    fn test_pubsub_subscription_id_validation() {
        // Valid subscription IDs
        assert!(validate_pubsub_subscription_id("abc").is_ok());
        assert!(validate_pubsub_subscription_id("my-subscription").is_ok());
        assert!(validate_pubsub_subscription_id("my_subscription_123").is_ok());
        assert!(validate_pubsub_subscription_id("subscription-name").is_ok());

        // Invalid subscription IDs - too short
        assert!(validate_pubsub_subscription_id("").is_err());
        assert!(validate_pubsub_subscription_id("a").is_err());
        assert!(validate_pubsub_subscription_id("ab").is_err());

        // Invalid subscription IDs - too long (256+ characters)
        assert!(validate_pubsub_subscription_id(&"a".repeat(256)).is_err());
        assert!(validate_pubsub_subscription_id(&"a".repeat(300)).is_err());

        // Invalid subscription IDs - must start with letter
        assert!(validate_pubsub_subscription_id("123sub").is_err());
        assert!(validate_pubsub_subscription_id("_subscription").is_err());
        assert!(validate_pubsub_subscription_id("-subscription").is_err());
        assert!(validate_pubsub_subscription_id("9sub").is_err());
    }

    #[test]
    fn test_pubsub_subscription_id_special_characters() {
        // Valid special characters (-, _, ., ~, +, %)
        assert!(validate_pubsub_subscription_id("sub-name").is_ok());
        assert!(validate_pubsub_subscription_id("sub_name").is_ok());
        assert!(validate_pubsub_subscription_id("sub.name").is_ok());
        assert!(validate_pubsub_subscription_id("sub~name").is_ok());
        assert!(validate_pubsub_subscription_id("sub+name").is_ok());
        assert!(validate_pubsub_subscription_id("sub%name").is_ok());

        // Combined special characters
        assert!(validate_pubsub_subscription_id("my-sub_123.test~v1+beta%20").is_ok());

        // Invalid special characters
        assert!(validate_pubsub_subscription_id("sub@name").is_err());
        assert!(validate_pubsub_subscription_id("sub#name").is_err());
        assert!(validate_pubsub_subscription_id("sub$name").is_err());
        assert!(validate_pubsub_subscription_id("sub name").is_err());
        assert!(validate_pubsub_subscription_id("sub!name").is_err());
        assert!(validate_pubsub_subscription_id("sub&name").is_err());
    }

    #[test]
    fn test_pubsub_subscription_id_length_boundaries() {
        // Exactly 3 characters (minimum valid)
        assert!(validate_pubsub_subscription_id("abc").is_ok());
        assert!(validate_pubsub_subscription_id("xyz").is_ok());

        // Exactly 255 characters (maximum valid)
        let max_valid = format!("s{}", "u".repeat(254));
        assert!(validate_pubsub_subscription_id(&max_valid).is_ok());

        // 256 characters (too long)
        let too_long = format!("s{}", "u".repeat(255));
        assert!(validate_pubsub_subscription_id(&too_long).is_err());
    }

    #[test]
    fn test_pubsub_subscription_id_alphanumeric() {
        // Valid alphanumeric combinations
        assert!(validate_pubsub_subscription_id("sub123").is_ok());
        assert!(validate_pubsub_subscription_id("subscription456").is_ok());
        assert!(validate_pubsub_subscription_id("mySubscription789").is_ok());

        // Must start with letter (not number)
        assert!(validate_pubsub_subscription_id("1subscription").is_err());
        assert!(validate_pubsub_subscription_id("0sub").is_err());
    }

    // ========================================================================
    // Message Size Additional Tests
    // ========================================================================

    #[test]
    fn test_message_size_edge_cases() {
        // Zero size (valid)
        assert!(validate_message_size(0, SQS_MAX_MESSAGE_SIZE).is_ok());

        // Exactly at SQS limit
        assert!(validate_message_size(SQS_MAX_MESSAGE_SIZE, SQS_MAX_MESSAGE_SIZE).is_ok());

        // One byte over SQS limit
        assert!(validate_message_size(SQS_MAX_MESSAGE_SIZE + 1, SQS_MAX_MESSAGE_SIZE).is_err());

        // Exactly at Pub/Sub limit
        assert!(validate_message_size(PUBSUB_MAX_MESSAGE_SIZE, PUBSUB_MAX_MESSAGE_SIZE).is_ok());

        // One byte over Pub/Sub limit
        assert!(
            validate_message_size(PUBSUB_MAX_MESSAGE_SIZE + 1, PUBSUB_MAX_MESSAGE_SIZE).is_err()
        );

        // Very large message
        assert!(validate_message_size(100_000_000, PUBSUB_MAX_MESSAGE_SIZE).is_err());
    }

    #[test]
    fn test_message_size_constants() {
        // Verify the constant values match expected sizes
        assert_eq!(SQS_MAX_MESSAGE_SIZE, 262_144); // 256 KB
        assert_eq!(PUBSUB_MAX_MESSAGE_SIZE, 10_485_760); // 10 MB
    }
}
