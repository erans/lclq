# Pub/Sub Push Subscriptions Design

**Date:** 2025-12-02
**Status:** Approved
**Feature:** HTTP Push Delivery for Pub/Sub Subscriptions

## Overview

This design extends lclq's existing Pub/Sub implementation to support push subscriptions, enabling automatic HTTP/HTTPS delivery of messages to registered endpoints. This matches Google Cloud Pub/Sub push subscription behavior with configurable retry policies and dead letter topic support.

## Goals

- Add push delivery mode to existing Pub/Sub subscriptions
- Implement configurable exponential backoff retry policy
- Support dead letter topics for failed deliveries
- Maintain API compatibility with GCP Pub/Sub push subscriptions
- Use background worker pool for efficient, non-blocking delivery

## Non-Goals

- JWT authentication (future enhancement)
- Subscription confirmation flow (future enhancement)
- Raw message delivery option (future enhancement)
- Per-subscription rate limiting (future enhancement)

## High-Level Architecture

### Components

1. **Subscription Manager** - Manages push subscription lifecycle
   - Stores push subscription config (endpoint URL, retry policy, DLT topic)
   - Validates endpoint URLs
   - Links subscriptions to topics

2. **Push Delivery Queue** - In-memory priority queue for delivery work
   - New messages: priority 0, immediate delivery
   - Retry messages: scheduled delivery time based on exponential backoff
   - Thread-safe, supports multiple producers/consumers

3. **Worker Pool** - Fixed-size pool of push workers
   - Default size: `num_cpus * 2` (configurable)
   - Workers pull from delivery queue
   - Execute HTTP POST with timeout
   - Handle success/retry/DLT logic

4. **Retry Scheduler** - Manages exponential backoff
   - Calculates next retry delay: `min(min_backoff * 2^attempt, max_backoff)`
   - Tracks attempt count per message
   - Moves to DLT after max_attempts exceeded

### Message Flow

```
Publish → Topic → [For each push subscription]
  → Push Delivery Queue → Worker Pool → HTTP POST
  → Success: Ack | Failure: Retry Schedule | Max Retries: DLT
```

### Persistence

- **SQLite backend:** Push subscriptions stored in database, survive restarts
- **In-memory backend:** Push subscriptions are ephemeral (consistent with current behavior)

## API Design

### Push Subscription Configuration

```rust
enum SubscriptionType {
    Pull,
    Push(PushConfig),
}

struct PushConfig {
    endpoint: String,              // Required: HTTP/HTTPS URL
    retry_policy: Option<RetryPolicy>,
    timeout_seconds: Option<u32>,  // Default: 30s
}

struct RetryPolicy {
    min_backoff_seconds: Option<u32>,  // Default: 10
    max_backoff_seconds: Option<u32>,  // Default: 600
    max_attempts: Option<u32>,         // Default: 5
}
```

### CreateSubscription Request

**JSON/REST Format:**
```json
{
  "name": "projects/my-project/subscriptions/my-push-sub",
  "topic": "projects/my-project/topics/my-topic",
  "pushConfig": {
    "pushEndpoint": "https://example.com/webhook",
    "retryPolicy": {
      "minimumBackoff": "10s",
      "maximumBackoff": "600s"
    }
  },
  "ackDeadlineSeconds": 30
}
```

**Key Points:**
- Empty `pushConfig` or missing = Pull subscription (existing behavior)
- `pushConfig` with `pushEndpoint` = Push subscription
- Retry policy fields are optional, use smart defaults
- Endpoint URL must be HTTP/HTTPS (validate on creation)
- UpdateSubscription can convert Pull ↔ Push

### Storage Schema

Extend existing `subscriptions` table:

```sql
ALTER TABLE subscriptions ADD COLUMN push_endpoint TEXT;
ALTER TABLE subscriptions ADD COLUMN retry_policy JSON;
ALTER TABLE subscriptions ADD COLUMN timeout_seconds INTEGER;
```

## Worker Pool Implementation

### Initialization

```rust
struct PushWorkerPool {
    workers: Vec<JoinHandle<()>>,
    delivery_queue: Arc<DeliveryQueue>,
    shutdown_signal: Arc<AtomicBool>,
    http_client: Arc<reqwest::Client>,
}

impl PushWorkerPool {
    fn new(num_workers: usize, backend: Arc<dyn Backend>) -> Self {
        // Create shared HTTP client with connection pooling
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(num_workers)
            .build()
            .unwrap();

        // Spawn workers
        let workers = (0..num_workers)
            .map(|id| spawn_worker(id, queue.clone(), client.clone()))
            .collect();

        Self { workers, delivery_queue, shutdown_signal, http_client }
    }
}
```

### Delivery Queue

