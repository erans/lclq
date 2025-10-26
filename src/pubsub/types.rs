//! Custom types and utilities for Pub/Sub implementation.

use crate::error::{Error, Result, ValidationError};
use std::fmt;

/// Represents a parsed Pub/Sub resource name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceName {
    /// Topic resource: projects/{project}/topics/{topic}
    Topic {
        /// The GCP project ID
        project: String,
        /// The topic name
        topic: String,
    },
    /// Subscription resource: projects/{project}/subscriptions/{subscription}
    Subscription {
        /// The GCP project ID
        project: String,
        /// The subscription name
        subscription: String,
    },
    /// Snapshot resource: projects/{project}/snapshots/{snapshot}
    Snapshot {
        /// The GCP project ID
        project: String,
        /// The snapshot name
        snapshot: String,
    },
}

impl ResourceName {
    /// Parse a resource name string into a ResourceName.
    pub fn parse(name: &str) -> Result<Self> {
        let parts: Vec<&str> = name.split('/').collect();

        if parts.len() != 4 || parts[0] != "projects" {
            return Err(Error::Validation(ValidationError::InvalidParameter {
                name: "resource_name".to_string(),
                reason: format!("Invalid resource name format: {}", name),
            }));
        }

        let project = parts[1].to_string();
        let resource_type = parts[2];
        let resource_id = parts[3].to_string();

        match resource_type {
            "topics" => Ok(ResourceName::Topic {
                project,
                topic: resource_id,
            }),
            "subscriptions" => Ok(ResourceName::Subscription {
                project,
                subscription: resource_id,
            }),
            "snapshots" => Ok(ResourceName::Snapshot {
                project,
                snapshot: resource_id,
            }),
            _ => Err(Error::Validation(ValidationError::InvalidParameter {
                name: "resource_type".to_string(),
                reason: format!("Unknown resource type: {}", resource_type),
            })),
        }
    }

    /// Get the project ID from the resource name.
    pub fn project(&self) -> &str {
        match self {
            ResourceName::Topic { project, .. } => project,
            ResourceName::Subscription { project, .. } => project,
            ResourceName::Snapshot { project, .. } => project,
        }
    }

    /// Get the resource ID (topic, subscription, or snapshot name).
    pub fn resource_id(&self) -> &str {
        match self {
            ResourceName::Topic { topic, .. } => topic,
            ResourceName::Subscription { subscription, .. } => subscription,
            ResourceName::Snapshot { snapshot, .. } => snapshot,
        }
    }

    /// Format a topic resource name.
    pub fn topic(project: impl Into<String>, topic: impl Into<String>) -> String {
        format!("projects/{}/topics/{}", project.into(), topic.into())
    }

    /// Format a subscription resource name.
    pub fn subscription(
        project: impl Into<String>,
        subscription: impl Into<String>,
    ) -> String {
        format!(
            "projects/{}/subscriptions/{}",
            project.into(),
            subscription.into()
        )
    }

    /// Format a snapshot resource name.
    pub fn snapshot(project: impl Into<String>, snapshot: impl Into<String>) -> String {
        format!(
            "projects/{}/snapshots/{}",
            project.into(),
            snapshot.into()
        )
    }
}

impl fmt::Display for ResourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceName::Topic { project, topic } => {
                write!(f, "projects/{}/topics/{}", project, topic)
            }
            ResourceName::Subscription {
                project,
                subscription,
            } => {
                write!(f, "projects/{}/subscriptions/{}", project, subscription)
            }
            ResourceName::Snapshot { project, snapshot } => {
                write!(f, "projects/{}/snapshots/{}", project, snapshot)
            }
        }
    }
}

/// Validates a Pub/Sub topic ID.
///
/// Topic IDs must:
/// - Be 3-255 characters
/// - Start with a letter
/// - Contain only letters, numbers, hyphens, underscores, periods, tildes, plus, and percent
pub fn validate_topic_id(topic_id: &str) -> Result<()> {
    if topic_id.len() < 3 || topic_id.len() > 255 {
        return Err(Error::Validation(ValidationError::InvalidTopicId(
            "Topic ID must be 3-255 characters".to_string(),
        )));
    }

    let first_char = topic_id.chars().next()
        .expect("topic_id is guaranteed to be non-empty by length check above");
    if !first_char.is_ascii_alphabetic() {
        return Err(Error::Validation(ValidationError::InvalidTopicId(
            "Topic ID must start with a letter".to_string(),
        )));
    }

    for ch in topic_id.chars() {
        if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' | '+' | '%') {
            return Err(Error::Validation(ValidationError::InvalidTopicId(format!(
                "Topic ID contains invalid character: {}",
                ch
            ))));
        }
    }

    Ok(())
}

