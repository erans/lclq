//! Storage backend traits and implementations.

use crate::types::{Message, QueueConfig, QueueStats, ReceiveOptions, SubscriptionConfig};
use crate::Result;
use async_trait::async_trait;

pub mod memory;

/// Filter for listing queues.
#[derive(Debug, Clone, Default)]
pub struct QueueFilter {
    /// Name prefix filter.
    pub name_prefix: Option<String>,
}

/// A received message with receipt handle.
#[derive(Debug, Clone)]
pub struct ReceivedMessage {
    /// The message.
    pub message: Message,
    /// Receipt handle for acknowledgment.
    pub receipt_handle: String,
}

/// Storage backend trait.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Create a new queue.
    async fn create_queue(&self, config: QueueConfig) -> Result<QueueConfig>;

    /// Get a queue by ID.
    async fn get_queue(&self, id: &str) -> Result<QueueConfig>;

    /// Delete a queue.
    async fn delete_queue(&self, id: &str) -> Result<()>;

    /// List queues with optional filter.
    async fn list_queues(&self, filter: Option<QueueFilter>) -> Result<Vec<QueueConfig>>;

    /// Update queue configuration.
    async fn update_queue(&self, config: QueueConfig) -> Result<QueueConfig>;

    /// Send a message to a queue.
    async fn send_message(&self, queue_id: &str, message: Message) -> Result<Message>;

    /// Send multiple messages to a queue.
    async fn send_messages(&self, queue_id: &str, messages: Vec<Message>)
        -> Result<Vec<Message>>;

    /// Receive messages from a queue.
    async fn receive_messages(
        &self,
        queue_id: &str,
        options: ReceiveOptions,
    ) -> Result<Vec<ReceivedMessage>>;

    /// Delete a message using receipt handle.
    async fn delete_message(&self, queue_id: &str, receipt_handle: &str) -> Result<()>;

    /// Change message visibility timeout.
    async fn change_visibility(
        &self,
        queue_id: &str,
        receipt_handle: &str,
        visibility_timeout: u32,
    ) -> Result<()>;

    /// Purge all messages from a queue.
    async fn purge_queue(&self, queue_id: &str) -> Result<()>;

    /// Get queue statistics.
    async fn get_stats(&self, queue_id: &str) -> Result<QueueStats>;

    /// Create a subscription (for Pub/Sub).
    async fn create_subscription(&self, config: SubscriptionConfig)
        -> Result<SubscriptionConfig>;

    /// Get a subscription by ID.
    async fn get_subscription(&self, id: &str) -> Result<SubscriptionConfig>;

    /// Delete a subscription.
    async fn delete_subscription(&self, id: &str) -> Result<()>;

    /// List subscriptions.
    async fn list_subscriptions(&self) -> Result<Vec<SubscriptionConfig>>;

    /// Health check.
    async fn health_check(&self) -> Result<HealthStatus>;
}

/// Health status.
#[derive(Debug, Clone)]
pub enum HealthStatus {
    /// Backend is healthy.
    Healthy,
    /// Backend is unhealthy.
    Unhealthy(String),
}
