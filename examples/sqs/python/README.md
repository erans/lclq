# SQS Python Example

Simple producer/consumer example for AWS SQS using boto3.

## Prerequisites

- Python 3.7+
- boto3 library
- lclq running on localhost:9324

## Installation

```bash
pip install boto3
```

## Usage

### 1. Start lclq

In a separate terminal:
```bash
lclq start
```

### 2. Run the Producer

```bash
python producer.py
```

This will:
- Create a queue named `example-queue`
- Send 10 messages with attributes
- Display progress

### 3. Run the Consumer

In another terminal:
```bash
python consumer.py
```

This will:
- Connect to the `example-queue`
- Poll for messages (long polling)
- Display received messages
- Delete messages after processing
- Run continuously until you press Ctrl+C

## Example Output

**Producer:**
```
Creating queue: example-queue
Queue URL: http://127.0.0.1:9324/queue/example-queue

Sending messages...
  ✓ Sent: Message 0 sent at 2025-01-23T10:30:00.123456
    Message ID: d4e5f6g7-h8i9-j0k1-l2m3-n4o5p6q7r8s9
  ✓ Sent: Message 1 sent at 2025-01-23T10:30:00.623456
    Message ID: a1b2c3d4-e5f6-g7h8-i9j0-k1l2m3n4o5p6
  ...

✓ Successfully sent 10 messages to example-queue
```

**Consumer:**
```
Getting queue URL for: example-queue
Queue URL: http://127.0.0.1:9324/queue/example-queue

Polling for messages (press Ctrl+C to stop)...

Received message #1:
  Body: Message 0 sent at 2025-01-23T10:30:00.123456
  Message ID: d4e5f6g7-h8i9-j0k1-l2m3-n4o5p6q7r8s9
  Attributes:
    MessageNumber: {'StringValue': '0', 'DataType': 'Number'}
    Sender: {'StringValue': 'producer.py', 'DataType': 'String'}
    Timestamp: {'StringValue': '2025-01-23T10:30:00.123456', 'DataType': 'String'}
  ✓ Deleted
```

## Code Explanation

### Producer

1. **Create SQS Client**: Points to lclq instead of AWS
   ```python
   sqs = boto3.client('sqs', endpoint_url='http://localhost:9324', ...)
   ```

2. **Create Queue**: Creates a queue (idempotent operation)
   ```python
   response = sqs.create_queue(QueueName='example-queue')
   ```

3. **Send Messages**: Sends messages with custom attributes
   ```python
   sqs.send_message(QueueUrl=queue_url, MessageBody=..., MessageAttributes=...)
   ```

### Consumer

1. **Get Queue URL**: Retrieves the queue URL by name
   ```python
   response = sqs.get_queue_url(QueueName='example-queue')
   ```

2. **Receive Messages**: Long polls for up to 10 messages
   ```python
   response = sqs.receive_message(
       QueueUrl=queue_url,
       MaxNumberOfMessages=10,
       WaitTimeSeconds=5  # Long polling
   )
   ```

3. **Delete Messages**: Removes processed messages from queue
   ```python
   sqs.delete_message(QueueUrl=queue_url, ReceiptHandle=...)
   ```

## Tips

- **Long Polling**: Setting `WaitTimeSeconds` reduces empty responses and saves CPU
- **Batch Operations**: Use `send_message_batch` for better throughput
- **Visibility Timeout**: Messages become invisible for 30 seconds by default after receiving
- **Error Handling**: Add retry logic and DLQ configuration for production use

## Next Steps

- Try modifying the producer to send different message types
- Add error handling and retry logic to the consumer
- Experiment with FIFO queues (use `.fifo` suffix)
- Set up a Dead Letter Queue for failed messages