```rust
struct DeliveryQueue {
    // Priority queue: messages sorted by scheduled_time
    queue: Mutex<BinaryHeap<DeliveryTask>>,
    notify: Condvar,  // Wake workers when new work arrives
}

struct DeliveryTask {
    message: PubsubMessage,
    subscription: PushSubscription,
    attempt: u32,
    scheduled_time: Instant,  // When to deliver
}

impl Ord for DeliveryTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Earlier scheduled_time = higher priority (reverse order)
        other.scheduled_time.cmp(&self.scheduled_time)
    }
}
```

### Worker Loop

1. Wait for task with `scheduled_time <= now()`
2. Execute HTTP POST to endpoint
3. **On 2xx response:** Done (success)
4. **On timeout/error/non-2xx:** Calculate backoff, re-queue with new `scheduled_time`
5. **On max_attempts exceeded:** Publish to DLT topic if configured

### Graceful Shutdown

1. Set `shutdown_signal` flag
2. Workers finish current requests (with timeout)
3. Join worker threads
4. Log remaining tasks in queue

## Retry Logic & Dead Letter Topics

### Exponential Backoff Calculation

```rust
fn calculate_next_attempt(task: &DeliveryTask, config: &RetryPolicy) -> Option<DeliveryTask> {
    if task.attempt >= config.max_attempts {
        return None;  // Exceeded max attempts → DLT
    }

    // Exponential backoff: min(min * 2^attempt, max)
    let backoff_secs = config.min_backoff_seconds * 2_u32.pow(task.attempt);
    let backoff_secs = backoff_secs.min(config.max_backoff_seconds);

    Some(DeliveryTask {
        scheduled_time: Instant::now() + Duration::from_secs(backoff_secs),
        attempt: task.attempt + 1,
        ..task.clone()
    })
}
```

### Retry Triggers

- HTTP timeout (after `timeout_seconds`)
- Connection error (network unreachable, DNS failure, etc.)
- Non-2xx response codes (4xx, 5xx)

### Dead Letter Topic Flow

When max attempts exceeded:

```rust
fn handle_max_retries(task: DeliveryTask, subscription: &PushSubscription, backend: &dyn Backend) {
    if let Some(dlq_topic) = &subscription.dead_letter_topic {
        // Publish original message to DLT with metadata
        backend.publish(dlq_topic, PubsubMessage {
            data: task.message.data,
            attributes: {
                "original_subscription": subscription.name.clone(),
                "failure_reason": "max_push_attempts_exceeded".to_string(),
                "attempts": task.attempt.to_string(),
            },
            ..task.message
        }).await;
    } else {
        // Log and drop
        warn!("Message dropped after {} attempts for subscription {}",
              task.attempt, subscription.name);
    }
}
```

**Key Points:**
- DLT is just another Pub/Sub topic (can have its own subscriptions)
- Original message preserved with failure metadata
- No DLT configured? Message logged and dropped

## HTTP Delivery Format

### Request Format

Match GCP Pub/Sub push format for compatibility:

```rust
async fn deliver_message(
    client: &reqwest::Client,
    endpoint: &str,
    message: &PubsubMessage,
    subscription: &str,
    timeout: Duration,
) -> Result<(), DeliveryError> {
    let payload = json!({
        "message": {
            "data": base64::encode(&message.data),
            "attributes": message.attributes,
            "messageId": message.message_id,
            "publishTime": message.publish_time.to_rfc3339(),
            "orderingKey": message.ordering_key,
        },
        "subscription": subscription,
    });

    let response = client.post(endpoint)
        .header("Content-Type", "application/json")
        .header("User-Agent", format!("lclq-push/{}", env!("CARGO_PKG_VERSION")))
        .json(&payload)
        .timeout(timeout)
        .send()
        .await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        status => Err(DeliveryError::HttpError(status)),
    }
}
```

### Request Headers

- `Content-Type: application/json`
- `User-Agent: lclq-push/<version>`
- Future: `Authorization: Bearer <jwt>` (if auth enabled)

### Response Handling

- **2xx (200-299):** Success, message acknowledged
- **4xx/5xx:** Retry with backoff
- **Timeout:** Retry with backoff
- **Connection refused/DNS failure:** Retry with backoff

**Important:** Message data is base64-encoded in JSON payload (matches GCP format). This ensures binary-safe delivery.

## Integration with Existing Pub/Sub

### Message Publishing Flow

Extend current publishing logic:

```rust
// In publisher.rs
pub async fn publish_message(
    backend: &dyn Backend,
    topic: &str,
    message: PubsubMessage,
    push_queue: &DeliveryQueue,  // New parameter
) -> Result<String> {
    // Existing: Store message in backend
    let message_id = backend.store_message(topic, &message).await?;

    // Existing: Notify pull subscriptions (unchanged)
    notify_pull_subscriptions(backend, topic, &message).await;

    // NEW: Enqueue for push subscriptions
    let push_subs = backend.get_push_subscriptions(topic).await?;
    for sub in push_subs {
        push_queue.enqueue(DeliveryTask {
            message: message.clone(),
            subscription: sub,
            attempt: 0,
            scheduled_time: Instant::now(),  // Immediate delivery
        });
    }

    Ok(message_id)
}
```

### Component Changes

1. **`src/pubsub/types.rs`** - Add `PushConfig`, `RetryPolicy` structs
2. **`src/pubsub/subscriber.rs`** - Extend subscription creation/update APIs
3. **`src/pubsub/push_worker.rs`** (NEW) - Worker pool implementation
4. **`src/pubsub/push_queue.rs`** (NEW) - Delivery queue with priority scheduling
5. **`src/storage/backend.rs`** - Add `get_push_subscriptions()` method
6. **`src/pubsub/mod.rs`** - Initialize worker pool on startup

### Startup Sequence

```rust
// In main.rs or pubsub/mod.rs
let push_worker_pool = PushWorkerPool::new(
    config.push_workers.unwrap_or(num_cpus::get() * 2),
    backend.clone(),
);

// Pass push_queue to publisher
let publisher = Publisher::new(backend.clone(), push_worker_pool.queue());
```

## Configuration

### Default Values

```rust
const DEFAULT_MIN_BACKOFF_SECONDS: u32 = 10;
const DEFAULT_MAX_BACKOFF_SECONDS: u32 = 600;
const DEFAULT_MAX_ATTEMPTS: u32 = 5;
const DEFAULT_TIMEOUT_SECONDS: u32 = 30;
const DEFAULT_WORKER_COUNT: usize = num_cpus::get() * 2;
```

### lclq.toml Configuration

```toml
[pubsub.push]
workers = 16              # Override default worker count
timeout_seconds = 30      # Default timeout for all push subscriptions
```

## Testing Strategy

### Unit Tests

- Exponential backoff calculation
- Delivery queue ordering
- Worker pool lifecycle
- HTTP request formatting

### Integration Tests

1. **Basic push delivery** - Publish message, verify HTTP POST received
2. **Retry on failure** - Simulate 500 error, verify retries with backoff
3. **Dead letter topic** - Exhaust retries, verify message in DLT
4. **Concurrent delivery** - Multiple subscriptions, verify parallel delivery
5. **Graceful shutdown** - Stop server mid-delivery, verify cleanup

### Test Tools

- Local HTTP server (using `axum` or `warp`) to receive push requests
- Mock slow/failing endpoints
- Verify retry timing with `tokio::time::sleep`

## Performance Considerations

- **Worker count:** Default `num_cpus * 2` handles typical workloads
- **HTTP client:** Connection pooling reduces overhead
- **Memory:** Delivery queue bounded by message count * subscription count
- **Backoff timing:** Max backoff 600s prevents excessive delays

## Security Considerations

- **URL validation:** Only allow `http://` and `https://` schemes
- **Timeout enforcement:** Prevent hanging on slow endpoints
- **Future:** JWT signing for endpoint authentication
- **Future:** TLS certificate validation options

## Future Enhancements

1. **JWT Authentication** - Sign requests with JWT for endpoint verification
2. **Subscription Confirmation** - Require endpoint to confirm subscription (AWS SNS style)
3. **Raw Message Delivery** - Option to send message body directly without JSON wrapper
4. **Per-subscription Rate Limiting** - Limit request rate to protect endpoints
5. **Metrics** - Expose push delivery metrics (success rate, retry count, latency)
6. **Push to localhost** - Allow `http://localhost` for local testing (currently only HTTPS)

## References

- [GCP Pub/Sub Push Subscriptions](https://cloud.google.com/pubsub/docs/push)
- [GCP Subscription Retry Policy](https://cloud.google.com/pubsub/docs/subscription-retry-policy)
- [AWS SNS HTTP/HTTPS Subscriptions](https://docs.aws.amazon.com/sns/latest/dg/SendMessageToHttp.html)

## Decision Log

| Decision | Rationale |
|----------|-----------|
| Background worker pool over immediate push | Decouples publishing from delivery, prevents slow endpoints from blocking publishers |
| Shared worker pool over per-subscription workers | More efficient resource usage, better handling of variable load |
| API-only config with optional persistence | Consistent with existing lclq behavior, simple mental model |
| Smart defaults with overrides | Easy for common case, flexible for power users |
| GCP Pub/Sub JSON format | Industry standard, well-documented, SDK compatible |
| Exponential backoff (10s-600s) | Matches GCP defaults, proven in production systems |
