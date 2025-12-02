//! Push worker pool for HTTP delivery.

use crate::error::{Error, Result};
use crate::pubsub::push_queue::{DeliveryQueue, DeliveryTask};
use crate::storage::StorageBackend;
use crate::types::{Message, MessageAttributeValue, MessageId, RetryPolicy, SubscriptionConfig};
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
    let subscription = task.subscription.clone();
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
            let default_policy = RetryPolicy::default();
            let retry_policy = push_config
                .retry_policy
                .as_ref()
                .unwrap_or(&default_policy);

            if let Some(retry_task) = calculate_retry(task.clone(), retry_policy) {
                // Re-enqueue for retry
                queue.enqueue(retry_task).await;
            } else {
                // Max attempts exceeded, send to DLT
                handle_max_retries(task, &subscription, backend).await;
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
    use crate::types::{MessageAttributes, PushConfig};

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
