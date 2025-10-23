//! In-memory storage backend implementation.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::EvictionPolicy;
use crate::core::receipt::{generate_receipt_handle, parse_receipt_handle};
use crate::storage::{HealthStatus, QueueFilter, ReceivedMessage, StorageBackend};
use crate::types::{
    Message, QueueConfig, QueueStats, QueueType, ReceiveOptions, SubscriptionConfig,
};
use crate::{Error, Result};

/// In-memory storage backend.
#[derive(Clone)]
pub struct InMemoryBackend {
    inner: Arc<InMemoryBackendInner>,
}

struct InMemoryBackendInner {
    /// Queues storage.
    queues: RwLock<HashMap<String, QueueData>>,
    /// Subscriptions storage (for Pub/Sub).
    subscriptions: RwLock<HashMap<String, SubscriptionConfig>>,
    /// Configuration.
    config: InMemoryConfig,
}

/// Configuration for in-memory backend.
#[derive(Debug, Clone)]
pub struct InMemoryConfig {
    /// Maximum number of messages across all queues.
    pub max_messages: usize,
    /// Eviction policy when full.
    pub eviction_policy: EvictionPolicy,
}

impl Default for InMemoryConfig {
    fn default() -> Self {
        Self {
            max_messages: 100_000,
            eviction_policy: EvictionPolicy::Lru,
        }
    }
}

/// Queue data storage.
struct QueueData {
    /// Queue configuration.
    config: QueueConfig,
    /// Messages available for delivery.
    available_messages: VecDeque<StoredMessage>,
    /// Messages currently in flight (invisible).
    in_flight_messages: HashMap<String, InFlightMessage>,
    /// Deduplication cache for FIFO queues (message dedup ID -> timestamp).
    deduplication_cache: HashMap<String, DateTime<Utc>>,
    /// Next sequence number for FIFO queues.
    next_sequence_number: u64,
    /// Last access time (for LRU eviction).
    last_access: DateTime<Utc>,
}

/// Stored message with metadata.
#[derive(Clone)]
struct StoredMessage {
    /// The message.
    message: Message,
    /// When this message becomes visible.
    visible_at: DateTime<Utc>,
}

/// In-flight message with receipt handle.
struct InFlightMessage {
    /// The message.
    message: Message,
    /// When visibility expires.
    visibility_expires_at: DateTime<Utc>,
}

impl InMemoryBackend {
    /// Create a new in-memory backend with default configuration.
    pub fn new() -> Self {
        Self::with_config(InMemoryConfig::default())
    }

    /// Create a new in-memory backend with custom configuration.
    pub fn with_config(config: InMemoryConfig) -> Self {
        info!(
            "Initializing in-memory backend with max_messages={}, policy={:?}",
            config.max_messages, config.eviction_policy
        );

        Self {
            inner: Arc::new(InMemoryBackendInner {
                queues: RwLock::new(HashMap::new()),
                subscriptions: RwLock::new(HashMap::new()),
                config,
            }),
        }
    }

    /// Get total message count across all queues.
    async fn total_message_count(&self) -> usize {
        let queues = self.inner.queues.read().await;
        queues
            .values()
            .map(|q| q.available_messages.len() + q.in_flight_messages.len())
            .sum()
    }

    /// Check if we need to evict messages.
    async fn maybe_evict(&self) -> Result<()> {
        let total = self.total_message_count().await;
        if total < self.inner.config.max_messages {
            return Ok(());
        }

        match self.inner.config.eviction_policy {
            EvictionPolicy::RejectNew => {
                return Err(Error::StorageError(
                    "Message storage is full, rejecting new messages".to_string(),
                ));
            }
            EvictionPolicy::Lru => {
                self.evict_lru().await?;
            }
            EvictionPolicy::Fifo => {
                self.evict_fifo().await?;
            }
        }

        Ok(())
    }

    /// Evict least recently used message.
    async fn evict_lru(&self) -> Result<()> {
        let mut queues = self.inner.queues.write().await;

        // Find queue with oldest last_access time that has messages
        let oldest_queue_id = queues
            .iter()
            .filter(|(_, q)| !q.available_messages.is_empty())
            .min_by_key(|(_, q)| q.last_access)
            .map(|(id, _)| id.clone());

        if let Some(queue_id) = oldest_queue_id {
            if let Some(queue) = queues.get_mut(&queue_id) {
                if let Some(evicted) = queue.available_messages.pop_front() {
                    warn!(
                        queue_id = %queue_id,
                        message_id = %evicted.message.id,
                        "Evicted message due to LRU policy"
                    );
                }
            }
        }

        Ok(())
    }