/// Validates a Pub/Sub subscription ID.
///
/// Subscription IDs must:
/// - Be 3-255 characters
/// - Start with a letter
/// - Contain only letters, numbers, hyphens, underscores, periods, tildes, plus, and percent
pub fn validate_subscription_id(subscription_id: &str) -> Result<()> {
    if subscription_id.len() < 3 || subscription_id.len() > 255 {
        return Err(Error::Validation(
            ValidationError::InvalidSubscriptionId(
                "Subscription ID must be 3-255 characters".to_string(),
            ),
        ));
    }

    let first_char = subscription_id.chars().next()
        .expect("subscription_id is guaranteed to be non-empty by length check above");
    if !first_char.is_ascii_alphabetic() {
        return Err(Error::Validation(
            ValidationError::InvalidSubscriptionId(
                "Subscription ID must start with a letter".to_string(),
            ),
        ));
    }

    for ch in subscription_id.chars() {
        if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' | '+' | '%') {
            return Err(Error::Validation(
                ValidationError::InvalidSubscriptionId(format!(
                    "Subscription ID contains invalid character: {}",
                    ch
                )),
            ));
        }
    }

    Ok(())
}

/// Validates a Pub/Sub project ID.
///
/// Project IDs must:
/// - Be 6-30 characters
/// - Start with a lowercase letter
/// - Contain only lowercase letters, numbers, and hyphens
pub fn validate_project_id(project_id: &str) -> Result<()> {
    if project_id.len() < 6 || project_id.len() > 30 {
        return Err(Error::Validation(ValidationError::InvalidParameter {
            name: "project_id".to_string(),
            reason: "Project ID must be 6-30 characters".to_string(),
        }));
    }

    let first_char = project_id.chars().next()
        .expect("project_id is guaranteed to be non-empty by length check above");
    if !first_char.is_ascii_lowercase() {
        return Err(Error::Validation(ValidationError::InvalidParameter {
            name: "project_id".to_string(),
            reason: "Project ID must start with a lowercase letter".to_string(),
        }));
    }

    for ch in project_id.chars() {
        if !matches!(ch, 'a'..='z' | '0'..='9' | '-') {
            return Err(Error::Validation(ValidationError::InvalidParameter {
                name: "project_id".to_string(),
                reason: format!("Project ID contains invalid character: {}", ch),
            }));
        }
    }

    Ok(())
}

