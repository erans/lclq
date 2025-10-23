# Quick Start Guide

Get started with lclq in under 5 minutes. This guide covers installation, basic usage, and common patterns for both AWS SQS and GCP Pub/Sub.

## Table of Contents

- [Installation](#installation)
- [Starting the Server](#starting-the-server)
- [AWS SQS Quick Start](#aws-sqs-quick-start)
  - [Python (boto3)](#python-boto3)
  - [JavaScript (AWS SDK v3)](#javascript-aws-sdk-v3)
  - [Go (AWS SDK v2)](#go-aws-sdk-v2)
  - [Rust (AWS SDK)](#rust-aws-sdk)
- [GCP Pub/Sub Quick Start](#gcp-pubsub-quick-start)
  - [Python (google-cloud-pubsub)](#python-google-cloud-pubsub)
  - [JavaScript (@google-cloud/pubsub)](#javascript-google-cloudpubsub)
  - [Go (cloud.google.com/go/pubsub)](#go-cloudgooglecomgopubsub)
- [Common Patterns](#common-patterns)
- [Troubleshooting](#troubleshooting)

## Installation

### Option 1: Binary Release (Recommended)

Download the latest release for your platform:

```bash
# Linux x86_64
curl -sSL https://github.com/yourusername/lclq/releases/latest/download/lclq-linux-x86_64 -o lclq
chmod +x lclq
sudo mv lclq /usr/local/bin/

# macOS (Apple Silicon)
curl -sSL https://github.com/yourusername/lclq/releases/latest/download/lclq-macos-aarch64 -o lclq
chmod +x lclq
sudo mv lclq /usr/local/bin/

# macOS (Intel)
curl -sSL https://github.com/yourusername/lclq/releases/latest/download/lclq-macos-x86_64 -o lclq
chmod +x lclq
sudo mv lclq /usr/local/bin/

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/yourusername/lclq/releases/latest/download/lclq-windows-x86_64.exe -OutFile lclq.exe
```

### Option 2: Docker

```bash
docker pull lclq/lclq:latest
```

### Option 3: From Source

```bash
cargo install lclq
```

## Starting the Server

### Basic Start

```bash
lclq start
```

This starts all services with default settings:
- **AWS SQS HTTP API**: http://localhost:9324
- **GCP Pub/Sub gRPC API**: localhost:8085
- **Admin API**: http://localhost:9000
- **Prometheus Metrics**: http://localhost:9090/metrics

### Docker Start

```bash
# In-memory storage (fast, not persistent)
docker run -p 9324:9324 -p 8085:8085 -p 9000:9000 lclq/lclq:latest

# SQLite storage (persistent)
docker run -p 9324:9324 -p 8085:8085 -p 9000:9000 \
  -v $(pwd)/data:/data \
  lclq/lclq:latest --storage-type sqlite --sqlite-path /data/lclq.db
```

### Verify Server is Running

```bash
# Check health
curl http://localhost:9000/health

# List queues (should return empty initially)
curl http://localhost:9000/queues
```

---

## AWS SQS Quick Start

### Python (boto3)

**1. Install Dependencies**

```bash
pip install boto3
```

**2. Basic Producer/Consumer Example**

```python
import boto3
import json

# Create SQS client pointing to lclq
sqs = boto3.client(
    'sqs',
    endpoint_url='http://localhost:9324',
    region_name='us-east-1',
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

# Create a queue
response = sqs.create_queue(QueueName='my-queue')
queue_url = response['QueueUrl']
print(f"Created queue: {queue_url}")

# Send a message
sqs.send_message(
    QueueUrl=queue_url,
    MessageBody='Hello from lclq!',
    MessageAttributes={
        'Author': {
            'StringValue': 'quickstart',
            'DataType': 'String'
        }
    }
)
print("Message sent!")

# Receive messages
response = sqs.receive_message(
    QueueUrl=queue_url,
    MaxNumberOfMessages=10,
    MessageAttributeNames=['All']
)

if 'Messages' in response:
    for message in response['Messages']:
        print(f"Received: {message['Body']}")
        print(f"Attributes: {message.get('MessageAttributes', {})}")

        # Delete the message after processing
        sqs.delete_message(
            QueueUrl=queue_url,
            ReceiptHandle=message['ReceiptHandle']
        )
        print("Message deleted")
else:
    print("No messages available")

# Cleanup
sqs.delete_queue(QueueUrl=queue_url)
print("Queue deleted")
```

**3. FIFO Queue Example**

```python
import boto3

sqs = boto3.client(
    'sqs',
    endpoint_url='http://localhost:9324',
    region_name='us-east-1',
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

# Create FIFO queue
response = sqs.create_queue(
    QueueName='my-queue.fifo',
    Attributes={
        'FifoQueue': 'true',
        'ContentBasedDeduplication': 'true'
    }
)
queue_url = response['QueueUrl']

# Send ordered messages
for i in range(5):
    sqs.send_message(
        QueueUrl=queue_url,
        MessageBody=f'Message {i}',
        MessageGroupId='group1'  # Messages with same group ID are ordered
    )

# Receive messages (will be in order)
response = sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=10)
for msg in response.get('Messages', []):
    print(f"Received in order: {msg['Body']}")
```

### JavaScript (AWS SDK v3)

**1. Install Dependencies**

```bash
npm install @aws-sdk/client-sqs
```

**2. Basic Producer/Consumer Example**

```javascript
import {
  SQSClient,
  CreateQueueCommand,
  SendMessageCommand,
  ReceiveMessageCommand,
  DeleteMessageCommand,
  DeleteQueueCommand
} from "@aws-sdk/client-sqs";

// Create SQS client pointing to lclq
const client = new SQSClient({
  region: "us-east-1",
  endpoint: "http://localhost:9324",
  credentials: {
    accessKeyId: "dummy",
    secretAccessKey: "dummy"
  }
});

async function main() {
  // Create queue
  const createResponse = await client.send(
    new CreateQueueCommand({ QueueName: "my-queue" })
  );
  const queueUrl = createResponse.QueueUrl;
  console.log(`Created queue: ${queueUrl}`);

  // Send message
  await client.send(
    new SendMessageCommand({
      QueueUrl: queueUrl,
      MessageBody: "Hello from lclq!",
      MessageAttributes: {
        Author: {
          StringValue: "quickstart",
          DataType: "String"
        }
      }
    })
  );
  console.log("Message sent!");

  // Receive messages
  const receiveResponse = await client.send(
    new ReceiveMessageCommand({
      QueueUrl: queueUrl,
      MaxNumberOfMessages: 10,
      MessageAttributeNames: ["All"]
    })
  );

  if (receiveResponse.Messages) {
    for (const message of receiveResponse.Messages) {
      console.log(`Received: ${message.Body}`);
      console.log(`Attributes: ${JSON.stringify(message.MessageAttributes)}`);

      // Delete message after processing
      await client.send(
        new DeleteMessageCommand({
          QueueUrl: queueUrl,
          ReceiptHandle: message.ReceiptHandle
        })
      );
      console.log("Message deleted");
    }
  } else {
    console.log("No messages available");
  }

  // Cleanup
  await client.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
  console.log("Queue deleted");
}

main().catch(console.error);
```

**3. Batch Operations Example**

```javascript
import {
  SQSClient,
  CreateQueueCommand,
  SendMessageBatchCommand,
  ReceiveMessageCommand,
  DeleteMessageBatchCommand
} from "@aws-sdk/client-sqs";

const client = new SQSClient({
  region: "us-east-1",
  endpoint: "http://localhost:9324",
  credentials: { accessKeyId: "dummy", secretAccessKey: "dummy" }
});

async function batchExample() {
  // Create queue
  const { QueueUrl } = await client.send(
    new CreateQueueCommand({ QueueName: "batch-queue" })
  );

  // Send batch of messages
  await client.send(
    new SendMessageBatchCommand({
      QueueUrl,
      Entries: [
        { Id: "1", MessageBody: "Message 1" },
        { Id: "2", MessageBody: "Message 2" },
        { Id: "3", MessageBody: "Message 3" }
      ]
    })
  );
  console.log("Batch sent!");

  // Receive messages
  const { Messages } = await client.send(
    new ReceiveMessageCommand({ QueueUrl, MaxNumberOfMessages: 10 })
  );

  if (Messages) {
    // Delete batch of messages
    await client.send(
      new DeleteMessageBatchCommand({
        QueueUrl,
        Entries: Messages.map((msg, idx) => ({
          Id: idx.toString(),
          ReceiptHandle: msg.ReceiptHandle
        }))
      })
    );
    console.log("Batch deleted!");
  }
}

batchExample().catch(console.error);
```

### Go (AWS SDK v2)

**1. Install Dependencies**

```bash
go get github.com/aws/aws-sdk-go-v2/config
go get github.com/aws/aws-sdk-go-v2/service/sqs
```

**2. Basic Producer/Consumer Example**

```go
package main

import (
    "context"
    "fmt"
    "log"

    "github.com/aws/aws-sdk-go-v2/aws"
    "github.com/aws/aws-sdk-go-v2/config"
    "github.com/aws/aws-sdk-go-v2/service/sqs"
    "github.com/aws/aws-sdk-go-v2/service/sqs/types"
)

func main() {
    ctx := context.Background()

    // Load config with custom endpoint
    cfg, err := config.LoadDefaultConfig(ctx,
        config.WithEndpointResolverWithOptions(
            aws.EndpointResolverWithOptionsFunc(
                func(service, region string, options ...interface{}) (aws.Endpoint, error) {
                    return aws.Endpoint{URL: "http://localhost:9324"}, nil
                },
            ),
        ),
    )
    if err != nil {
        log.Fatal(err)
    }

    client := sqs.NewFromConfig(cfg)

    // Create queue
    createResp, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
        QueueName: aws.String("my-queue"),
    })
    if err != nil {
        log.Fatal(err)
    }
    queueURL := *createResp.QueueUrl
    fmt.Printf("Created queue: %s\n", queueURL)

    // Send message
    _, err = client.SendMessage(ctx, &sqs.SendMessageInput{
        QueueUrl:    &queueURL,
        MessageBody: aws.String("Hello from lclq!"),
        MessageAttributes: map[string]types.MessageAttributeValue{
            "Author": {
                StringValue: aws.String("quickstart"),
                DataType:    aws.String("String"),
            },
        },
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println("Message sent!")

    // Receive messages
    receiveResp, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
        QueueUrl:            &queueURL,
        MaxNumberOfMessages: 10,
        MessageAttributeNames: []string{"All"},
    })
    if err != nil {
        log.Fatal(err)
    }

    if len(receiveResp.Messages) > 0 {
        for _, msg := range receiveResp.Messages {
            fmt.Printf("Received: %s\n", *msg.Body)

            // Delete message after processing
            _, err = client.DeleteMessage(ctx, &sqs.DeleteMessageInput{
                QueueUrl:      &queueURL,
                ReceiptHandle: msg.ReceiptHandle,
            })
            if err != nil {
                log.Fatal(err)
            }
            fmt.Println("Message deleted")
        }
    } else {
        fmt.Println("No messages available")
    }

    // Cleanup
    _, err = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{
        QueueUrl: &queueURL,
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println("Queue deleted")
}
```

### Rust (AWS SDK)

**1. Add Dependencies to Cargo.toml**

```toml
[dependencies]
aws-config = "1.0"
aws-sdk-sqs = "1.0"
tokio = { version = "1", features = ["full"] }
```

**2. Basic Producer/Consumer Example**

```rust
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_sqs::{Client, config::Region};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure client with custom endpoint
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env()
        .region(region_provider)
        .endpoint_url("http://localhost:9324")
        .load()
        .await;

    let client = Client::new(&config);

    // Create queue
    let create_resp = client
        .create_queue()
        .queue_name("my-queue")
        .send()
        .await?;

    let queue_url = create_resp.queue_url.unwrap();
    println!("Created queue: {}", queue_url);

    // Send message
    client
        .send_message()
        .queue_url(&queue_url)
        .message_body("Hello from lclq!")
        .message_attributes(
            "Author",
            aws_sdk_sqs::types::MessageAttributeValue::builder()
                .string_value("quickstart")
                .data_type("String")
                .build()?,
        )
        .send()
        .await?;

    println!("Message sent!");

    // Receive messages
    let receive_resp = client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .message_attribute_names("All")
        .send()
        .await?;

    if let Some(messages) = receive_resp.messages {
        for message in messages {
            println!("Received: {}", message.body.unwrap_or_default());

            // Delete message after processing
            if let Some(receipt_handle) = message.receipt_handle {
                client
                    .delete_message()
                    .queue_url(&queue_url)
                    .receipt_handle(receipt_handle)
                    .send()
                    .await?;
                println!("Message deleted");
            }
        }
    } else {
        println!("No messages available");
    }

    // Cleanup
    client.delete_queue().queue_url(&queue_url).send().await?;
    println!("Queue deleted");

    Ok(())
}
```

---

## GCP Pub/Sub Quick Start

### Python (google-cloud-pubsub)

**1. Install Dependencies**

```bash
pip install google-cloud-pubsub
```

**2. Basic Publisher/Subscriber Example**

```python
import os
from google.cloud import pubsub_v1

# Point to lclq instead of GCP
os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'

# Create clients
publisher = pubsub_v1.PublisherClient()
subscriber = pubsub_v1.SubscriberClient()

# Define resource names
project_id = 'my-project'
topic_id = 'my-topic'
subscription_id = 'my-subscription'

topic_path = publisher.topic_path(project_id, topic_id)
subscription_path = subscriber.subscription_path(project_id, subscription_id)

# Create topic
topic = publisher.create_topic(request={"name": topic_path})
print(f"Created topic: {topic.name}")

# Create subscription
subscription = subscriber.create_subscription(
    request={"name": subscription_path, "topic": topic_path}
)
print(f"Created subscription: {subscription.name}")

# Publish messages
for i in range(5):
    data = f"Message {i}".encode("utf-8")
    future = publisher.publish(topic_path, data, origin="quickstart")
    message_id = future.result()
    print(f"Published message {i} with ID: {message_id}")

# Pull messages
response = subscriber.pull(
    request={"subscription": subscription_path, "max_messages": 10}
)

print(f"Received {len(response.received_messages)} messages:")
for received_message in response.received_messages:
    print(f"  - {received_message.message.data.decode('utf-8')}")
    print(f"    Attributes: {received_message.message.attributes}")

# Acknowledge messages
ack_ids = [msg.ack_id for msg in response.received_messages]
subscriber.acknowledge(request={"subscription": subscription_path, "ack_ids": ack_ids})
print("Messages acknowledged")

# Cleanup
subscriber.delete_subscription(request={"subscription": subscription_path})
publisher.delete_topic(request={"topic": topic_path})
print("Resources deleted")
```

**3. Message Ordering Example**

```python
import os
from google.cloud import pubsub_v1

os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'

# Create publisher with message ordering enabled
publisher = pubsub_v1.PublisherClient(
    publisher_options=pubsub_v1.types.PublisherOptions(enable_message_ordering=True)
)
subscriber = pubsub_v1.SubscriberClient()

project_id = 'my-project'
topic_path = publisher.topic_path(project_id, 'ordered-topic')
subscription_path = subscriber.subscription_path(project_id, 'ordered-sub')

# Create topic and subscription
publisher.create_topic(request={"name": topic_path})
subscriber.create_subscription(
    request={
        "name": subscription_path,
        "topic": topic_path,
        "enable_message_ordering": True
    }
)

# Publish ordered messages
ordering_key = "order-1"
for i in range(5):
    future = publisher.publish(
        topic_path,
        f"Ordered message {i}".encode("utf-8"),
        ordering_key=ordering_key
    )
    print(f"Published ordered message {i}")

# Pull messages (will be in order)
response = subscriber.pull(
    request={"subscription": subscription_path, "max_messages": 10}
)

print("Received messages in order:")
for msg in response.received_messages:
    print(f"  - {msg.message.data.decode('utf-8')}")
```

### JavaScript (@google-cloud/pubsub)

**1. Install Dependencies**

```bash
npm install @google-cloud/pubsub
```

**2. Basic Publisher/Subscriber Example**

```javascript
// Set emulator host before importing
process.env.PUBSUB_EMULATOR_HOST = 'localhost:8085';

import { PubSub } from '@google-cloud/pubsub';

async function main() {
  // Create PubSub client
  const pubsub = new PubSub({ projectId: 'my-project' });

  // Create topic
  const [topic] = await pubsub.createTopic('my-topic');
  console.log(`Created topic: ${topic.name}`);

  // Create subscription
  const [subscription] = await topic.createSubscription('my-subscription');
  console.log(`Created subscription: ${subscription.name}`);

  // Publish messages
  for (let i = 0; i < 5; i++) {
    const messageId = await topic.publishMessage({
      data: Buffer.from(`Message ${i}`),
      attributes: { origin: 'quickstart' }
    });
    console.log(`Published message ${i} with ID: ${messageId}`);
  }

  // Pull messages using v1 client (for synchronous pull)
  const { v1 } = await import('@google-cloud/pubsub');
  const grpc = await import('@grpc/grpc-js');

  const subscriberClient = new v1.SubscriberClient({
    servicePath: 'localhost',
    port: 8085,
    sslCreds: grpc.credentials.createInsecure(),
  });

  const subscriptionPath = subscriberClient.subscriptionPath('my-project', 'my-subscription');
  const [response] = await subscriberClient.pull({
    subscription: subscriptionPath,
    maxMessages: 10,
  });

  console.log(`Received ${response.receivedMessages.length} messages:`);
  for (const message of response.receivedMessages) {
    console.log(`  - ${message.message.data.toString()}`);
    console.log(`    Attributes: ${JSON.stringify(message.message.attributes)}`);
  }

  // Acknowledge messages
  const ackIds = response.receivedMessages.map(msg => msg.ackId);
  await subscriberClient.acknowledge({
    subscription: subscriptionPath,
    ackIds: ackIds,
  });
  console.log('Messages acknowledged');

  // Cleanup
  await subscription.delete();
  await topic.delete();
  console.log('Resources deleted');
}

main().catch(console.error);
```

### Go (cloud.google.com/go/pubsub)

**1. Install Dependencies**

```bash
go get cloud.google.com/go/pubsub
```

**2. Basic Publisher/Subscriber Example**

```go
package main

import (
    "context"
    "fmt"
    "log"
    "os"

    "cloud.google.com/go/pubsub"
)

func main() {
    // Set emulator host
    os.Setenv("PUBSUB_EMULATOR_HOST", "localhost:8085")

    ctx := context.Background()
    projectID := "my-project"

    // Create client
    client, err := pubsub.NewClient(ctx, projectID)
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Create topic
    topic, err := client.CreateTopic(ctx, "my-topic")
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Created topic: %s\n", topic.ID())

    // Create subscription
    sub, err := client.CreateSubscription(ctx, "my-subscription", pubsub.SubscriptionConfig{
        Topic: topic,
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Created subscription: %s\n", sub.ID())

    // Publish messages
    for i := 0; i < 5; i++ {
        result := topic.Publish(ctx, &pubsub.Message{
            Data: []byte(fmt.Sprintf("Message %d", i)),
            Attributes: map[string]string{
                "origin": "quickstart",
            },
        })
        msgID, err := result.Get(ctx)
        if err != nil {
            log.Fatal(err)
        }
        fmt.Printf("Published message %d with ID: %s\n", i, msgID)
    }

    // Pull messages (synchronous)
    err = sub.Receive(ctx, func(ctx context.Context, msg *pubsub.Message) {
        fmt.Printf("Received: %s\n", string(msg.Data))
        fmt.Printf("  Attributes: %v\n", msg.Attributes)
        msg.Ack() // Acknowledge the message
    })
    if err != nil {
        log.Fatal(err)
    }

    // Cleanup
    if err := sub.Delete(ctx); err != nil {
        log.Fatal(err)
    }
    if err := topic.Delete(ctx); err != nil {
        log.Fatal(err)
    }
    fmt.Println("Resources deleted")
}
```

---

## Common Patterns

### Dead Letter Queue (SQS)

```python
import boto3

sqs = boto3.client(
    'sqs',
    endpoint_url='http://localhost:9324',
    region_name='us-east-1',
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

# Create DLQ
dlq_response = sqs.create_queue(QueueName='my-dlq')
dlq_url = dlq_response['QueueUrl']
dlq_arn = f"arn:aws:sqs:us-east-1:000000000000:my-dlq"

# Create main queue with DLQ configured
queue_response = sqs.create_queue(
    QueueName='my-main-queue',
    Attributes={
        'RedrivePolicy': json.dumps({
            'deadLetterTargetArn': dlq_arn,
            'maxReceiveCount': '3'  # Move to DLQ after 3 failed attempts
        })
    }
)
main_queue_url = queue_response['QueueUrl']

# Send a message
sqs.send_message(QueueUrl=main_queue_url, MessageBody='Test message')

# Receive and NOT delete (simulate failure)
for _ in range(3):
    response = sqs.receive_message(QueueUrl=main_queue_url, VisibilityTimeout=1)
    # Don't acknowledge - message will become visible again

# After 3 receives, message moves to DLQ
import time
time.sleep(2)

dlq_messages = sqs.receive_message(QueueUrl=dlq_url)
print(f"Messages in DLQ: {len(dlq_messages.get('Messages', []))}")
```

### Long Polling (SQS)

```python
import boto3

sqs = boto3.client(
    'sqs',
    endpoint_url='http://localhost:9324',
    region_name='us-east-1',
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

queue_url = sqs.create_queue(QueueName='poll-queue')['QueueUrl']

# Long poll for up to 20 seconds
print("Waiting for messages...")
response = sqs.receive_message(
    QueueUrl=queue_url,
    WaitTimeSeconds=20,  # Wait up to 20 seconds for messages
    MaxNumberOfMessages=10
)

if 'Messages' in response:
    print(f"Received {len(response['Messages'])} messages")
else:
    print("No messages received within 20 seconds")
```

### Dead Letter Topic (Pub/Sub)

```python
import os
from google.cloud import pubsub_v1

os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'

publisher = pubsub_v1.PublisherClient()
subscriber = pubsub_v1.SubscriberClient()

project_id = 'my-project'

# Create DLQ topic
dlq_topic_path = publisher.topic_path(project_id, 'my-dlq-topic')
publisher.create_topic(request={"name": dlq_topic_path})

# Create main topic and subscription with DLQ
main_topic_path = publisher.topic_path(project_id, 'main-topic')
publisher.create_topic(request={"name": main_topic_path})

subscription_path = subscriber.subscription_path(project_id, 'main-sub')
subscriber.create_subscription(
    request={
        "name": subscription_path,
        "topic": main_topic_path,
        "dead_letter_policy": {
            "dead_letter_topic": dlq_topic_path,
            "max_delivery_attempts": 5
        }
    }
)

print("Subscription created with DLQ policy")
```

---

## Troubleshooting

### Server Won't Start

**Issue**: Port already in use

```bash
# Check what's using the ports
lsof -i :9324
lsof -i :8085

# Kill the process or use different ports
lclq start --sqs-port 9325 --pubsub-port 8086
```

**Issue**: Permission denied

```bash
# Run on unprivileged ports (>1024) or use sudo
sudo lclq start
```

### Cannot Connect from SDK

**SQS Issue**: Endpoint not recognized

```python
# Make sure endpoint_url includes http://
sqs = boto3.client('sqs', endpoint_url='http://localhost:9324')  # Correct
sqs = boto3.client('sqs', endpoint_url='localhost:9324')  # Wrong
```

**Pub/Sub Issue**: Cannot find emulator

```python
# Make sure environment variable is set BEFORE importing
import os
os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'  # Must be first
from google.cloud import pubsub_v1  # Then import
```

### Messages Not Appearing

**Check visibility timeout**: Messages may be invisible due to active timeout

```bash
# Use Admin API to check queue stats
curl http://localhost:9000/queues/my-queue
```

**Check message retention**: Messages may have expired

```python
# SQS: Set longer retention (default is 4 days)
sqs.set_queue_attributes(
    QueueUrl=queue_url,
    Attributes={'MessageRetentionPeriod': '1209600'}  # 14 days
)
```

### Performance Issues

**Use in-memory backend for maximum speed**:

```bash
lclq start --storage-type memory
```

**Increase batch sizes**:

```python
# Send/receive in batches for better throughput
sqs.send_message_batch(...)  # Up to 10 messages
sqs.receive_message(MaxNumberOfMessages=10)  # Receive up to 10
```

### Docker Networking

**Cannot connect from host**:

```bash
# Make sure ports are exposed
docker run -p 9324:9324 -p 8085:8085 lclq/lclq:latest
```

**Cannot connect from another container**:

```yaml
# Use Docker Compose networking
services:
  lclq:
    image: lclq/lclq:latest
  app:
    environment:
      - AWS_ENDPOINT_URL=http://lclq:9324  # Use service name
```

---

## Next Steps

- **Configuration Guide**: Learn about all configuration options in [configuration.md](configuration.md)
- **API Reference**: Complete API documentation in [api-reference.md](api-reference.md)
- **Architecture**: Understand how lclq works internally in [architecture.md](architecture.md)
- **Benchmarks**: See detailed performance analysis in [benchmarks.md](benchmarks.md)

---

**Need help?** Open an issue at https://github.com/yourusername/lclq/issues
