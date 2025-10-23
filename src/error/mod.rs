//! Error types for lclq.

use thiserror::Error;

/// Result type for lclq operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for lclq.
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation error.
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// Storage backend error.
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Queue already exists.
    #[error("Queue already exists: {0}")]
    QueueAlreadyExists(String),

    /// Feature not yet implemented.
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Queue not found.
    #[error("Queue not found: {0}")]
    QueueNotFound(String),

    /// Topic not found.
    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    /// Subscription not found.
    #[error("Subscription not found: {0}")]
    SubscriptionNotFound(String),

    /// Message not found.
    #[error("Message not found: {0}")]
    MessageNotFound(String),

    /// Invalid receipt handle.
    #[error("Invalid receipt handle")]
    InvalidReceiptHandle,

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Validation error types.
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Invalid queue name.
    #[error("Invalid queue name: {0}")]
    InvalidQueueName(String),

    /// Invalid topic ID.
    #[error("Invalid topic ID: {0}")]
    InvalidTopicId(String),

    /// Invalid subscription ID.
    #[error("Invalid subscription ID: {0}")]
    InvalidSubscriptionId(String),

    /// Message too large.
    #[error("Message too large: {size} bytes (max: {max} bytes)")]
    MessageTooLarge {
        /// Actual message size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Invalid attribute.
    #[error("Invalid attribute: {0}")]
    InvalidAttribute(String),

    /// Invalid parameter.
    #[error("Invalid parameter {name}: {reason}")]
    InvalidParameter {
        /// Parameter name.
        name: String,
        /// Reason for invalidity.
        reason: String,
    },
}
