# Pub/Sub Push Subscriptions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add HTTP push delivery mode to Pub/Sub subscriptions with configurable retry policies and dead letter topic support.

**Architecture:** Background worker pool pulls from priority delivery queue, executes HTTP POST to registered endpoints with exponential backoff retry, moves failed messages to DLT after max attempts.

**Tech Stack:** Rust, Tokio (async runtime), reqwest (HTTP client), tokio::sync (channels/primitives), serde (JSON)

---

## Task 1: Add Push Configuration Types

**Files:**
- Modify: `src/types/mod.rs:187-214`
- Test: `src/types/mod.rs:274` (after existing tests)

**Step 1: Write the failing test**

Add at end of tests module in `src/types/mod.rs`:

```rust
#[test]
fn test_push_config_validation() {
    let push_config = PushConfig {
        endpoint: "https://example.com/webhook".to_string(),
        retry_policy: Some(RetryPolicy::default()),
        timeout_seconds: Some(30),
    };

    assert_eq!(push_config.endpoint, "https://example.com/webhook");
    assert!(push_config.retry_policy.is_some());
    assert_eq!(push_config.timeout_seconds, Some(30));
}

#[test]
fn test_retry_policy_defaults() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.min_backoff_seconds, 10);
    assert_eq!(policy.max_backoff_seconds, 600);
    assert_eq!(policy.max_attempts, 5);
}

#[test]
fn test_subscription_config_with_push() {
    let config = SubscriptionConfig {
        id: "test-sub".to_string(),
        name: "test-sub".to_string(),
        topic_id: "test-topic".to_string(),
        ack_deadline_seconds: 30,
        message_retention_duration: 604800,
        enable_message_ordering: false,
        filter: None,
        dead_letter_policy: None,
        push_config: Some(PushConfig {
            endpoint: "https://example.com/hook".to_string(),
            retry_policy: None,
            timeout_seconds: None,
        }),
    };

    assert!(config.push_config.is_some());
    assert_eq!(config.push_config.unwrap().endpoint, "https://example.com/hook");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_push_config_validation --lib`
Expected: FAIL with "cannot find type `PushConfig`"

**Step 3: Write minimal implementation**

Add to `src/types/mod.rs` after `DeadLetterPolicy` (line 214):

```rust
/// Push configuration for push subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// Push endpoint URL (HTTP or HTTPS).
    pub endpoint: String,
    /// Retry policy for failed deliveries.
    pub retry_policy: Option<RetryPolicy>,
    /// HTTP request timeout in seconds.
    pub timeout_seconds: Option<u32>,
}

/// Retry policy for push deliveries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Minimum backoff delay in seconds (default: 10).
    pub min_backoff_seconds: u32,
    /// Maximum backoff delay in seconds (default: 600).
    pub max_backoff_seconds: u32,
    /// Maximum delivery attempts (default: 5).
    pub max_attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            min_backoff_seconds: 10,
            max_backoff_seconds: 600,
            max_attempts: 5,
        }
    }
}
```

**Step 4: Update SubscriptionConfig**

Modify `SubscriptionConfig` struct (line 187-205) to add push_config field:

```rust
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
    /// Push configuration (None = pull subscription).
    pub push_config: Option<PushConfig>,
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test test_push_config_validation test_retry_policy_defaults test_subscription_config_with_push --lib`
Expected: PASS (3 tests)

**Step 6: Commit**

```bash
git add src/types/mod.rs
git commit -m "feat(types): add PushConfig and RetryPolicy types for push subscriptions"
```

---

## Task 2: Create Push Delivery Queue

**Files:**
- Create: `src/pubsub/push_queue.rs`
- Modify: `src/pubsub/mod.rs:26` (add module)

**Step 1: Write the failing test**

Create `src/pubsub/push_queue.rs`:

