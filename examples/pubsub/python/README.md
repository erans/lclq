# Pub/Sub Python Example

Simple publisher/subscriber example for GCP Pub/Sub using google-cloud-pubsub.

## Prerequisites

- Python 3.7+
- google-cloud-pubsub library
- lclq running on localhost:8085

## Installation

```bash
pip install google-cloud-pubsub
```

## Usage

### 1. Start lclq

In a separate terminal:
```bash
lclq start
```

### 2. Run the Publisher

```bash
python publisher.py
```

This will:
- Create a topic named `example-topic`
- Publish 10 messages with attributes
- Display progress

### 3. Run the Subscriber

In another terminal:
```bash
python subscriber.py
```

This will:
- Create a subscription to `example-topic`
- Pull messages from the subscription
- Display received messages
- Acknowledge messages after processing
- Run continuously until you press Ctrl+C

## Example Output

**Publisher:**
```
Creating topic: projects/example-project/topics/example-topic
Topic created: projects/example-project/topics/example-topic

Publishing messages...
  ✓ Published: Message 0 sent at 2025-01-23T10:30:00.123456
    Message ID: 1234567890
  ✓ Published: Message 1 sent at 2025-01-23T10:30:00.623456
    Message ID: 1234567891
  ...

✓ Successfully published 10 messages to example-topic
```

**Subscriber:**
```
Creating subscription: projects/example-project/subscriptions/example-subscription
Subscription created: projects/example-project/subscriptions/example-subscription

Polling for messages (press Ctrl+C to stop)...

Received message #1:
  Data: Message 0 sent at 2025-01-23T10:30:00.123456
  Message ID: 1234567890
  Attributes:
    message_number: 0
    sender: publisher.py
    timestamp: 2025-01-23T10:30:00.123456
  ✓ Acknowledged
```

## Code Explanation

### Publisher

1. **Set Emulator Host**: Points to lclq instead of GCP
   ```python
   os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'
   ```

2. **Create Publisher Client**: Must be after setting environment variable
   ```python
   publisher = pubsub_v1.PublisherClient()
   ```

3. **Create Topic**: Creates a topic (idempotent operation)
   ```python
   topic = publisher.create_topic(request={"name": topic_path})
   ```

4. **Publish Messages**: Publishes messages with custom attributes
   ```python
   future = publisher.publish(topic_path, data, attribute1='value', ...)
   message_id = future.result()  # Wait for publish to complete
   ```

### Subscriber

1. **Create Subscription**: Subscribes to the topic
   ```python
   subscription = subscriber.create_subscription(
       request={"name": subscription_path, "topic": topic_path}
   )
   ```

2. **Pull Messages**: Pulls up to 10 messages
   ```python
   response = subscriber.pull(
       request={"subscription": subscription_path, "max_messages": 10}
   )
   ```

3. **Acknowledge Messages**: Confirms message processing
   ```python
   ack_ids = [msg.ack_id for msg in response.received_messages]
   subscriber.acknowledge(
       request={"subscription": subscription_path, "ack_ids": ack_ids}
   )
   ```

## Tips

- **Environment Variable**: Must set `PUBSUB_EMULATOR_HOST` before importing
- **Acknowledgments**: Messages not acknowledged will be redelivered
- **Message Ordering**: Use `ordering_key` parameter and enable message ordering
- **Error Handling**: Add retry logic and dead letter topics for production use

## Next Steps

- Try modifying the publisher to use message ordering keys
- Add error handling and retry logic to the subscriber
- Experiment with multiple subscriptions on the same topic
- Set up a dead letter topic for failed messages