    /// Evict oldest message (FIFO eviction across all queues).
    async fn evict_fifo(&self) -> Result<()> {
        let mut queues = self.inner.queues.write().await;

        // Find the oldest message across all queues
        let oldest = queues
            .iter()
            .filter_map(|(id, q)| {
                q.available_messages
                    .front()
                    .map(|m| (id.clone(), m.message.sent_timestamp))
            })
            .min_by_key(|(_, timestamp)| *timestamp);

        if let Some((queue_id, _)) = oldest {
            if let Some(queue) = queues.get_mut(&queue_id) {
                if let Some(evicted) = queue.available_messages.pop_front() {
                    warn!(
                        queue_id = %queue_id,
                        message_id = %evicted.message.id,
                        "Evicted message due to FIFO policy"
                    );
                }
            }
        }

        Ok(())
    }

    /// Clean up expired deduplication cache entries (older than 5 minutes).
    #[allow(dead_code)]
    async fn cleanup_deduplication_cache(&self, queue_id: &str) -> Result<()> {
        let mut queues = self.inner.queues.write().await;
        let cutoff = Utc::now() - Duration::minutes(5);

        if let Some(queue) = queues.get_mut(queue_id) {
            queue
                .deduplication_cache
                .retain(|_, timestamp| *timestamp > cutoff);
        }

        Ok(())
    }