```rust
//! Push delivery queue with priority scheduling.

use crate::types::{Message, PushConfig, RetryPolicy, SubscriptionConfig};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::sync::Notify;

/// A task for delivering a message to a push endpoint.
#[derive(Debug, Clone)]
pub struct DeliveryTask {
    /// The message to deliver.
    pub message: Message,
    /// The subscription configuration.
    pub subscription: Arc<SubscriptionConfig>,
    /// Current attempt number (0-indexed).
    pub attempt: u32,
    /// When this task should be delivered.
    pub scheduled_time: Instant,
}

impl PartialEq for DeliveryTask {
    fn eq(&self, other: &Self) -> bool {
        self.scheduled_time == other.scheduled_time
    }
}

impl Eq for DeliveryTask {}

impl PartialOrd for DeliveryTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DeliveryTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order: earlier scheduled_time = higher priority
        other.scheduled_time.cmp(&self.scheduled_time)
    }
}

/// Push delivery queue with priority scheduling.
#[derive(Clone)]
pub struct DeliveryQueue {
    queue: Arc<Mutex<BinaryHeap<DeliveryTask>>>,
    notify: Arc<Notify>,
}

impl DeliveryQueue {
    /// Create a new empty delivery queue.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Enqueue a new delivery task.
    pub async fn enqueue(&self, task: DeliveryTask) {
        let mut queue = self.queue.lock().await;
        queue.push(task);
        drop(queue);
        self.notify.notify_one();
    }

    /// Dequeue the next ready task (scheduled_time <= now).
    /// Returns None if no tasks are ready.
    pub async fn dequeue_ready(&self) -> Option<DeliveryTask> {
        let mut queue = self.queue.lock().await;

        if let Some(task) = queue.peek() {
            if task.scheduled_time <= Instant::now() {
                return queue.pop();
            }
        }

        None
    }

    /// Wait for a task to become ready and dequeue it.
    pub async fn wait_and_dequeue(&self) -> Option<DeliveryTask> {
        loop {
            // Check if any task is ready
            if let Some(task) = self.dequeue_ready().await {
                return Some(task);
            }

            // Calculate wait duration for next task
            let wait_duration = {
                let queue = self.queue.lock().await;
                if let Some(task) = queue.peek() {
                    let now = Instant::now();
                    if task.scheduled_time > now {
                        Some(task.scheduled_time - now)
                    } else {
                        Some(Duration::from_millis(1))
                    }
                } else {
                    None
                }
            };

            if let Some(duration) = wait_duration {
                tokio::select! {
                    _ = tokio::time::sleep(duration) => {},
                    _ = self.notify.notified() => {},
                }
            } else {
                // Queue is empty, wait for notification
                self.notify.notified().await;
            }
        }
    }

    /// Get the current queue size.
    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Check if the queue is empty.
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }
}

impl Default for DeliveryQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MessageId, QueueType};
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_test_subscription() -> Arc<SubscriptionConfig> {
        Arc::new(SubscriptionConfig {
            id: "test-sub".to_string(),
            name: "test-sub".to_string(),
            topic_id: "test-topic".to_string(),
            ack_deadline_seconds: 30,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: Some(PushConfig {
                endpoint: "https://example.com/webhook".to_string(),
                retry_policy: Some(RetryPolicy::default()),
                timeout_seconds: Some(30),
            }),
        })
    }

    fn create_test_message(id: &str) -> Message {
        Message {
            id: MessageId::from_string(id.to_string()),
            body: "test message".to_string(),
            attributes: HashMap::new(),
            queue_id: "test-topic".to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        }
    }

    #[tokio::test]
    async fn test_delivery_queue_enqueue_dequeue() {
        let queue = DeliveryQueue::new();
        let subscription = create_test_subscription();
        let message = create_test_message("msg-1");

        let task = DeliveryTask {
            message,
            subscription,
            attempt: 0,
            scheduled_time: Instant::now(),
        };

        queue.enqueue(task).await;
        assert_eq!(queue.len().await, 1);

        let dequeued = queue.dequeue_ready().await;
        assert!(dequeued.is_some());
        assert_eq!(queue.len().await, 0);
    }

    #[tokio::test]
    async fn test_delivery_queue_priority_ordering() {
        let queue = DeliveryQueue::new();
        let subscription = create_test_subscription();

        // Enqueue tasks with different scheduled times
        let now = Instant::now();
        let task1 = DeliveryTask {
            message: create_test_message("msg-1"),
            subscription: subscription.clone(),
            attempt: 0,
            scheduled_time: now + Duration::from_secs(10),
        };
        let task2 = DeliveryTask {
            message: create_test_message("msg-2"),
            subscription: subscription.clone(),
            attempt: 0,
            scheduled_time: now + Duration::from_secs(5),
        };
        let task3 = DeliveryTask {
            message: create_test_message("msg-3"),
            subscription: subscription.clone(),
            attempt: 0,
            scheduled_time: now, // Ready immediately
        };

        queue.enqueue(task1).await;
        queue.enqueue(task2).await;
        queue.enqueue(task3).await;

        // Should dequeue task3 first (scheduled now)
        let dequeued = queue.dequeue_ready().await;
        assert!(dequeued.is_some());
        assert_eq!(dequeued.unwrap().message.id.0, "msg-3");
    }

    #[tokio::test]
    async fn test_delivery_queue_dequeue_not_ready() {
        let queue = DeliveryQueue::new();
        let subscription = create_test_subscription();

        // Enqueue task scheduled for future
        let task = DeliveryTask {
            message: create_test_message("msg-1"),
            subscription,
            attempt: 0,
            scheduled_time: Instant::now() + Duration::from_secs(60),
        };

        queue.enqueue(task).await;

        // Should return None because task is not ready
        let dequeued = queue.dequeue_ready().await;
        assert!(dequeued.is_none());
        assert_eq!(queue.len().await, 1);
    }

    #[tokio::test]
    async fn test_delivery_queue_wait_and_dequeue() {
        let queue = DeliveryQueue::new();
        let subscription = create_test_subscription();

        // Enqueue task scheduled slightly in future
        let task = DeliveryTask {
            message: create_test_message("msg-1"),
            subscription,
            attempt: 0,
            scheduled_time: Instant::now() + Duration::from_millis(50),
        };

        queue.enqueue(task).await;

        // Should wait and then dequeue
        let start = Instant::now();
        let dequeued = queue.wait_and_dequeue().await;
        let elapsed = start.elapsed();

        assert!(dequeued.is_some());
        assert!(elapsed >= Duration::from_millis(50));
        assert!(elapsed < Duration::from_millis(200)); // Reasonable upper bound
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib push_queue`
Expected: FAIL with "module not found"

**Step 3: Add module declaration**

Modify `src/pubsub/mod.rs` (line 26) to add:

```rust
pub mod push_queue;
```

**Step 4: Run test to verify it passes**

Run: `cargo test --lib push_queue`
Expected: PASS (4 tests)

**Step 5: Commit**

```bash
git add src/pubsub/push_queue.rs src/pubsub/mod.rs
git commit -m "feat(pubsub): add push delivery queue with priority scheduling"
```

---

## Task 3: Create Worker Pool

**Files:**
- Create: `src/pubsub/push_worker.rs`
- Modify: `src/pubsub/mod.rs:27` (add module)
- Modify: `Cargo.toml:73` (add num_cpus dependency)

**Step 1: Add num_cpus dependency**

Modify `Cargo.toml` to add after line 73 (after reqwest):

```toml
num_cpus = "1.16"
```

**Step 2: Write the failing test**

Create `src/pubsub/push_worker.rs`:

