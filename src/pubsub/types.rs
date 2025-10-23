//! Custom types and utilities for Pub/Sub implementation.

use crate::error::{Error, Result, ValidationError};
use std::fmt;

/// Represents a parsed Pub/Sub resource name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceName {
    /// Topic resource: projects/{project}/topics/{topic}
    Topic { project: String, topic: String },
    /// Subscription resource: projects/{project}/subscriptions/{subscription}
    Subscription {
        project: String,
        subscription: String,
    },
    /// Snapshot resource: projects/{project}/snapshots/{snapshot}
    Snapshot { project: String, snapshot: String },
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

    let first_char = topic_id.chars().next().unwrap();
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

    let first_char = subscription_id.chars().next().unwrap();
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

    let first_char = project_id.chars().next().unwrap();
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
}