    /// Generate content-based deduplication ID.
    fn generate_content_dedup_id(message: &Message) -> String {
        let mut hasher = Sha256::new();
        hasher.update(message.body.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StorageBackend for InMemoryBackend {
    async fn create_queue(&self, config: QueueConfig) -> Result<QueueConfig> {
        debug!(queue_name = %config.name, queue_type = ?config.queue_type, "Creating queue");

        let mut queues = self.inner.queues.write().await;

        if queues.contains_key(&config.id) {
            return Err(Error::StorageError(format!("Queue {} already exists", config.id)));
        }

        let queue_data = QueueData {
            config: config.clone(),
            available_messages: VecDeque::new(),
            in_flight_messages: HashMap::new(),
            deduplication_cache: HashMap::new(),
            next_sequence_number: 0,
            last_access: Utc::now(),
        };

        queues.insert(config.id.clone(), queue_data);
        info!(queue_id = %config.id, queue_name = %config.name, "Queue created");

        Ok(config)
    }

    async fn get_queue(&self, id: &str) -> Result<QueueConfig> {
        let queues = self.inner.queues.read().await;
        queues
            .get(id)
            .map(|q| q.config.clone())
            .ok_or_else(|| Error::QueueNotFound(id.to_string()))
    }

    async fn delete_queue(&self, id: &str) -> Result<()> {
        debug!(queue_id = %id, "Deleting queue");

        let mut queues = self.inner.queues.write().await;
        queues
            .remove(id)
            .ok_or_else(|| Error::QueueNotFound(id.to_string()))?;

        info!(queue_id = %id, "Queue deleted");
        Ok(())
    }

    async fn list_queues(&self, filter: Option<QueueFilter>) -> Result<Vec<QueueConfig>> {
        let queues = self.inner.queues.read().await;

        let mut result: Vec<QueueConfig> = queues
            .values()
            .filter(|q| {
                if let Some(ref f) = filter {
                    if let Some(ref prefix) = f.name_prefix {
                        return q.config.name.starts_with(prefix);
                    }
                }
                true
            })
            .map(|q| q.config.clone())
            .collect();

        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    async fn update_queue(&self, config: QueueConfig) -> Result<QueueConfig> {
        debug!(queue_id = %config.id, "Updating queue configuration");

        let mut queues = self.inner.queues.write().await;
        let queue = queues
            .get_mut(&config.id)
            .ok_or_else(|| Error::QueueNotFound(config.id.clone()))?;

        queue.config = config.clone();
        info!(queue_id = %config.id, "Queue configuration updated");

        Ok(config)
    }

    async fn send_message(&self, queue_id: &str, mut message: Message) -> Result<Message> {
        self.maybe_evict().await?;

        let mut queues = self.inner.queues.write().await;
        let queue = queues
            .get_mut(queue_id)
            .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        queue.last_access = Utc::now();

        // Handle FIFO deduplication
        if queue.config.queue_type == QueueType::SqsFifo {
            let dedup_id = if let Some(ref id) = message.deduplication_id {
                id.clone()
            } else if queue.config.content_based_deduplication {
                Self::generate_content_dedup_id(&message)
            } else {
                return Err(Error::Validation(
                    crate::error::ValidationError::InvalidParameter {
                        name: "MessageDeduplicationId".to_string(),
                        reason: "Required for FIFO queues without content-based deduplication"
                            .to_string(),
                    },
                ));
            };

            // Check deduplication cache
            if let Some(&cached_time) = queue.deduplication_cache.get(&dedup_id) {
                let age = Utc::now() - cached_time;
                if age < Duration::minutes(5) {
                    debug!(
                        queue_id = %queue_id,
                        dedup_id = %dedup_id,
                        "Message deduplicated (already sent within 5 minutes)"
                    );
                    return Ok(message); // Return the message but don't add it
                }
            }

            // Add to deduplication cache
            queue.deduplication_cache.insert(dedup_id, Utc::now());

            // Assign sequence number
            message.sequence_number = Some(queue.next_sequence_number);
            queue.next_sequence_number += 1;
        }

        // Calculate when message becomes visible
        let delay = message.delay_seconds.unwrap_or(queue.config.delay_seconds);
        let visible_at = Utc::now() + Duration::seconds(delay as i64);

        let stored = StoredMessage {
            message: message.clone(),
            visible_at,
        };

        queue.available_messages.push_back(stored);

        debug!(
            queue_id = %queue_id,
            message_id = %message.id,
            visible_in_seconds = delay,
            "Message sent"
        );

        Ok(message)
    }

    async fn send_messages(&self, queue_id: &str, messages: Vec<Message>) -> Result<Vec<Message>> {
        let mut results = Vec::new();
        for message in messages {
            results.push(self.send_message(queue_id, message).await?);
        }
        Ok(results)
    }

    async fn receive_messages(
        &self,
        queue_id: &str,
        options: ReceiveOptions,
    ) -> Result<Vec<ReceivedMessage>> {
        let mut queues = self.inner.queues.write().await;
        let queue = queues
            .get_mut(queue_id)
            .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        queue.last_access = Utc::now();

        let now = Utc::now();
        let visibility_timeout = options
            .visibility_timeout
            .unwrap_or(queue.config.visibility_timeout);

        let mut received = Vec::new();
        let max_messages = options.max_messages.min(10) as usize;

        // For FIFO queues, we need to respect message group ordering
        if queue.config.queue_type == QueueType::SqsFifo {
            // Track which message groups are already in flight
            let in_flight_groups: std::collections::HashSet<_> = queue
                .in_flight_messages
                .values()
                .filter_map(|m| m.message.message_group_id.as_ref())
                .cloned()
                .collect();

            let mut i = 0;
            while i < queue.available_messages.len() && received.len() < max_messages {
                let stored = &queue.available_messages[i];

                // Check if message is visible
                if stored.visible_at > now {
                    i += 1;
                    continue;
                }

                // For FIFO, skip if this message group is already in flight
                if let Some(ref group_id) = stored.message.message_group_id {
                    if in_flight_groups.contains(group_id) {
                        i += 1;
                        continue;
                    }
                }

                // Remove from available messages
                let stored = queue.available_messages.remove(i).unwrap();
                let mut message = stored.message;
                message.receive_count += 1;

                // Generate receipt handle
                let receipt_handle = generate_receipt_handle(queue_id, &message.id);

                // Add to in-flight
                let visibility_expires_at = now + Duration::seconds(visibility_timeout as i64);
                queue.in_flight_messages.insert(
                    message.id.0.clone(),
                    InFlightMessage {
                        message: message.clone(),
                        visibility_expires_at,
                    },
                );

                received.push(ReceivedMessage {
                    message,
                    receipt_handle,
                });
            }
        } else {
            // Standard queue - simpler logic
            let mut i = 0;
            while i < queue.available_messages.len() && received.len() < max_messages {
                let stored = &queue.available_messages[i];

                if stored.visible_at > now {
                    i += 1;
                    continue;
                }

                let stored = queue.available_messages.remove(i).unwrap();
                let mut message = stored.message;
                message.receive_count += 1;

                let receipt_handle = generate_receipt_handle(queue_id, &message.id);
                let visibility_expires_at = now + Duration::seconds(visibility_timeout as i64);

                queue.in_flight_messages.insert(
                    message.id.0.clone(),
                    InFlightMessage {
                        message: message.clone(),
                        visibility_expires_at,
                    },
                );

                received.push(ReceivedMessage {
                    message,
                    receipt_handle,
                });
            }
        }

        debug!(
            queue_id = %queue_id,
            received_count = received.len(),
            "Messages received"
        );

        Ok(received)
    }

    async fn delete_message(&self, queue_id: &str, receipt_handle: &str) -> Result<()> {
        let handle_data = parse_receipt_handle(receipt_handle)?;

        if handle_data.queue_id != queue_id {
            return Err(Error::InvalidReceiptHandle);
        }

        let mut queues = self.inner.queues.write().await;
        let queue = queues
            .get_mut(queue_id)
            .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        queue
            .in_flight_messages
            .remove(&handle_data.message_id.0)
            .ok_or(Error::MessageNotFound(handle_data.message_id.0.clone()))?;

        debug!(
            queue_id = %queue_id,
            message_id = %handle_data.message_id,
            "Message deleted"
        );

        Ok(())
    }

    async fn change_visibility(
        &self,
        queue_id: &str,
        receipt_handle: &str,
        visibility_timeout: u32,
    ) -> Result<()> {
        let handle_data = parse_receipt_handle(receipt_handle)?;

        if handle_data.queue_id != queue_id {
            return Err(Error::InvalidReceiptHandle);
        }

        let mut queues = self.inner.queues.write().await;
        let queue = queues
            .get_mut(queue_id)
            .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        // If visibility timeout is 0, immediately return message to available queue
        if visibility_timeout == 0 {
            let in_flight_msg = queue
                .in_flight_messages
                .remove(&handle_data.message_id.0)
                .ok_or(Error::MessageNotFound(handle_data.message_id.0.clone()))?;

            // Put message back into available queue with immediate visibility
            queue.available_messages.push_back(StoredMessage {
                message: in_flight_msg.message,
                visible_at: Utc::now(), // Immediately visible
            });

            debug!(
                queue_id = %queue_id,
                message_id = %handle_data.message_id,
                "Message returned to queue (visibility timeout set to 0)"
            );
        } else {
            // Update visibility timeout for in-flight message
            let in_flight = queue
                .in_flight_messages
                .get_mut(&handle_data.message_id.0)
                .ok_or(Error::MessageNotFound(handle_data.message_id.0.clone()))?;

            in_flight.visibility_expires_at =
                Utc::now() + Duration::seconds(visibility_timeout as i64);

            debug!(
                queue_id = %queue_id,
                message_id = %handle_data.message_id,
                visibility_timeout = visibility_timeout,
                "Visibility timeout changed"
            );
        }

        Ok(())
    }

    async fn purge_queue(&self, queue_id: &str) -> Result<()> {
        debug!(queue_id = %queue_id, "Purging queue");

        let mut queues = self.inner.queues.write().await;
        let queue = queues
            .get_mut(queue_id)
            .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        let message_count =
            queue.available_messages.len() + queue.in_flight_messages.len();

        queue.available_messages.clear();
        queue.in_flight_messages.clear();

        info!(queue_id = %queue_id, messages_purged = message_count, "Queue purged");
        Ok(())
    }

    async fn get_stats(&self, queue_id: &str) -> Result<QueueStats> {
        let queues = self.inner.queues.read().await;
        let queue = queues
            .get(queue_id)
            .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        let oldest_message_timestamp = queue
            .available_messages
            .front()
            .map(|m| m.message.sent_timestamp);

        Ok(QueueStats {
            available_messages: queue.available_messages.len() as u64,
            in_flight_messages: queue.in_flight_messages.len() as u64,
            dlq_messages: 0, // TODO: Track DLQ messages
            oldest_message_timestamp,
        })
    }

    async fn create_subscription(
        &self,
        config: SubscriptionConfig,
    ) -> Result<SubscriptionConfig> {
        debug!(subscription_id = %config.id, topic_id = %config.topic_id, "Creating subscription");

        let mut subscriptions = self.inner.subscriptions.write().await;

        if subscriptions.contains_key(&config.id) {
            return Err(Error::StorageError(format!(
                "Subscription {} already exists",
                config.id
            )));
        }

        subscriptions.insert(config.id.clone(), config.clone());
        info!(subscription_id = %config.id, "Subscription created");

        Ok(config)
    }

    async fn get_subscription(&self, id: &str) -> Result<SubscriptionConfig> {
        let subscriptions = self.inner.subscriptions.read().await;
        subscriptions
            .get(id)
            .cloned()
            .ok_or_else(|| Error::SubscriptionNotFound(id.to_string()))
    }

    async fn delete_subscription(&self, id: &str) -> Result<()> {
        debug!(subscription_id = %id, "Deleting subscription");

        let mut subscriptions = self.inner.subscriptions.write().await;
        subscriptions
            .remove(id)
            .ok_or_else(|| Error::SubscriptionNotFound(id.to_string()))?;

        info!(subscription_id = %id, "Subscription deleted");
        Ok(())
    }

    async fn list_subscriptions(&self) -> Result<Vec<SubscriptionConfig>> {
        let subscriptions = self.inner.subscriptions.read().await;
        Ok(subscriptions.values().cloned().collect())
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::Healthy)
    }
}