```rust
//! Push worker pool for HTTP delivery.

use crate::error::{Error, Result};
use crate::pubsub::push_queue::{DeliveryQueue, DeliveryTask};
use crate::storage::StorageBackend;
use crate::types::{Message, MessageAttributeValue, MessageAttributes, MessageId, PushConfig, RetryPolicy, SubscriptionConfig};
use base64::Engine;
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

const DEFAULT_TIMEOUT_SECONDS: u32 = 30;

/// Push worker pool for delivering messages to HTTP endpoints.
pub struct PushWorkerPool {
    workers: Vec<JoinHandle<()>>,
    shutdown_signal: Arc<AtomicBool>,
    delivery_queue: DeliveryQueue,
}

impl PushWorkerPool {
    /// Create a new worker pool.
    ///
    /// # Arguments
    /// * `num_workers` - Number of worker threads (default: num_cpus * 2)
    /// * `delivery_queue` - Shared delivery queue
    /// * `backend` - Storage backend for DLT publishing
    pub fn new(
        num_workers: Option<usize>,
        delivery_queue: DeliveryQueue,
        backend: Arc<dyn StorageBackend>,
    ) -> Self {
        let num_workers = num_workers.unwrap_or_else(|| num_cpus::get() * 2);
        let shutdown_signal = Arc::new(AtomicBool::new(false));

        let workers = (0..num_workers)
            .map(|id| {
                spawn_worker(
                    id,
                    delivery_queue.clone(),
                    backend.clone(),
                    shutdown_signal.clone(),
                )
            })
            .collect();

        info!("Started push worker pool with {} workers", num_workers);

        Self {
            workers,
            shutdown_signal,
            delivery_queue,
        }
    }

    /// Get a reference to the delivery queue.
    pub fn queue(&self) -> DeliveryQueue {
        self.delivery_queue.clone()
    }

    /// Shutdown the worker pool gracefully.
    pub async fn shutdown(self) {
        info!("Shutting down push worker pool");
        self.shutdown_signal.store(true, Ordering::SeqCst);

        for worker in self.workers {
            let _ = worker.await;
        }

        info!("Push worker pool shutdown complete");
    }
}

/// Spawn a single worker.
fn spawn_worker(
    id: usize,
    queue: DeliveryQueue,
    backend: Arc<dyn StorageBackend>,
    shutdown_signal: Arc<AtomicBool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS as u64))
            .build()
            .expect("Failed to create HTTP client");

        debug!("Push worker {} started", id);

        loop {
            // Check shutdown signal
            if shutdown_signal.load(Ordering::SeqCst) {
                debug!("Push worker {} received shutdown signal", id);
                break;
            }

            // Wait for next task
            let task = tokio::select! {
                task = queue.wait_and_dequeue() => task,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // Timeout to check shutdown signal periodically
                    continue;
                }
            };

            if let Some(task) = task {
                handle_delivery_task(task, &client, &queue, backend.as_ref()).await;
            }
        }

        debug!("Push worker {} stopped", id);
    })
}

/// Handle a single delivery task.
async fn handle_delivery_task(
    task: DeliveryTask,
    client: &Client,
    queue: &DeliveryQueue,
    backend: &dyn StorageBackend,
) {
    let subscription = &task.subscription;
    let push_config = match &subscription.push_config {
        Some(config) => config,
        None => {
            error!("Task has no push config, skipping");
            return;
        }
    };

    debug!(
        "Worker delivering message {} to {} (attempt {})",
        task.message.id, push_config.endpoint, task.attempt
    );

    // Execute HTTP delivery
    let result = deliver_message(
        client,
        &push_config.endpoint,
        &task.message,
        &subscription.name,
        push_config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS),
    )
    .await;

    match result {
        Ok(_) => {
            info!(
                "Successfully delivered message {} to {}",
                task.message.id, push_config.endpoint
            );
        }
        Err(e) => {
            warn!(
                "Failed to deliver message {} to {}: {}",
                task.message.id, push_config.endpoint, e
            );

            // Calculate retry
            let retry_policy = push_config
                .retry_policy
                .as_ref()
                .unwrap_or(&RetryPolicy::default());

            if let Some(retry_task) = calculate_retry(task, retry_policy) {
                // Re-enqueue for retry
                queue.enqueue(retry_task).await;
            } else {
                // Max attempts exceeded, send to DLT
                handle_max_retries(task, subscription, backend).await;
            }
        }
    }
}

/// Deliver a message via HTTP POST.
async fn deliver_message(
    client: &Client,
    endpoint: &str,
    message: &Message,
    subscription_name: &str,
    timeout_seconds: u32,
) -> Result<()> {
    // Build JSON payload matching GCP Pub/Sub format
    let payload = json!({
        "message": {
            "data": base64::engine::general_purpose::STANDARD.encode(message.body.as_bytes()),
            "attributes": message.attributes.iter().map(|(k, v)| {
                (k.clone(), v.string_value.clone().unwrap_or_default())
            }).collect::<std::collections::HashMap<String, String>>(),
            "messageId": message.id.to_string(),
            "publishTime": message.sent_timestamp.to_rfc3339(),
        },
        "subscription": subscription_name,
    });

    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            format!("lclq-push/{}", env!("CARGO_PKG_VERSION")),
        )
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .json(&payload)
        .send()
        .await
        .map_err(|e| Error::Internal(format!("HTTP request failed: {}", e)))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(Error::Internal(format!(
            "HTTP request failed with status: {}",
            response.status()
        )))
    }
}

/// Calculate next retry attempt with exponential backoff.
fn calculate_retry(task: DeliveryTask, policy: &RetryPolicy) -> Option<DeliveryTask> {
    let next_attempt = task.attempt + 1;

    if next_attempt >= policy.max_attempts {
        return None;
    }

    // Exponential backoff: min(min * 2^attempt, max)
    let backoff_secs = policy
        .min_backoff_seconds
        .saturating_mul(2_u32.saturating_pow(task.attempt))
        .min(policy.max_backoff_seconds);

    Some(DeliveryTask {
        message: task.message,
        subscription: task.subscription,
        attempt: next_attempt,
        scheduled_time: Instant::now() + Duration::from_secs(backoff_secs as u64),
    })
}

/// Handle max retries exceeded - publish to DLT if configured.
async fn handle_max_retries(
    task: DeliveryTask,
    subscription: &SubscriptionConfig,
    backend: &dyn StorageBackend,
) {
    if let Some(dlp) = &subscription.dead_letter_policy {
        warn!(
            "Max retries exceeded for message {}, sending to DLT: {}",
            task.message.id, dlp.dead_letter_topic
        );

        // Build message for DLT with metadata
        let mut attributes = task.message.attributes.clone();
        attributes.insert(
            "original_subscription".to_string(),
            MessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some(subscription.name.clone()),
                binary_value: None,
            },
        );
        attributes.insert(
            "failure_reason".to_string(),
            MessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some("max_push_attempts_exceeded".to_string()),
                binary_value: None,
            },
        );
        attributes.insert(
            "attempts".to_string(),
            MessageAttributeValue {
                data_type: "String".to_string(),
                string_value: Some(task.attempt.to_string()),
                binary_value: None,
            },
        );

        let dlq_message = Message {
            id: MessageId::new(),
            body: task.message.body,
            attributes,
            queue_id: dlp.dead_letter_topic.clone(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        };

        if let Err(e) = backend.send_message(&dlp.dead_letter_topic, dlq_message).await {
            error!("Failed to send message to DLT: {}", e);
        }
    } else {
        error!(
            "Max retries exceeded for message {}, no DLT configured - dropping message",
            task.message.id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_policy() -> RetryPolicy {
        RetryPolicy {
            min_backoff_seconds: 10,
            max_backoff_seconds: 600,
            max_attempts: 5,
        }
    }

    #[test]
    fn test_calculate_retry_exponential_backoff() {
        let policy = create_test_policy();
        let subscription = Arc::new(SubscriptionConfig {
            id: "test-sub".to_string(),
            name: "test-sub".to_string(),
            topic_id: "test-topic".to_string(),
            ack_deadline_seconds: 30,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: Some(PushConfig {
                endpoint: "https://example.com/webhook".to_string(),
                retry_policy: Some(policy.clone()),
                timeout_seconds: Some(30),
            }),
        });

        let message = Message {
            id: MessageId::from_string("test-msg".to_string()),
            body: "test".to_string(),
            attributes: MessageAttributes::new(),
            queue_id: "test-topic".to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        };

        // Attempt 0 -> 1: 10 * 2^0 = 10 seconds
        let task = DeliveryTask {
            message: message.clone(),
            subscription: subscription.clone(),
            attempt: 0,
            scheduled_time: Instant::now(),
        };
        let retry = calculate_retry(task, &policy).unwrap();
        assert_eq!(retry.attempt, 1);

        // Attempt 1 -> 2: 10 * 2^1 = 20 seconds
        let task = DeliveryTask {
            message: message.clone(),
            subscription: subscription.clone(),
            attempt: 1,
            scheduled_time: Instant::now(),
        };
        let retry = calculate_retry(task, &policy).unwrap();
        assert_eq!(retry.attempt, 2);

        // Attempt 4 -> 5: max attempts reached
        let task = DeliveryTask {
            message: message.clone(),
            subscription: subscription.clone(),
            attempt: 4,
            scheduled_time: Instant::now(),
        };
        let retry = calculate_retry(task, &policy);
        assert!(retry.is_none());
    }

    #[test]
    fn test_calculate_retry_max_backoff() {
        let policy = RetryPolicy {
            min_backoff_seconds: 10,
            max_backoff_seconds: 100,
            max_attempts: 10,
        };
        let subscription = Arc::new(SubscriptionConfig {
            id: "test-sub".to_string(),
            name: "test-sub".to_string(),
            topic_id: "test-topic".to_string(),
            ack_deadline_seconds: 30,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: Some(PushConfig {
                endpoint: "https://example.com/webhook".to_string(),
                retry_policy: Some(policy.clone()),
                timeout_seconds: Some(30),
            }),
        });

        let message = Message {
            id: MessageId::from_string("test-msg".to_string()),
            body: "test".to_string(),
            attributes: MessageAttributes::new(),
            queue_id: "test-topic".to_string(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id: None,
            deduplication_id: None,
            sequence_number: None,
            delay_seconds: None,
        };

        // Attempt 7: 10 * 2^7 = 1280 > 100, should cap at 100
        let task = DeliveryTask {
            message,
            subscription,
            attempt: 7,
            scheduled_time: Instant::now(),
        };
        let retry = calculate_retry(task, &policy).unwrap();
        assert_eq!(retry.attempt, 8);
        // We can't directly check scheduled_time, but backoff should be capped
    }
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test --lib push_worker`
Expected: FAIL with "module not found"