/// Validates Pub/Sub message size.
///
/// Pub/Sub messages can be up to 10 MB.
pub fn validate_message_size(data: &[u8]) -> Result<()> {
    const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10 MB

    if data.len() > MAX_MESSAGE_SIZE {
        return Err(Error::Validation(ValidationError::MessageTooLarge {
            size: data.len(),
            max: MAX_MESSAGE_SIZE,
        }));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_name_parsing() {
        let name = "projects/test-project/topics/test-topic";
        let parsed = ResourceName::parse(name).unwrap();
        assert_eq!(parsed.project(), "test-project");
        assert_eq!(parsed.resource_id(), "test-topic");

        let name = "projects/test-project/subscriptions/test-sub";
        let parsed = ResourceName::parse(name).unwrap();
        assert_eq!(parsed.project(), "test-project");
        assert_eq!(parsed.resource_id(), "test-sub");
    }

    #[test]
    fn test_resource_name_formatting() {
        let topic = ResourceName::topic("test-project", "test-topic");
        assert_eq!(topic, "projects/test-project/topics/test-topic");

        let sub = ResourceName::subscription("test-project", "test-sub");
        assert_eq!(sub, "projects/test-project/subscriptions/test-sub");
    }

    #[test]
    fn test_topic_id_validation() {
        assert!(validate_topic_id("valid-topic").is_ok());
        assert!(validate_topic_id("t").is_err()); // Too short
        assert!(validate_topic_id("123topic").is_err()); // Must start with letter
        assert!(validate_topic_id("topic@invalid").is_err()); // Invalid character
    }

    #[test]
    fn test_subscription_id_validation() {
        assert!(validate_subscription_id("valid-subscription").is_ok());
        assert!(validate_subscription_id("su").is_err()); // Too short
        assert!(validate_subscription_id("123sub").is_err()); // Must start with letter
    }

    #[test]
    fn test_project_id_validation() {
        assert!(validate_project_id("test-project").is_ok());
        assert!(validate_project_id("short").is_err()); // Too short
        assert!(validate_project_id("TestProject").is_err()); // Must be lowercase
    }

    #[test]
    fn test_message_size_validation() {
        let small_msg = vec![0u8; 1024];
        assert!(validate_message_size(&small_msg).is_ok());

        let large_msg = vec![0u8; 11 * 1024 * 1024];
        assert!(validate_message_size(&large_msg).is_err());
    }

    #[test]
    fn test_resource_name_snapshot_parsing() {
        // Test snapshot parsing (lines 57-59)
        let name = "projects/test-project/snapshots/test-snapshot";
        let parsed = ResourceName::parse(name).unwrap();

        match &parsed {
            ResourceName::Snapshot { project, snapshot } => {
                assert_eq!(project, "test-project");
                assert_eq!(snapshot, "test-snapshot");
            }
            _ => panic!("Expected Snapshot variant"),
        }

        // Test snapshot project() method (line 73)
        assert_eq!(parsed.project(), "test-project");

        // Test snapshot resource_id() method (line 82)
        assert_eq!(parsed.resource_id(), "test-snapshot");
    }

    #[test]
    fn test_resource_name_unknown_type() {
        // Test unknown resource type error (lines 61-63)
        let name = "projects/test-project/unknown/test-resource";
        let result = ResourceName::parse(name);

        assert!(result.is_err());
        match result {
            Err(Error::Validation(ValidationError::InvalidParameter { name, reason })) => {
                assert_eq!(name, "resource_type");
                assert!(reason.contains("Unknown resource type"));
            }
            _ => panic!("Expected InvalidParameter error"),
        }
    }

    #[test]
    fn test_resource_name_snapshot_formatting() {
        // Test snapshot() formatting function (lines 104-108)
        let snapshot = ResourceName::snapshot("test-project", "test-snapshot");
        assert_eq!(snapshot, "projects/test-project/snapshots/test-snapshot");
    }

    #[test]
    fn test_resource_name_display() {
        // Test Display trait for Topic (lines 116-117)
        let topic = ResourceName::Topic {
            project: "test-project".to_string(),
            topic: "test-topic".to_string(),
        };
        assert_eq!(topic.to_string(), "projects/test-project/topics/test-topic");

        // Test Display trait for Subscription (lines 119-123)
        let subscription = ResourceName::Subscription {
            project: "test-project".to_string(),
            subscription: "test-sub".to_string(),
        };
        assert_eq!(subscription.to_string(), "projects/test-project/subscriptions/test-sub");

        // Test Display trait for Snapshot (lines 125-126)
        let snapshot = ResourceName::Snapshot {
            project: "test-project".to_string(),
            snapshot: "test-snapshot".to_string(),
        };
        assert_eq!(snapshot.to_string(), "projects/test-project/snapshots/test-snapshot");
    }

    #[test]
    fn test_subscription_id_invalid_character() {
        // Test invalid character error for subscription ID (lines 192-193)
        let result = validate_subscription_id("sub@invalid");

        assert!(result.is_err());
        match result {
            Err(Error::Validation(ValidationError::InvalidSubscriptionId(msg))) => {
                assert!(msg.contains("invalid character"));
            }
            _ => panic!("Expected InvalidSubscriptionId error"),
        }
    }

    #[test]
    fn test_project_id_invalid_character() {
        // Test invalid character error for project ID (lines 229-231)
        let result = validate_project_id("project@invalid");

        assert!(result.is_err());
        match result {
            Err(Error::Validation(ValidationError::InvalidParameter { name, reason })) => {
                assert_eq!(name, "project_id");
                assert!(reason.contains("invalid character"));
            }
            _ => panic!("Expected InvalidParameter error"),
        }
    }
}
