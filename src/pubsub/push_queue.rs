//! Push delivery queue with priority scheduling.

use crate::types::{Message, SubscriptionConfig};
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
    use crate::types::{MessageId, PushConfig, RetryPolicy};
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