**Step 4: Add module declaration**

Modify `src/pubsub/mod.rs` to add after push_queue:

```rust
pub mod push_worker;
```

**Step 5: Run test to verify it passes**

Run: `cargo test --lib push_worker`
Expected: PASS (2 tests)

**Step 6: Commit**

```bash
git add src/pubsub/push_worker.rs src/pubsub/mod.rs Cargo.toml
git commit -m "feat(pubsub): add push worker pool with HTTP delivery"
```

---

## Task 4: Integrate with Publisher

**Files:**
- Modify: `src/pubsub/publisher.rs:29-36`
- Modify: `src/pubsub/publisher.rs:100-end` (after existing functions)

**Step 1: Write the failing test**

Add at end of `src/pubsub/publisher.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pubsub::push_queue::DeliveryQueue;
    use crate::storage::memory::MemoryBackend;
    use crate::types::{PushConfig, RetryPolicy};
    use std::time::Instant;

    #[tokio::test]
    async fn test_publish_enqueues_push_subscriptions() {
        let backend = Arc::new(MemoryBackend::new(1000));
        let delivery_queue = DeliveryQueue::new();
        let service = PublisherService::new(backend.clone(), Some(delivery_queue.clone()));

        // Create topic
        let topic = Topic {
            name: "projects/test/topics/test-topic".to_string(),
            labels: HashMap::new(),
            message_storage_policy: None,
            kms_key_name: String::new(),
            schema_settings: None,
            satisfies_pzs: false,
            message_retention_duration: None,
        };
        service.create_topic(Request::new(topic)).await.unwrap();

        // Create push subscription
        let subscription = crate::types::SubscriptionConfig {
            id: "test:test-sub".to_string(),
            name: "projects/test/subscriptions/test-sub".to_string(),
            topic_id: "test:test-topic".to_string(),
            ack_deadline_seconds: 30,
            message_retention_duration: 604800,
            enable_message_ordering: false,
            filter: None,
            dead_letter_policy: None,
            push_config: Some(PushConfig {
                endpoint: "https://example.com/webhook".to_string(),
                retry_policy: Some(RetryPolicy::default()),
                timeout_seconds: Some(30),
            }),
        };
        backend.create_subscription(subscription).await.unwrap();

        // Publish message
        let publish_req = PublishRequest {
            topic: "projects/test/topics/test-topic".to_string(),
            messages: vec![PubsubMessage {
                data: b"test message".to_vec(),
                attributes: HashMap::new(),
                message_id: String::new(),
                publish_time: None,
                ordering_key: String::new(),
            }],
        };

        service.publish(Request::new(publish_req)).await.unwrap();

        // Verify task was enqueued
        assert_eq!(delivery_queue.len().await, 1);

        let task = delivery_queue.dequeue_ready().await.unwrap();
        assert_eq!(task.attempt, 0);
        assert!(task.scheduled_time <= Instant::now());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib test_publish_enqueues_push_subscriptions`
