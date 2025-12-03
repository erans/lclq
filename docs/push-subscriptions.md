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

```python
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
```

## Webhook Format

Messages are delivered as HTTP POST requests with JSON payload:

```json
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
```

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

```python
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
```

Failed messages will be published to the DLQ with additional attributes:
- `original_subscription`: Source subscription name
- `failure_reason`: "max_push_attempts_exceeded"
- `attempts`: Number of delivery attempts

## Configuration

### Worker Pool

Configure the number of push delivery worker threads via environment variable:

```bash
# Default: 2 workers
LCLQ_PUSH_WORKERS=2 lclq start

# For high-throughput scenarios
LCLQ_PUSH_WORKERS=8 lclq start

# Or set in your environment
export LCLQ_PUSH_WORKERS=4
lclq start
```

**Worker Pool Sizing:**
- **Default: 2 workers** - Suitable for most development and testing scenarios
- **Low traffic**: 1-2 workers
- **Medium traffic**: 4-8 workers
- **High traffic**: 16+ workers

Each worker can handle concurrent HTTP requests, so 2 workers is typically sufficient for local development. Increase the worker count if you're testing high-throughput push delivery scenarios.

### Default Timeout

The default HTTP request timeout is 30 seconds per delivery attempt. This can be configured per-subscription via the push config:

```python
subscription = subscriber.create_subscription(
    request={
        "name": subscription_path,
        "topic": topic_path,
        "push_config": {
            "push_endpoint": "https://example.com/webhook"
        }
    }
)
# Note: Per-subscription timeout configuration coming in future release
```

## Monitoring

Push delivery metrics (coming soon):
- `lclq_push_deliveries_total` - Total delivery attempts
- `lclq_push_delivery_failures_total` - Failed deliveries
- `lclq_push_delivery_duration_seconds` - Delivery latency

## Limitations

- Push endpoints must be HTTP or HTTPS
- No JWT authentication (coming in future release)
- No subscription confirmation flow (coming in future release)