Expected: FAIL with "no method named `new` found for struct `PublisherService`"

**Step 3: Add delivery_queue field to PublisherService**

Modify `PublisherService` struct (line 27-30):

```rust
use crate::pubsub::push_queue::{DeliveryQueue, DeliveryTask};
use std::time::Instant;

/// Publisher service implementation.
pub struct PublisherService {
    backend: Arc<dyn StorageBackend>,
    delivery_queue: Option<DeliveryQueue>,
}

impl PublisherService {
    /// Create a new Publisher service.
    pub fn new(backend: Arc<dyn StorageBackend>, delivery_queue: Option<DeliveryQueue>) -> Self {
        Self {
            backend,
            delivery_queue,
        }
    }
```

**Step 4: Add push subscription enqueueing to publish method**

Find the `publish` method implementation and modify it to enqueue push subscriptions after storing messages. Add this helper method after the `impl Publisher for PublisherService` block:

```rust
impl PublisherService {
    /// Enqueue push subscriptions for a published message.
    async fn enqueue_push_subscriptions(&self, topic_id: &str, message: &Message) {
        if let Some(ref queue) = self.delivery_queue {
            // Get all subscriptions for this topic
            match self.backend.list_subscriptions().await {
                Ok(subscriptions) => {
                    for sub in subscriptions {
                        // Check if subscription is for this topic and has push config
                        if sub.topic_id == topic_id && sub.push_config.is_some() {
                            let task = DeliveryTask {
                                message: message.clone(),
                                subscription: Arc::new(sub),
                                attempt: 0,
                                scheduled_time: Instant::now(),
                            };
                            queue.enqueue(task).await;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get subscriptions for push delivery: {}", e);
                }
            }
        }
    }
}
```

Then modify the `publish` method to call this. Find the publish implementation and add after message sending:

```rust
// After: self.backend.send_message(&topic_id, internal_msg).await?;
// Add:
self.enqueue_push_subscriptions(&topic_id, &internal_msg).await;
```

**Step 5: Run test to verify it passes**

Run: `cargo test --lib test_publish_enqueues_push_subscriptions`
Expected: PASS

**Step 6: Update all PublisherService::new() calls in codebase**

Search for existing usages:

Run: `grep -r "PublisherService::new" src/`

Update each occurrence to pass `None` for delivery_queue initially. Will be updated in next task.

**Step 7: Commit**

```bash
git add src/pubsub/publisher.rs
git commit -m "feat(pubsub): integrate push delivery queue with publisher"
```

---

## Task 5: Wire Up Worker Pool in Main

**Files:**
- Modify: `src/pubsub/grpc_server.rs` (where PublisherService is created)
- Modify: `src/main.rs` or startup code

**Step 1: Find where Pub/Sub services are initialized**

Run: `grep -r "PublisherService::new\|SubscriberService::new" src/`

**Step 2: Update initialization to create worker pool**

In the file where gRPC server is set up (likely `src/pubsub/grpc_server.rs` or `src/main.rs`), add:

```rust
use crate::pubsub::push_queue::DeliveryQueue;
use crate::pubsub::push_worker::PushWorkerPool;

// Create delivery queue
let delivery_queue = DeliveryQueue::new();

// Create worker pool
let push_worker_pool = PushWorkerPool::new(
    None, // Use default worker count
    delivery_queue.clone(),
    backend.clone(),
);

// Create publisher service with delivery queue
let publisher = PublisherService::new(backend.clone(), Some(delivery_queue));
```

**Step 3: Store worker pool handle for graceful shutdown**

Add worker pool to the server struct or main function so it can be shut down gracefully.

**Step 4: Test manually**

Run: `cargo build --release`
Run: `./target/release/lclq start`
Expected: Server starts with "Started push worker pool with N workers" log

**Step 5: Commit**

```bash
git add src/pubsub/grpc_server.rs src/main.rs
git commit -m "feat(pubsub): initialize push worker pool on startup"
```

---

## Task 6: Update Subscription APIs for Push Config

**Files:**
- Modify: `src/pubsub/subscriber.rs:40-80` (subscription_to_config)
- Modify: `src/pubsub/subscriber.rs:82-end` (config_to_subscription)

**Step 1: Write the failing test**

Add to end of `src/pubsub/subscriber.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PushConfig, RetryPolicy};

    #[test]
    fn test_subscription_to_config_with_push() {
        let sub = Subscription {
            name: "projects/test/subscriptions/test-sub".to_string(),
            topic: "projects/test/topics/test-topic".to_string(),
            push_config: Some(push_config::PushConfig {
                push_endpoint: "https://example.com/webhook".to_string(),
                attributes: std::collections::HashMap::new(),
                oidc_token: None,
            }),
            ack_deadline_seconds: 30,
            retain_acked_messages: false,
            message_retention_duration: Some(prost_types::Duration {
                seconds: 604800,
                nanos: 0,
            }),
            labels: std::collections::HashMap::new(),
            enable_message_ordering: false,
            expiration_policy: None,
            filter: String::new(),
            dead_letter_policy: None,
            retry_policy: Some(retry_policy::RetryPolicy {
                minimum_backoff: Some(prost_types::Duration {
                    seconds: 10,
                    nanos: 0,
                }),
                maximum_backoff: Some(prost_types::Duration {
                    seconds: 600,
                    nanos: 0,
                }),
            }),
            detached: false,
            enable_exactly_once_delivery: false,
            topic_message_retention_duration: None,
            state: 0,
        };

        let config = SubscriberService::subscription_to_config(&sub).unwrap();

        assert!(config.push_config.is_some());
        let push_config = config.push_config.unwrap();
        assert_eq!(push_config.endpoint, "https://example.com/webhook");
        assert!(push_config.retry_policy.is_some());

        let retry_policy = push_config.retry_policy.unwrap();
        assert_eq!(retry_policy.min_backoff_seconds, 10);
        assert_eq!(retry_policy.max_backoff_seconds, 600);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib test_subscription_to_config_with_push`
Expected: FAIL with assertion error (push_config is None)

**Step 3: Update subscription_to_config to parse push_config**

Modify `subscription_to_config` method (around line 40-80) to add push_config parsing:

```rust
fn subscription_to_config(subscription: &Subscription) -> Result<SubscriptionConfig> {
    let resource_name = ResourceName::parse(&subscription.name)?;
    let sub_id = format!(
        "{}:{}",
        resource_name.project(),
        resource_name.resource_id()
    );

    // Parse topic name to get topic_id
    let topic_resource = ResourceName::parse(&subscription.topic)?;
    let topic_id = format!(
        "{}:{}",
        topic_resource.project(),
        topic_resource.resource_id()
    );

    // Parse push config if present
    let push_config = subscription.push_config.as_ref().map(|pc| {
        let retry_policy = subscription.retry_policy.as_ref().map(|rp| {
            RetryPolicy {
                min_backoff_seconds: rp
                    .minimum_backoff
                    .as_ref()
                    .map(|d| d.seconds as u32)
                    .unwrap_or(10),
                max_backoff_seconds: rp
                    .maximum_backoff
                    .as_ref()
                    .map(|d| d.seconds as u32)
                    .unwrap_or(600),
                max_attempts: 5, // Not in proto, use default
            }
        });

        PushConfig {
            endpoint: pc.push_endpoint.clone(),
            retry_policy,
            timeout_seconds: Some(30), // Default timeout
        }
    });

    Ok(SubscriptionConfig {
        id: sub_id.clone(),
        name: subscription.name.clone(),
        topic_id,
        ack_deadline_seconds: subscription.ack_deadline_seconds as u32,
        message_retention_duration: subscription
            .message_retention_duration
            .as_ref()
            .map(|d| d.seconds as u32)
            .unwrap_or(604800),
        enable_message_ordering: subscription.enable_message_ordering,
        filter: if subscription.filter.is_empty() {
            None
        } else {
            Some(subscription.filter.clone())
        },
        dead_letter_policy: subscription.dead_letter_policy.as_ref().map(|dlp| {
            crate::types::DeadLetterPolicy {
                dead_letter_topic: dlp.dead_letter_topic.clone(),
                max_delivery_attempts: dlp.max_delivery_attempts as u32,
            }
        }),
        push_config,
    })
}
```

**Step 4: Update config_to_subscription to serialize push_config**

Modify `config_to_subscription` method to add push_config serialization:

```rust
fn config_to_subscription(config: &SubscriptionConfig) -> Subscription {
    // ... existing code ...

    // Serialize push config
    let push_config = config.push_config.as_ref().map(|pc| {
        push_config::PushConfig {
            push_endpoint: pc.endpoint.clone(),
            attributes: std::collections::HashMap::new(),
            oidc_token: None,
        }
    });

    let retry_policy = config
        .push_config
        .as_ref()
        .and_then(|pc| pc.retry_policy.as_ref())
        .map(|rp| retry_policy::RetryPolicy {
            minimum_backoff: Some(prost_types::Duration {
                seconds: rp.min_backoff_seconds as i64,
                nanos: 0,
            }),
            maximum_backoff: Some(prost_types::Duration {
                seconds: rp.max_backoff_seconds as i64,
                nanos: 0,
            }),
        });

    Subscription {
        name: config.name.clone(),
        topic,
        push_config,
        retry_policy,
        // ... rest of fields ...
    }
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test --lib test_subscription_to_config_with_push`
Expected: PASS

**Step 6: Commit**

```bash
git add src/pubsub/subscriber.rs
git commit -m "feat(pubsub): add push config parsing to subscription APIs"
```

---

## Task 7: Update Storage Backend for Push Subscriptions

**Files:**
- Modify: `src/storage/memory.rs` (update create_subscription)
- Modify: `src/storage/sqlite/mod.rs` (update schema and queries)

**Step 1: Update in-memory backend**

Modify `src/storage/memory.rs` to store push_config in subscriptions. The SubscriptionConfig already has the field, so no changes needed unless there's validation.

**Step 2: Update SQLite schema**

Modify `src/storage/sqlite/mod.rs` or migrations to add push config columns:

```sql
ALTER TABLE subscriptions ADD COLUMN push_endpoint TEXT;
ALTER TABLE subscriptions ADD COLUMN retry_min_backoff_seconds INTEGER;
ALTER TABLE subscriptions ADD COLUMN retry_max_backoff_seconds INTEGER;
ALTER TABLE subscriptions ADD COLUMN retry_max_attempts INTEGER;
ALTER TABLE subscriptions ADD COLUMN push_timeout_seconds INTEGER;
```

Create migration file in `migrations/` directory.

**Step 3: Update SQLite insert/select queries**

Update queries to include push config fields when inserting and selecting subscriptions.

**Step 4: Test manually**

Run: `cargo test --lib storage`
Expected: All storage tests pass

**Step 5: Commit**

```bash
git add src/storage/memory.rs src/storage/sqlite/mod.rs migrations/
git commit -m "feat(storage): add push config persistence support"
```

---

## Task 8: Add Integration Test

**Files:**
- Create: `tests/integration/push_subscriptions_test.rs`

**Step 1: Create integration test file**

Create `tests/integration/push_subscriptions_test.rs`:

```rust
//! Integration tests for push subscriptions.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use lclq::pubsub::proto::*;
use lclq::storage::memory::MemoryBackend;
use lclq::pubsub::push_queue::DeliveryQueue;
use lclq::pubsub::push_worker::PushWorkerPool;
use lclq::pubsub::publisher::PublisherService;
use lclq::pubsub::subscriber::SubscriberService;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Request;

#[derive(Clone)]
struct TestWebhookState {
    messages: Arc<Mutex<Vec<Value>>>,
}

async fn webhook_handler(
    State(state): State<TestWebhookState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let mut messages = state.messages.lock().await;
    messages.push(payload);
    StatusCode::OK
}

#[tokio::test]
async fn test_push_subscription_delivery() {
    // Start test webhook server
    let state = TestWebhookState {
        messages: Arc::new(Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/webhook", post(webhook_handler))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let webhook_url = format!("http://{}/webhook", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create backend and services
    let backend = Arc::new(MemoryBackend::new(1000));
    let delivery_queue = DeliveryQueue::new();
    let worker_pool = PushWorkerPool::new(Some(2), delivery_queue.clone(), backend.clone());

    let publisher = PublisherService::new(backend.clone(), Some(delivery_queue));
    let subscriber = SubscriberService::new(backend.clone());

    // Create topic
    let topic_req = Request::new(Topic {
        name: "projects/test/topics/push-test".to_string(),
        ..Default::default()
    });
    publisher.create_topic(topic_req).await.unwrap();

    // Create push subscription
    let sub_req = Request::new(Subscription {
        name: "projects/test/subscriptions/push-sub".to_string(),
        topic: "projects/test/topics/push-test".to_string(),
        push_config: Some(push_config::PushConfig {
            push_endpoint: webhook_url.clone(),
            attributes: std::collections::HashMap::new(),
            oidc_token: None,
        }),
        retry_policy: Some(retry_policy::RetryPolicy {
            minimum_backoff: Some(prost_types::Duration {
                seconds: 1,
                nanos: 0,
            }),
            maximum_backoff: Some(prost_types::Duration {
                seconds: 5,
                nanos: 0,
            }),
        }),
        ack_deadline_seconds: 30,
        ..Default::default()
    });
    subscriber.create_subscription(sub_req).await.unwrap();

    // Publish message
    let pub_req = Request::new(PublishRequest {
        topic: "projects/test/topics/push-test".to_string(),
        messages: vec![PubsubMessage {
            data: b"Hello, Push!".to_vec(),
            attributes: [("key".to_string(), "value".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }],
    });
    publisher.publish(pub_req).await.unwrap();

    // Wait for delivery (with timeout)
    for _ in 0..50 {
        let messages = state.messages.lock().await;
        if !messages.is_empty() {
            break;
        }
        drop(messages);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Verify message was delivered
    let messages = state.messages.lock().await;
    assert_eq!(messages.len(), 1);

    let message = &messages[0];
    assert!(message["message"]["data"].is_string());
    assert_eq!(
        message["subscription"].as_str().unwrap(),
        "projects/test/subscriptions/push-sub"
    );

    // Decode base64 data
    let data_b64 = message["message"]["data"].as_str().unwrap();
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .unwrap();
    assert_eq!(std::str::from_utf8(&data).unwrap(), "Hello, Push!");

    // Shutdown worker pool
    worker_pool.shutdown().await;
}

#[tokio::test]
async fn test_push_subscription_retry_on_failure() {
    // Start test webhook server that fails first N times
    let state = Arc::new(Mutex::new(0u32));
    let state_clone = state.clone();

    let app = Router::new().route(
        "/webhook",
        post(move || {
            let state = state_clone.clone();
            async move {
                let mut count = state.lock().await;
                *count += 1;

                // Fail first 2 attempts, succeed on 3rd
                if *count < 3 {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else {
                    StatusCode::OK
                }
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let webhook_url = format!("http://{}/webhook", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create backend and services with fast retry
    let backend = Arc::new(MemoryBackend::new(1000));
    let delivery_queue = DeliveryQueue::new();
    let worker_pool = PushWorkerPool::new(Some(2), delivery_queue.clone(), backend.clone());

    let publisher = PublisherService::new(backend.clone(), Some(delivery_queue));
    let subscriber = SubscriberService::new(backend.clone());

    // Create topic and subscription with fast retry
    let topic_req = Request::new(Topic {
        name: "projects/test/topics/retry-test".to_string(),
        ..Default::default()
    });
    publisher.create_topic(topic_req).await.unwrap();

    let sub_req = Request::new(Subscription {
        name: "projects/test/subscriptions/retry-sub".to_string(),
        topic: "projects/test/topics/retry-test".to_string(),
        push_config: Some(push_config::PushConfig {
            push_endpoint: webhook_url,
            attributes: std::collections::HashMap::new(),
            oidc_token: None,
        }),
        retry_policy: Some(retry_policy::RetryPolicy {
            minimum_backoff: Some(prost_types::Duration {
                seconds: 1, // Fast retry for testing
                nanos: 0,
            }),
            maximum_backoff: Some(prost_types::Duration {
                seconds: 2,
                nanos: 0,
            }),
        }),
        ack_deadline_seconds: 30,
        ..Default::default()
    });
    subscriber.create_subscription(sub_req).await.unwrap();

    // Publish message
    let pub_req = Request::new(PublishRequest {
        topic: "projects/test/topics/retry-test".to_string(),
        messages: vec![PubsubMessage {
            data: b"Retry me!".to_vec(),
            ..Default::default()
        }],
    });
    publisher.publish(pub_req).await.unwrap();

    // Wait for retries and success
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Verify it was retried 3 times
    let count = state.lock().await;
    assert_eq!(*count, 3);

    worker_pool.shutdown().await;
}
```

**Step 2: Run integration test**

Run: `cargo test --test push_subscriptions_test`
Expected: PASS (2 tests)

**Step 3: Commit**

```bash
git add tests/integration/push_subscriptions_test.rs
git commit -m "test: add integration tests for push subscriptions"
```

---

## Task 9: Add Documentation

**Files:**
- Create: `docs/push-subscriptions.md`
- Modify: `README.md` (add push subscriptions feature)

**Step 1: Create documentation**

Create `docs/push-subscriptions.md`:

```markdown
# Push Subscriptions

## Overview

lclq supports GCP Pub/Sub push subscriptions, which automatically deliver messages to HTTP/HTTPS endpoints. This is useful for webhook-style integrations where you want messages pushed to your application rather than polling.

## Features

- HTTP/HTTPS endpoint delivery
- Configurable exponential backoff retry
- Dead letter topic support for failed deliveries
- GCP Pub/Sub compatible JSON format

## Creating a Push Subscription

### Using Python SDK

\`\`\`python
from google.cloud import pubsub_v1

# Set up client
subscriber = pubsub_v1.SubscriberClient()

# Create push subscription
subscription_path = subscriber.subscription_path('my-project', 'my-push-sub')
topic_path = subscriber.topic_path('my-project', 'my-topic')

subscription = subscriber.create_subscription(
    request={
        "name": subscription_path,
        "topic": topic_path,
        "push_config": {
            "push_endpoint": "https://example.com/webhook"
        },
        "retry_policy": {
            "minimum_backoff": {"seconds": 10},
            "maximum_backoff": {"seconds": 600}
        }
    }
)
\`\`\`

## Webhook Format

Messages are delivered as HTTP POST requests with JSON payload:

\`\`\`json
{
  "message": {
    "data": "SGVsbG8sIFdvcmxkIQ==",
    "attributes": {
      "key": "value"
    },
    "messageId": "123456789",
    "publishTime": "2025-12-02T10:30:00Z"
  },
  "subscription": "projects/my-project/subscriptions/my-push-sub"
}
\`\`\`

The `data` field is base64-encoded.

## Acknowledgment

Your webhook endpoint should:
- Return HTTP 2xx status code to acknowledge successful delivery
- Return HTTP 4xx/5xx or timeout to trigger retry

## Retry Policy

Default retry policy:
- **Min backoff**: 10 seconds
- **Max backoff**: 600 seconds (10 minutes)
- **Max attempts**: 5
- **Backoff**: Exponential (min × 2^attempt, capped at max)

### Retry Example

- Attempt 1: Immediate
- Attempt 2: 10 seconds delay
- Attempt 3: 20 seconds delay
- Attempt 4: 40 seconds delay
- Attempt 5: 80 seconds delay
- After 5 attempts: Move to dead letter topic (if configured)

## Dead Letter Topics

Configure a dead letter topic to receive messages that exceed max retry attempts:

\`\`\`python
subscription = subscriber.create_subscription(
    request={
        "name": subscription_path,
        "topic": topic_path,
        "push_config": {
            "push_endpoint": "https://example.com/webhook"
        },
        "dead_letter_policy": {
            "dead_letter_topic": subscriber.topic_path('my-project', 'my-dlq'),
            "max_delivery_attempts": 5
        }
    }
)
\`\`\`

Failed messages will be published to the DLQ with additional attributes:
- `original_subscription`: Source subscription name
- `failure_reason`: "max_push_attempts_exceeded"
- `attempts`: Number of delivery attempts

## Configuration

### Worker Pool

Configure the number of push worker threads:

\`\`\`toml
# lclq.toml
[pubsub.push]
workers = 16  # Default: num_cpus * 2
\`\`\`

### Default Timeout

\`\`\`toml
[pubsub.push]
timeout_seconds = 30  # Default: 30 seconds
\`\`\`

## Monitoring

Push delivery metrics (coming soon):
- `lclq_push_deliveries_total` - Total delivery attempts
- `lclq_push_delivery_failures_total` - Failed deliveries
- `lclq_push_delivery_duration_seconds` - Delivery latency

## Limitations

- Push endpoints must be HTTP or HTTPS
- No JWT authentication (coming in future release)
- No subscription confirmation flow (coming in future release)
```

**Step 2: Update README.md**

Add to the Features section:

```markdown
### GCP Pub/Sub Push Subscriptions

- HTTP/HTTPS webhook delivery
- Configurable exponential backoff retry
- Dead letter topic support
- GCP Pub/Sub compatible JSON format

See [Push Subscriptions Documentation](docs/push-subscriptions.md) for details.
```

**Step 3: Commit**

```bash
git add docs/push-subscriptions.md README.md
git commit -m "docs: add push subscriptions documentation"
```

---

## Task 10: Final Testing and Cleanup

**Step 1: Run all tests**

Run: `cargo test --all`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 3: Run format check**

Run: `cargo fmt -- --check`
Expected: All files formatted correctly

**Step 4: Build release**

Run: `cargo build --release`
Expected: Successful build

**Step 5: Manual end-to-end test**

1. Start server: `./target/release/lclq start`
2. Create topic via API
3. Create push subscription pointing to test endpoint
4. Publish message
5. Verify delivery to endpoint

**Step 6: Final commit**

```bash
git add -A
git commit -m "feat: complete push subscriptions implementation

Added HTTP push delivery for Pub/Sub subscriptions:
- Background worker pool with configurable concurrency
- Priority delivery queue with exponential backoff retry
- Dead letter topic support for failed messages
- GCP Pub/Sub compatible JSON format
- Integration tests and documentation

Closes #[issue-number]
"
```

---

## Execution Complete

**Summary:**
- ✅ 10 tasks completed
- ✅ Push configuration types added
- ✅ Delivery queue with priority scheduling
- ✅ Worker pool with HTTP delivery
- ✅ Integration with publisher
- ✅ Storage backend support
- ✅ API updates for push config
- ✅ Integration tests
- ✅ Documentation

**Next Steps:**
- Test with real applications
- Add metrics and monitoring
- Consider JWT authentication
- Consider subscription confirmation flow
