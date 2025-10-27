# Technical Product Requirements Document: lclq

**Version:** 1.0
**Date:** October 2025
**Status:** Draft
**Target Language:** Rust 1.90

## Executive Summary

This document provides technical specifications for implementing lclq, a local queue system compatible with AWS SQS and GCP Pub/Sub. The implementation will use Rust 1.90 for performance, safety, and concurrency, with strict adherence to cloud service constraints and SDK compatibility.

## 1. Technology Stack

### 1.1 Core Technologies

**Language and Runtime:**
- **Rust:** 1.90.0 (stable)
- **Edition:** 2024
- **Target Platforms:** Linux (x86_64, ARM64), macOS (x86_64, ARM64), Windows (x86_64)

**Key Dependencies:**
```toml
[dependencies]
# HTTP/REST Server
axum = "0.7"                    # Modern async web framework
tower = "0.5"                    # Service middleware
tower-http = "0.6"              # HTTP middleware (compression, CORS)
hyper = "1.5"                    # HTTP implementation

# gRPC Server
tonic = "0.12"                   # gRPC framework
prost = "0.13"                   # Protocol Buffers
tonic-reflection = "0.12"        # gRPC reflection

# Async Runtime
tokio = { version = "1.40", features = ["full"] }
tokio-util = "0.7"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
quick-xml = { version = "0.36", features = ["serialize"] }
prost-types = "0.13"

# Storage
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
rusqlite = { version = "0.32", features = ["bundled"] }

# Utilities
uuid = { version = "1.10", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Cryptography (for signature verification)
sha2 = "0.10"
hmac = "0.12"
base64 = "0.22"

# Configuration
config = "0.14"
clap = { version = "4.5", features = ["derive"] }

# Metrics
prometheus = "0.13"

[dev-dependencies]
aws-sdk-sqs = "1.48"            # For compatibility testing
google-cloud-pubsub = "0.27"     # For compatibility testing
criterion = "0.5"                # Benchmarking
```

### 1.2 Build and Tooling

**Build System:**
- Cargo 1.90+
- cargo-make for task automation
- cross for cross-compilation

**Development Tools:**
- rustfmt (code formatting)
- clippy (linting)
- cargo-audit (security auditing)
- cargo-tarpaulin (code coverage)

**CI/CD:**
- GitHub Actions
- Docker multi-stage builds
- Release binaries for all platforms

## 2. AWS SQS Technical Specifications

### 2.1 Service Constraints

**Queue Naming:**
```rust
// Queue name validation
const QUEUE_NAME_MIN_LENGTH: usize = 1;
const QUEUE_NAME_MAX_LENGTH: usize = 80;
const FIFO_SUFFIX: &str = ".fifo";

// Valid characters: alphanumeric, hyphen, underscore
fn validate_queue_name(name: &str) -> Result<(), ValidationError> {
    let regex = Regex::new(r"^[a-zA-Z0-9_-]{1,80}$").unwrap();

    if name.ends_with(FIFO_SUFFIX) {
        // FIFO queue names must be 75 chars or less (excluding .fifo)
        let base_name = &name[..name.len() - 5];
        if base_name.len() < 1 || base_name.len() > 75 {
            return Err(ValidationError::InvalidQueueName);
        }
    }

    if !regex.is_match(name) {
        return Err(ValidationError::InvalidQueueName);
    }

    Ok(())
}
```

**Message Constraints:**
- **Size:** Maximum 256 KB (262,144 bytes)
- **Retention:** 60 seconds to 1,209,600 seconds (14 days), default 345,600 (4 days)
- **Visibility Timeout:** 0 to 43,200 seconds (12 hours), default 30 seconds
- **Delay:** 0 to 900 seconds (15 minutes)
- **Batch Size:** 1-10 messages
- **Message Attributes:** Maximum 10 per message
- **Attribute Name:** 1-256 characters (alphanumeric, hyphen, underscore, period)
- **Attribute Value:** Maximum 256 KB (including name, type, value)

**FIFO-Specific Constraints:**
- **Throughput:** 300 transactions/second (standard), 3,000 with batching
- **Message Group ID:** Maximum 128 characters
- **Deduplication ID:** Maximum 128 characters
- **Deduplication Window:** 5 minutes
- **Content-based Deduplication:** SHA-256 hash of message body

**Queue URL Format:**
```
http://localhost:4566/000000000000/{QueueName}
http://localhost:4566/{account-id}/{QueueName}
```

### 2.2 API Implementation

**HTTP Protocol:**
- HTTP/1.1 and HTTP/2 support
- POST requests with form-encoded bodies
- XML response format (SQS uses Query Protocol)
- AWS Signature Version 4 (optional validation)

**Required Actions:**
```rust
pub enum SqsAction {
    // Queue Management
    CreateQueue,
    DeleteQueue,
    GetQueueUrl,
    GetQueueAttributes,
    SetQueueAttributes,
    ListQueues,
    ListQueueTags,
    TagQueue,
    UntagQueue,
    PurgeQueue,

    // Message Operations
    SendMessage,
    SendMessageBatch,
    ReceiveMessage,
    DeleteMessage,
    DeleteMessageBatch,
    ChangeMessageVisibility,
    ChangeMessageVisibilityBatch,
}
```

**SQS Attributes:**
```rust
pub enum QueueAttribute {
    // Timing
    DelaySeconds,                    // 0-900
    MessageRetentionPeriod,          // 60-1209600
    ReceiveMessageWaitTimeSeconds,   // 0-20 (long polling)
    VisibilityTimeout,               // 0-43200

    // FIFO
    FifoQueue,                       // true/false
    ContentBasedDeduplication,       // true/false
    DeduplicationScope,              // messageGroup | queue
    FifoThroughputLimit,             // perQueue | perMessageGroupId

    // Dead Letter Queue
    RedrivePolicy,                   // JSON: { deadLetterTargetArn, maxReceiveCount }
    RedriveAllowPolicy,              // JSON: allow/deny policy

    // Size and Counts
    ApproximateNumberOfMessages,
    ApproximateNumberOfMessagesNotVisible,
    ApproximateNumberOfMessagesDelayed,
    MaximumMessageSize,              // 1024-262144

    // Metadata
    CreatedTimestamp,
    LastModifiedTimestamp,
    QueueArn,
}
```

**Message Structure:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsMessage {
    pub message_id: String,              // UUID
    pub receipt_handle: String,          // Opaque token for deletion
    pub md5_of_body: String,             // MD5 hash
    pub body: String,                    // Max 256 KB
    pub attributes: HashMap<String, String>,
    pub message_attributes: HashMap<String, MessageAttributeValue>,
    pub md5_of_message_attributes: Option<String>,

    // FIFO-specific
    pub sequence_number: Option<String>,
    pub message_deduplication_id: Option<String>,
    pub message_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttributeValue {
    pub data_type: String,               // String, Number, Binary, String.Array, Number.Array
    pub string_value: Option<String>,
    pub binary_value: Option<Vec<u8>>,
}
```

**Receipt Handle:**
```rust
// Receipt handle encodes: message_id, receipt_time, queue_id
// Format: base64(message_id:timestamp:queue_id:random)
fn generate_receipt_handle(msg_id: &str, queue_id: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let random = Uuid::new_v4();
    let handle = format!("{}:{}:{}:{}", msg_id, timestamp, queue_id, random);
    STANDARD.encode(handle.as_bytes())
}
```

### 2.3 Error Codes

```rust
pub enum SqsError {
    // Client Errors (4xx)
    InvalidParameterValue,
    InvalidAttributeName,
    InvalidMessageContents,
    MessageTooLong,
    BatchRequestTooLong,
    EmptyBatchRequest,
    InvalidBatchEntryId,
    TooManyEntriesInBatchRequest,
    BatchEntryIdsNotDistinct,
    ReceiptHandleIsInvalid,
    QueueDoesNotExist,
    QueueDeletedRecently,
    QueueNameExists,
    InvalidAttributeValue,
    InvalidIdFormat,
    UnsupportedOperation,

    // Server Errors (5xx)
    InternalError,
    ServiceUnavailable,
}
```

### 2.4 SDK Compatibility Matrix

**Target SDKs:**
- **AWS SDK for Python (boto3):** 1.35+
- **AWS SDK for JavaScript (v3):** 3.600+
- **AWS SDK for Go (v2):** 1.30+
- **AWS SDK for Ruby (v3):** 1.200+
- **AWS SDK for Java (v2):** 2.26+
- **AWS SDK for .NET:** 3.7+
- **AWS SDK for Rust:** 1.48+

**Endpoint Configuration:**
```python
# Python (boto3)
sqs = boto3.client('sqs', endpoint_url='http://localhost:4566')

# JavaScript (v3)
const client = new SQSClient({ endpoint: "http://localhost:4566" });

# Go (v2)
cfg, _ := config.LoadDefaultConfig(context.TODO(),
    config.WithEndpointResolverWithOptions(aws.EndpointResolverWithOptionsFunc(
        func(service, region string, options ...interface{}) (aws.Endpoint, error) {
            return aws.Endpoint{URL: "http://localhost:4566"}, nil
        })),
)
```

## 3. GCP Pub/Sub Technical Specifications

### 3.1 Service Constraints

**Topic Naming:**
```rust
// Format: projects/{project}/topics/{topic-id}
// topic-id constraints:
const TOPIC_ID_MIN_LENGTH: usize = 3;
const TOPIC_ID_MAX_LENGTH: usize = 255;

fn validate_topic_id(topic_id: &str) -> Result<(), ValidationError> {
    // Must start with a letter
    // Can contain: letters, numbers, dashes, underscores, tildes, percentages, plus
    let regex = Regex::new(r"^[a-zA-Z][a-zA-Z0-9._~%+-]{2,254}$").unwrap();

    if !regex.is_match(topic_id) {
        return Err(ValidationError::InvalidTopicId);
    }

    // Reserved prefixes
    if topic_id.starts_with("goog") {
        return Err(ValidationError::ReservedPrefix);
    }

    Ok(())
}
```

**Subscription Naming:**
```rust
// Format: projects/{project}/subscriptions/{subscription-id}
// Same constraints as topic-id
const SUBSCRIPTION_ID_MIN_LENGTH: usize = 3;
const SUBSCRIPTION_ID_MAX_LENGTH: usize = 255;
```

**Message Constraints:**
- **Size:** Maximum 10 MB (10,485,760 bytes)
- **Retention:** 10 minutes to 7 days, default 7 days
- **Ack Deadline:** 10 to 600 seconds, default 10 seconds
- **Ordering Key:** Maximum 1024 bytes
- **Attributes:** Maximum 100 attributes
- **Attribute Key:** Maximum 256 bytes
- **Attribute Value:** Maximum 1024 bytes
- **Message ID:** Server-assigned, unique

**Subscription Constraints:**
- **Push Endpoint:** Valid HTTPS URL
- **Max Delivery Attempts:** 5-100 for dead letter topics
- **Filter Syntax:** CEL (Common Expression Language)

**Resource Name Formats:**
```
projects/{project}/topics/{topic}
projects/{project}/subscriptions/{subscription}
projects/{project}/snapshots/{snapshot}
```

### 3.2 Protocol Implementation

#### 3.2.1 gRPC Interface

**Proto Definition:**
```protobuf
syntax = "proto3";

package google.pubsub.v1;

service Publisher {
  rpc CreateTopic(Topic) returns (Topic);
  rpc UpdateTopic(UpdateTopicRequest) returns (Topic);
  rpc Publish(PublishRequest) returns (PublishResponse);
  rpc GetTopic(GetTopicRequest) returns (Topic);
  rpc ListTopics(ListTopicsRequest) returns (ListTopicsResponse);
  rpc ListTopicSubscriptions(ListTopicSubscriptionsRequest) returns (ListTopicSubscriptionsResponse);
  rpc DeleteTopic(DeleteTopicRequest) returns (google.protobuf.Empty);
  rpc DetachSubscription(DetachSubscriptionRequest) returns (DetachSubscriptionResponse);
}

service Subscriber {
  rpc CreateSubscription(Subscription) returns (Subscription);
  rpc GetSubscription(GetSubscriptionRequest) returns (Subscription);
  rpc UpdateSubscription(UpdateSubscriptionRequest) returns (Subscription);
  rpc ListSubscriptions(ListSubscriptionsRequest) returns (ListSubscriptionsResponse);
  rpc DeleteSubscription(DeleteSubscriptionRequest) returns (google.protobuf.Empty);
  rpc ModifyAckDeadline(ModifyAckDeadlineRequest) returns (google.protobuf.Empty);
  rpc Acknowledge(AcknowledgeRequest) returns (google.protobuf.Empty);
  rpc Pull(PullRequest) returns (PullResponse);
  rpc StreamingPull(stream StreamingPullRequest) returns (stream StreamingPullResponse);
  rpc ModifyPushConfig(ModifyPushConfigRequest) returns (google.protobuf.Empty);
  rpc Seek(SeekRequest) returns (SeekResponse);
}

message PubsubMessage {
  string data = 1;                          // Base64-encoded payload
  map<string, string> attributes = 2;       // Max 100 attributes
  string message_id = 3;                    // Server-assigned
  google.protobuf.Timestamp publish_time = 4;
  string ordering_key = 5;                  // Max 1024 bytes
}

message Topic {
  string name = 1;                          // projects/{project}/topics/{topic}
  map<string, string> labels = 2;
  MessageStoragePolicy message_storage_policy = 3;
  string kms_key_name = 4;
  SchemaSettings schema_settings = 5;
  bool satisfies_pzs = 6;
  google.protobuf.Duration message_retention_duration = 7;
}

message Subscription {
  string name = 1;                          // projects/{project}/subscriptions/{subscription}
  string topic = 2;
  PushConfig push_config = 3;
  int32 ack_deadline_seconds = 4;          // 10-600
  bool retain_acked_messages = 5;
  google.protobuf.Duration message_retention_duration = 6;
  map<string, string> labels = 7;
  bool enable_message_ordering = 8;
  ExpirationPolicy expiration_policy = 9;
  string filter = 10;                      // CEL expression
  DeadLetterPolicy dead_letter_policy = 11;
  RetryPolicy retry_policy = 12;
  bool detached = 13;
}
```

**Rust Implementation:**
```rust
use tonic::{transport::Server, Request, Response, Status};
use pubsub_proto::publisher_server::{Publisher, PublisherServer};
use pubsub_proto::subscriber_server::{Subscriber, SubscriberServer};

pub struct PubSubPublisher {
    core: Arc<QueueCore>,
}

#[tonic::async_trait]
impl Publisher for PubSubPublisher {
    async fn create_topic(
        &self,
        request: Request<Topic>,
    ) -> Result<Response<Topic>, Status> {
        let topic = request.into_inner();

        // Validate topic name
        validate_topic_name(&topic.name)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // Create topic in core engine
        self.core.create_topic(topic.clone()).await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(topic))
    }

    async fn publish(
        &self,
        request: Request<PublishRequest>,
    ) -> Result<Response<PublishResponse>, Status> {
        let req = request.into_inner();

        // Validate message size
        for msg in &req.messages {
            if msg.data.len() > MAX_MESSAGE_SIZE {
                return Err(Status::invalid_argument("Message too large"));
            }
        }

        // Publish messages
        let message_ids = self.core.publish(req.topic, req.messages).await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(PublishResponse { message_ids }))
    }
}
```

#### 3.2.2 HTTP/REST Interface

**Base URL:**
```
http://localhost:8085/v1
```

**Endpoints:**
```rust
// Topics
POST   /v1/projects/{project}/topics/{topic}
GET    /v1/projects/{project}/topics/{topic}
DELETE /v1/projects/{project}/topics/{topic}
GET    /v1/projects/{project}/topics
POST   /v1/projects/{project}/topics/{topic}:publish

// Subscriptions
PUT    /v1/projects/{project}/subscriptions/{subscription}
GET    /v1/projects/{project}/subscriptions/{subscription}
DELETE /v1/projects/{project}/subscriptions/{subscription}
GET    /v1/projects/{project}/subscriptions
POST   /v1/projects/{project}/subscriptions/{subscription}:pull
POST   /v1/projects/{project}/subscriptions/{subscription}:acknowledge
POST   /v1/projects/{project}/subscriptions/{subscription}:modifyAckDeadline
POST   /v1/projects/{project}/subscriptions/{subscription}:modifyPushConfig
POST   /v1/projects/{project}/subscriptions/{subscription}:seek
```

**REST Implementation:**
```rust
use axum::{
    routing::{get, post, delete, put},
    Router, Json, extract::{Path, State},
};

pub fn pubsub_rest_routes(core: Arc<QueueCore>) -> Router {
    Router::new()
        // Topic routes
        .route(
            "/v1/projects/:project/topics/:topic",
            put(create_topic).get(get_topic).delete(delete_topic)
        )
        .route(
            "/v1/projects/:project/topics",
            get(list_topics)
        )
        .route(
            "/v1/projects/:project/topics/:topic:publish",
            post(publish_messages)
        )
        // Subscription routes
        .route(
            "/v1/projects/:project/subscriptions/:subscription",
            put(create_subscription).get(get_subscription).delete(delete_subscription)
        )
        .route(
            "/v1/projects/:project/subscriptions/:subscription:pull",
            post(pull_messages)
        )
        .route(
            "/v1/projects/:project/subscriptions/:subscription:acknowledge",
            post(acknowledge_messages)
        )
        .with_state(core)
}
```

### 3.3 Message Ordering

```rust
// Ordering key ensures messages with same key are delivered in order
pub struct OrderingQueue {
    ordering_key: String,
    messages: VecDeque<PubsubMessage>,
    in_flight: HashSet<String>,  // Message IDs
}

impl OrderingQueue {
    pub fn can_deliver(&self) -> bool {
        // Can only deliver next message if no messages are in flight
        self.in_flight.is_empty()
    }

    pub fn deliver_next(&mut self) -> Option<PubsubMessage> {
        if self.can_deliver() {
            if let Some(msg) = self.messages.pop_front() {
                self.in_flight.insert(msg.message_id.clone());
                Some(msg)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn ack(&mut self, message_id: &str) {
        self.in_flight.remove(message_id);
    }
}
```

### 3.4 Message Filtering

```rust
// Implement CEL (Common Expression Language) subset for filtering
use cel_interpreter::{Context, Value};

pub struct SubscriptionFilter {
    expression: String,
    compiled: cel_interpreter::Program,
}

impl SubscriptionFilter {
    pub fn new(expression: String) -> Result<Self, FilterError> {
        let compiled = cel_interpreter::Program::compile(&expression)?;
        Ok(Self { expression, compiled })
    }

    pub fn matches(&self, message: &PubsubMessage) -> Result<bool, FilterError> {
        let mut context = Context::default();

        // Add message fields to context
        context.add_variable("message.data", Value::Bytes(message.data.clone()));

        // Add attributes
        for (key, value) in &message.attributes {
            context.add_variable(&format!("attributes.{}", key), Value::String(value.clone()));
        }

        // Evaluate
        let result = self.compiled.execute(&context)?;
        Ok(result.as_bool().unwrap_or(false))
    }
}

// Example filters:
// attributes.priority = "high"
// attributes.region IN ["us-east1", "us-west1"]
// hasPrefix(attributes.name, "test-")
```

### 3.5 SDK Compatibility Matrix

**Target SDKs:**
- **google-cloud-python:** 2.23+
- **@google-cloud/pubsub (Node.js):** 4.7+
- **cloud.google.com/go/pubsub (Go):** 1.42+
- **google-cloud-pubsub (Ruby):** 2.21+
- **google-cloud-pubsub (Java):** 1.130+
- **Google.Cloud.PubSub.V1 (.NET):** 3.15+

**Endpoint Configuration:**
```python
# Python
from google.cloud import pubsub_v1
from google.api_core import client_options

publisher = pubsub_v1.PublisherClient(
    client_options=client_options.ClientOptions(
        api_endpoint='localhost:8086'  # gRPC
    )
)

# Or with REST
publisher = pubsub_v1.PublisherClient(
    client_options=client_options.ClientOptions(
        api_endpoint='http://localhost:8085'  # HTTP
    )
)
```

```javascript
// Node.js
const {PubSub} = require('@google-cloud/pubsub');

const pubsub = new PubSub({
  apiEndpoint: 'localhost:8086',  // gRPC
  projectId: 'local-project',
});
```

```go
// Go
import "cloud.google.com/go/pubsub"

client, err := pubsub.NewClient(ctx, "local-project",
    option.WithEndpoint("localhost:8086"),
    option.WithoutAuthentication(),
)
```

## 4. Storage Backend Architecture

### 4.1 Backend Trait

```rust
use async_trait::async_trait;
use std::time::Duration;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    // Queue/Topic Management
    async fn create_queue(&self, config: QueueConfig) -> Result<Queue>;
    async fn get_queue(&self, id: &str) -> Result<Option<Queue>>;
    async fn delete_queue(&self, id: &str) -> Result<()>;
    async fn list_queues(&self, filter: &QueueFilter) -> Result<Vec<Queue>>;
    async fn update_queue(&self, id: &str, config: QueueConfig) -> Result<Queue>;

    // Message Operations
    async fn send_message(&self, queue_id: &str, message: Message) -> Result<MessageId>;
    async fn send_messages(&self, queue_id: &str, messages: Vec<Message>) -> Result<Vec<MessageId>>;
    async fn receive_messages(&self, queue_id: &str, opts: ReceiveOptions) -> Result<Vec<ReceivedMessage>>;
    async fn delete_message(&self, queue_id: &str, receipt_handle: &str) -> Result<()>;
    async fn change_visibility(&self, queue_id: &str, receipt_handle: &str, timeout: Duration) -> Result<()>;

    // Subscription Operations (Pub/Sub)
    async fn create_subscription(&self, config: SubscriptionConfig) -> Result<Subscription>;
    async fn get_subscription(&self, id: &str) -> Result<Option<Subscription>>;
    async fn delete_subscription(&self, id: &str) -> Result<()>;
    async fn list_subscriptions(&self, topic_id: Option<&str>) -> Result<Vec<Subscription>>;

    // Maintenance
    async fn purge_queue(&self, queue_id: &str) -> Result<()>;
    async fn get_stats(&self, queue_id: &str) -> Result<QueueStats>;

    // Visibility Timeout Management
    async fn process_expired_visibility(&self) -> Result<Vec<Message>>;

    // Dead Letter Queue
    async fn move_to_dlq(&self, message: &Message, dlq_id: &str) -> Result<()>;

    // Health Check
    async fn health_check(&self) -> Result<HealthStatus>;
}
```

### 4.2 In-Memory Backend

```rust
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct InMemoryBackend {
    queues: Arc<RwLock<HashMap<String, QueueData>>>,
    subscriptions: Arc<RwLock<HashMap<String, SubscriptionData>>>,
    config: InMemoryConfig,
}

struct QueueData {
    config: QueueConfig,
    messages: VecDeque<StoredMessage>,
    in_flight: HashMap<String, InFlightMessage>,
    dlq_messages: VecDeque<StoredMessage>,
    stats: QueueStats,

    // FIFO-specific
    deduplication_ids: HashMap<String, Instant>,  // 5-minute window
    message_groups: HashMap<String, VecDeque<StoredMessage>>,

    // Pub/Sub-specific
    ordering_queues: HashMap<String, OrderingQueue>,
}

struct StoredMessage {
    id: String,
    body: Vec<u8>,
    attributes: HashMap<String, String>,
    enqueue_time: Instant,
    receive_count: u32,
    first_receive_time: Option<Instant>,

    // FIFO
    sequence_number: Option<u64>,
    deduplication_id: Option<String>,
    message_group_id: Option<String>,

    // Pub/Sub
    ordering_key: Option<String>,
    publish_time: Instant,
}

struct InFlightMessage {
    message: StoredMessage,
    receipt_handle: String,
    visibility_timeout: Instant,
    receive_time: Instant,
}

#[derive(Clone)]
pub struct InMemoryConfig {
    pub max_queues: usize,
    pub max_messages_per_queue: usize,
    pub max_total_messages: usize,
    pub eviction_policy: EvictionPolicy,
}

pub enum EvictionPolicy {
    LRU,           // Least Recently Used
    FIFO,          // First In First Out
    RejectNew,     // Reject when full
}

#[async_trait]
impl StorageBackend for InMemoryBackend {
    async fn send_message(&self, queue_id: &str, message: Message) -> Result<MessageId> {
        let mut queues = self.queues.write().await;
        let queue = queues.get_mut(queue_id)
            .ok_or(Error::QueueNotFound)?;

        // Check FIFO deduplication
        if let Some(dedup_id) = &message.deduplication_id {
            if queue.deduplication_ids.contains_key(dedup_id) {
                // Return existing message ID for deduplication
                return Ok(MessageId::new());
            }
            queue.deduplication_ids.insert(dedup_id.clone(), Instant::now());
        }

        // Check capacity
        if queue.messages.len() >= self.config.max_messages_per_queue {
            self.apply_eviction_policy(queue)?;
        }

        let stored = StoredMessage {
            id: Uuid::new_v4().to_string(),
            body: message.body,
            attributes: message.attributes,
            enqueue_time: Instant::now(),
            receive_count: 0,
            first_receive_time: None,
            sequence_number: message.sequence_number,
            deduplication_id: message.deduplication_id,
            message_group_id: message.message_group_id.clone(),
            ordering_key: message.ordering_key,
            publish_time: Instant::now(),
        };

        let message_id = stored.id.clone();

        // Handle FIFO message groups
        if let Some(group_id) = &message.message_group_id {
            queue.message_groups
                .entry(group_id.clone())
                .or_insert_with(VecDeque::new)
                .push_back(stored);
        } else {
            queue.messages.push_back(stored);
        }

        queue.stats.messages_sent += 1;
        Ok(MessageId(message_id))
    }

    async fn receive_messages(&self, queue_id: &str, opts: ReceiveOptions) -> Result<Vec<ReceivedMessage>> {
        let mut queues = self.queues.write().await;
        let queue = queues.get_mut(queue_id)
            .ok_or(Error::QueueNotFound)?;

        let mut received = Vec::new();
        let max_messages = opts.max_messages.min(10); // AWS limit

        // Handle FIFO queues with message groups
        if queue.config.is_fifo {
            // Process one message per group to maintain ordering
            for (_, group_queue) in queue.message_groups.iter_mut() {
                if received.len() >= max_messages {
                    break;
                }

                if let Some(mut msg) = group_queue.pop_front() {
                    msg.receive_count += 1;
                    if msg.first_receive_time.is_none() {
                        msg.first_receive_time = Some(Instant::now());
                    }

                    let receipt_handle = generate_receipt_handle(&msg.id, queue_id);
                    let visibility_timeout = Instant::now() + opts.visibility_timeout;

                    queue.in_flight.insert(receipt_handle.clone(), InFlightMessage {
                        message: msg.clone(),
                        receipt_handle: receipt_handle.clone(),
                        visibility_timeout,
                        receive_time: Instant::now(),
                    });

                    received.push(ReceivedMessage::from_stored(msg, receipt_handle));
                }
            }
        } else {
            // Standard queue
            for _ in 0..max_messages {
                if let Some(mut msg) = queue.messages.pop_front() {
                    msg.receive_count += 1;
                    if msg.first_receive_time.is_none() {
                        msg.first_receive_time = Some(Instant::now());
                    }

                    let receipt_handle = generate_receipt_handle(&msg.id, queue_id);
                    let visibility_timeout = Instant::now() + opts.visibility_timeout;

                    queue.in_flight.insert(receipt_handle.clone(), InFlightMessage {
                        message: msg.clone(),
                        receipt_handle: receipt_handle.clone(),
                        visibility_timeout,
                        receive_time: Instant::now(),
                    });

                    received.push(ReceivedMessage::from_stored(msg, receipt_handle));
                } else {
                    break;
                }
            }
        }

        queue.stats.messages_received += received.len() as u64;
        Ok(received)
    }

    async fn process_expired_visibility(&self) -> Result<Vec<Message>> {
        let mut queues = self.queues.write().await;
        let now = Instant::now();
        let mut requeued = Vec::new();

        for (_, queue) in queues.iter_mut() {
            let expired: Vec<String> = queue.in_flight.iter()
                .filter(|(_, msg)| msg.visibility_timeout < now)
                .map(|(handle, _)| handle.clone())
                .collect();

            for handle in expired {
                if let Some(in_flight) = queue.in_flight.remove(&handle) {
                    let mut msg = in_flight.message;

                    // Check if should move to DLQ
                    if let Some(dlq_config) = &queue.config.dlq_config {
                        if msg.receive_count >= dlq_config.max_receive_count {
                            queue.dlq_messages.push_back(msg.clone());
                            queue.stats.messages_to_dlq += 1;
                            continue;
                        }
                    }

                    // Return to queue
                    if let Some(group_id) = &msg.message_group_id {
                        queue.message_groups
                            .entry(group_id.clone())
                            .or_insert_with(VecDeque::new)
                            .push_back(msg.clone());
                    } else {
                        queue.messages.push_back(msg.clone());
                    }

                    requeued.push(Message::from_stored(&msg));
                }
            }
        }

        Ok(requeued)
    }
}
```

### 4.3 SQLite Backend

```rust
use sqlx::{sqlite::SqlitePool, Row};

pub struct SqliteBackend {
    pool: SqlitePool,
    config: SqliteConfig,
}

#[derive(Clone)]
pub struct SqliteConfig {
    pub path: PathBuf,
    pub max_connections: u32,
    pub wal_mode: bool,
    pub busy_timeout: Duration,
}

impl SqliteBackend {
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        let pool = SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&config.path)
                .create_if_missing(true)
                .journal_mode(if config.wal_mode {
                    sqlx::sqlite::SqliteJournalMode::Wal
                } else {
                    sqlx::sqlite::SqliteJournalMode::Delete
                })
                .busy_timeout(config.busy_timeout)
        ).await?;

        // Run migrations
        Self::migrate(&pool).await?;

        Ok(Self { pool, config })
    }

    async fn migrate(pool: &SqlitePool) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS queues (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                queue_type TEXT NOT NULL,
                config JSON NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                queue_id TEXT NOT NULL,
                body BLOB NOT NULL,
                attributes JSON NOT NULL,
                enqueue_time INTEGER NOT NULL,
                visibility_timeout INTEGER,
                receive_count INTEGER NOT NULL DEFAULT 0,
                first_receive_time INTEGER,

                -- FIFO fields
                sequence_number INTEGER,
                deduplication_id TEXT,
                message_group_id TEXT,

                -- Pub/Sub fields
                ordering_key TEXT,
                publish_time INTEGER NOT NULL,

                FOREIGN KEY (queue_id) REFERENCES queues(id) ON DELETE CASCADE
            )
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_messages_queue_id
            ON messages(queue_id)
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_messages_visibility
            ON messages(queue_id, visibility_timeout)
            WHERE visibility_timeout IS NOT NULL
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_messages_group
            ON messages(queue_id, message_group_id, sequence_number)
            WHERE message_group_id IS NOT NULL
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS subscriptions (
                id TEXT PRIMARY KEY,
                topic_id TEXT NOT NULL,
                name TEXT NOT NULL,
                config JSON NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (topic_id) REFERENCES queues(id) ON DELETE CASCADE,
                UNIQUE(topic_id, name)
            )
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS subscription_messages (
                subscription_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                ack_deadline INTEGER NOT NULL,
                ack_id TEXT NOT NULL,
                delivered_at INTEGER,
                PRIMARY KEY (subscription_id, message_id),
                FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            )
        "#).execute(pool).await?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS deduplication_cache (
                queue_id TEXT NOT NULL,
                deduplication_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                PRIMARY KEY (queue_id, deduplication_id)
            )
        "#).execute(pool).await?;

        Ok(())
    }
}

#[async_trait]
impl StorageBackend for SqliteBackend {
    async fn send_message(&self, queue_id: &str, message: Message) -> Result<MessageId> {
        let mut tx = self.pool.begin().await?;

        // Check FIFO deduplication
        if let Some(dedup_id) = &message.deduplication_id {
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT message_id FROM deduplication_cache
                 WHERE queue_id = ? AND deduplication_id = ? AND expires_at > ?"
            )
            .bind(queue_id)
            .bind(dedup_id)
            .bind(now_timestamp())
            .fetch_optional(&mut *tx)
            .await?;

            if let Some((existing_id,)) = existing {
                return Ok(MessageId(existing_id));
            }
        }

        let message_id = Uuid::new_v4().to_string();
        let now = now_timestamp();

        // Insert message
        sqlx::query(r#"
            INSERT INTO messages (
                id, queue_id, body, attributes, enqueue_time,
                receive_count, sequence_number, deduplication_id,
                message_group_id, ordering_key, publish_time
            ) VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?)
        "#)
        .bind(&message_id)
        .bind(queue_id)
        .bind(&message.body)
        .bind(serde_json::to_string(&message.attributes)?)
        .bind(now)
        .bind(message.sequence_number)
        .bind(&message.deduplication_id)
        .bind(&message.message_group_id)
        .bind(&message.ordering_key)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // Insert deduplication entry if needed
        if let Some(dedup_id) = &message.deduplication_id {
            let expires_at = now + 300; // 5 minutes
            sqlx::query(
                "INSERT INTO deduplication_cache (queue_id, deduplication_id, message_id, expires_at)
                 VALUES (?, ?, ?, ?)"
            )
            .bind(queue_id)
            .bind(dedup_id)
            .bind(&message_id)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(MessageId(message_id))
    }

    async fn receive_messages(&self, queue_id: &str, opts: ReceiveOptions) -> Result<Vec<ReceivedMessage>> {
        let mut tx = self.pool.begin().await?;
        let now = now_timestamp();
        let visibility_timeout = now + opts.visibility_timeout.as_secs() as i64;
        let max_messages = opts.max_messages.min(10);

        // Get queue config to check if FIFO
        let queue: (String,) = sqlx::query_as(
            "SELECT config FROM queues WHERE id = ?"
        )
        .bind(queue_id)
        .fetch_one(&mut *tx)
        .await?;

        let config: QueueConfig = serde_json::from_str(&queue.0)?;

        let messages = if config.is_fifo {
            // FIFO: one message per group, ordered by sequence
            sqlx::query_as::<_, MessageRow>(r#"
                SELECT DISTINCT ON (message_group_id) *
                FROM messages
                WHERE queue_id = ?
                  AND (visibility_timeout IS NULL OR visibility_timeout < ?)
                ORDER BY message_group_id, sequence_number
                LIMIT ?
            "#)
            .bind(queue_id)
            .bind(now)
            .bind(max_messages as i64)
            .fetch_all(&mut *tx)
            .await?
        } else {
            // Standard: any available messages
            sqlx::query_as::<_, MessageRow>(r#"
                SELECT * FROM messages
                WHERE queue_id = ?
                  AND (visibility_timeout IS NULL OR visibility_timeout < ?)
                ORDER BY enqueue_time
                LIMIT ?
            "#)
            .bind(queue_id)
            .bind(now)
            .bind(max_messages as i64)
            .fetch_all(&mut *tx)
            .await?
        };

        let mut received = Vec::new();

        for msg in messages {
            // Update visibility timeout and receive count
            sqlx::query(r#"
                UPDATE messages
                SET visibility_timeout = ?,
                    receive_count = receive_count + 1,
                    first_receive_time = COALESCE(first_receive_time, ?)
                WHERE id = ?
            "#)
            .bind(visibility_timeout)
            .bind(now)
            .bind(&msg.id)
            .execute(&mut *tx)
            .await?;

            let receipt_handle = generate_receipt_handle(&msg.id, queue_id);
            received.push(ReceivedMessage {
                message_id: msg.id,
                receipt_handle,
                body: msg.body,
                attributes: serde_json::from_str(&msg.attributes)?,
                receive_count: msg.receive_count + 1,
                message_group_id: msg.message_group_id,
                sequence_number: msg.sequence_number,
            });
        }

        tx.commit().await?;
        Ok(received)
    }

    async fn process_expired_visibility(&self) -> Result<Vec<Message>> {
        let now = now_timestamp();

        // Get expired messages
        let expired = sqlx::query_as::<_, MessageRow>(
            "SELECT * FROM messages WHERE visibility_timeout < ?"
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        // Reset visibility timeout
        sqlx::query(
            "UPDATE messages SET visibility_timeout = NULL WHERE visibility_timeout < ?"
        )
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(expired.into_iter().map(|r| Message::from_row(r)).collect())
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    queue_id: String,
    body: Vec<u8>,
    attributes: String,
    enqueue_time: i64,
    visibility_timeout: Option<i64>,
    receive_count: i32,
    first_receive_time: Option<i64>,
    sequence_number: Option<i64>,
    deduplication_id: Option<String>,
    message_group_id: Option<String>,
    ordering_key: Option<String>,
    publish_time: i64,
}
```

## 5. Core Queue Engine

### 5.1 Message Router

```rust
pub struct MessageRouter {
    backends: HashMap<String, Arc<dyn StorageBackend>>,
    default_backend: String,
    queue_backends: Arc<RwLock<HashMap<String, String>>>,
}

impl MessageRouter {
    pub async fn route_send(&self, queue_id: &str, message: Message) -> Result<MessageId> {
        let backend = self.get_backend_for_queue(queue_id).await?;
        backend.send_message(queue_id, message).await
    }

    pub async fn route_receive(&self, queue_id: &str, opts: ReceiveOptions) -> Result<Vec<ReceivedMessage>> {
        let backend = self.get_backend_for_queue(queue_id).await?;
        backend.receive_messages(queue_id, opts).await
    }

    async fn get_backend_for_queue(&self, queue_id: &str) -> Result<Arc<dyn StorageBackend>> {
        let queue_backends = self.queue_backends.read().await;
        let backend_name = queue_backends.get(queue_id)
            .unwrap_or(&self.default_backend);

        self.backends.get(backend_name)
            .cloned()
            .ok_or(Error::BackendNotFound)
    }
}
```

### 5.2 Visibility Timeout Manager

```rust
pub struct VisibilityManager {
    router: Arc<MessageRouter>,
    check_interval: Duration,
}

impl VisibilityManager {
    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.check_interval);
            loop {
                interval.tick().await;
                if let Err(e) = self.process_expired().await {
                    tracing::error!("Failed to process expired visibility: {}", e);
                }
            }
        });
    }

    async fn process_expired(&self) -> Result<()> {
        // Process all backends
        for backend in self.router.backends.values() {
            backend.process_expired_visibility().await?;
        }
        Ok(())
    }
}
```

### 5.3 Dead Letter Queue Handler

```rust
pub struct DlqHandler {
    router: Arc<MessageRouter>,
}

impl DlqHandler {
    pub async fn handle_failed_message(
        &self,
        message: &Message,
        source_queue_id: &str,
    ) -> Result<()> {
        // Get source queue config
        let backend = self.router.get_backend_for_queue(source_queue_id).await?;
        let queue = backend.get_queue(source_queue_id).await?
            .ok_or(Error::QueueNotFound)?;

        if let Some(dlq_config) = queue.config.dlq_config {
            // Move to DLQ
            backend.move_to_dlq(message, &dlq_config.target_queue_id).await?;
        }

        Ok(())
    }
}
```

## 6. Configuration System

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LclqConfig {
    pub server: ServerConfig,
    pub sqs: SqsConfig,
    pub pubsub: PubsubConfig,
    pub storage: StorageConfig,
    pub logging: LoggingConfig,
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub sqs_port: u16,
    pub pubsub_http_port: u16,
    pub pubsub_grpc_port: u16,
    pub admin_port: u16,
    pub tls: Option<TlsConfig>,
    pub graceful_shutdown_timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub default_visibility_timeout: Duration,
    pub default_retention_period: Duration,
    pub max_message_size: usize,
    pub verify_signatures: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubsubConfig {
    pub enabled: bool,
    pub http_endpoint: String,
    pub grpc_endpoint: String,
    pub default_ack_deadline: Duration,
    pub default_retention_period: Duration,
    pub max_message_size: usize,
    pub enable_ordering: bool,
    pub enable_filtering: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub default_backend: String,
    pub backends: HashMap<String, BackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BackendConfig {
    InMemory {
        max_queues: usize,
        max_messages_per_queue: usize,
        max_total_messages: usize,
        eviction_policy: EvictionPolicy,
    },
    Sqlite {
        path: PathBuf,
        max_connections: u32,
        wal_mode: bool,
        busy_timeout_ms: u64,
    },
}

impl Default for LclqConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                sqs_port: 4566,
                pubsub_http_port: 8085,
                pubsub_grpc_port: 8086,
                admin_port: 9000,
                tls: None,
                graceful_shutdown_timeout: Duration::from_secs(30),
            },
            sqs: SqsConfig {
                enabled: true,
                endpoint: "http://localhost:4566".to_string(),
                default_visibility_timeout: Duration::from_secs(30),
                default_retention_period: Duration::from_secs(345600),
                max_message_size: 262144,
                verify_signatures: false,
            },
            pubsub: PubsubConfig {
                enabled: true,
                http_endpoint: "http://localhost:8085".to_string(),
                grpc_endpoint: "localhost:8086".to_string(),
                default_ack_deadline: Duration::from_secs(10),
                default_retention_period: Duration::from_secs(604800),
                max_message_size: 10485760,
                enable_ordering: true,
                enable_filtering: true,
            },
            storage: StorageConfig {
                default_backend: "memory".to_string(),
                backends: {
                    let mut map = HashMap::new();
                    map.insert("memory".to_string(), BackendConfig::InMemory {
                        max_queues: 10000,
                        max_messages_per_queue: 100000,
                        max_total_messages: 1000000,
                        eviction_policy: EvictionPolicy::RejectNew,
                    });
                    map
                },
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
            metrics: MetricsConfig {
                enabled: true,
                port: 9090,
            },
        }
    }
}
```

**Configuration File (lclq.toml):**
```toml
[server]
host = "0.0.0.0"
sqs_port = 4566
pubsub_http_port = 8085
pubsub_grpc_port = 8086
admin_port = 9000

[sqs]
enabled = true
default_visibility_timeout = "30s"
default_retention_period = "4d"
max_message_size = 262144

[pubsub]
enabled = true
default_ack_deadline = "10s"
default_retention_period = "7d"
max_message_size = 10485760
enable_ordering = true
enable_filtering = true

[storage]
default_backend = "memory"

[storage.backends.memory]
type = "InMemory"
max_queues = 10000
max_messages_per_queue = 100000
max_total_messages = 1000000
eviction_policy = "RejectNew"

[storage.backends.sqlite]
type = "Sqlite"
path = "./lclq.db"
max_connections = 10
wal_mode = true
busy_timeout_ms = 5000

[logging]
level = "info"
format = "json"

[metrics]
enabled = true
port = 9090
```

## 7. Performance Requirements

### 7.1 Benchmarking Targets

```rust
// Benchmark configuration
#[derive(Debug, Clone)]
pub struct PerformanceTargets {
    // Throughput (messages/second)
    pub memory_throughput_min: u64,      // 10,000
    pub sqlite_throughput_min: u64,      // 1,000

    // Latency (milliseconds)
    pub memory_p50_latency_max: f64,     // 1.0
    pub memory_p99_latency_max: f64,     // 10.0
    pub sqlite_p50_latency_max: f64,     // 5.0
    pub sqlite_p99_latency_max: f64,     // 50.0

    // Concurrent connections
    pub max_concurrent_connections: usize, // 1,000

    // Memory
    pub max_idle_memory_mb: usize,       // 50

    // Startup
    pub max_startup_time_ms: u64,        // 100
}
```

### 7.2 Metrics Collection

```rust
use prometheus::{Registry, IntCounterVec, HistogramVec, GaugeVec};

pub struct Metrics {
    // Message operations
    pub messages_sent: IntCounterVec,
    pub messages_received: IntCounterVec,
    pub messages_deleted: IntCounterVec,
    pub messages_to_dlq: IntCounterVec,

    // Latency
    pub send_latency: HistogramVec,
    pub receive_latency: HistogramVec,

    // Queue stats
    pub queue_depth: GaugeVec,
    pub in_flight_messages: GaugeVec,
    pub queue_count: GaugeVec,

    // System
    pub active_connections: GaugeVec,
    pub backend_errors: IntCounterVec,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Result<Self> {
        Ok(Self {
            messages_sent: IntCounterVec::new(
                opts!("lclq_messages_sent_total", "Total messages sent"),
                &["queue_id", "dialect"]
            )?,
            // ... other metrics
        })
    }
}
```

## 8. Testing Strategy

### 8.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqs_send_receive() {
        let backend = InMemoryBackend::new(Default::default());
        let queue_id = "test-queue";

        // Create queue
        backend.create_queue(QueueConfig {
            id: queue_id.to_string(),
            name: queue_id.to_string(),
            queue_type: QueueType::SqsStandard,
            ..Default::default()
        }).await.unwrap();

        // Send message
        let msg = Message {
            body: b"test message".to_vec(),
            attributes: HashMap::new(),
            ..Default::default()
        };
        let msg_id = backend.send_message(queue_id, msg).await.unwrap();
        assert!(!msg_id.0.is_empty());

        // Receive message
        let received = backend.receive_messages(queue_id, ReceiveOptions {
            max_messages: 1,
            visibility_timeout: Duration::from_secs(30),
            wait_time: Duration::from_secs(0),
        }).await.unwrap();

        assert_eq!(received.len(), 1);
        assert_eq!(received[0].body, b"test message");
    }

    #[tokio::test]
    async fn test_fifo_ordering() {
        // Test FIFO message ordering within groups
    }

    #[tokio::test]
    async fn test_deduplication() {
        // Test FIFO deduplication
    }
}
```

### 8.2 Integration Tests

```rust
#[cfg(test)]
mod integration_tests {
    use aws_sdk_sqs::Client as SqsClient;
    use google_cloud_pubsub::client::Client as PubsubClient;

    #[tokio::test]
    async fn test_aws_sdk_compatibility() {
        // Start lclq server
        let server = start_test_server().await;

        // Create AWS SDK client pointing to lclq
        let config = aws_config::from_env()
            .endpoint_url("http://localhost:4566")
            .load()
            .await;
        let client = SqsClient::new(&config);

        // Test operations
        let queue_url = client
            .create_queue()
            .queue_name("test-queue")
            .send()
            .await
            .unwrap()
            .queue_url
            .unwrap();

        client
            .send_message()
            .queue_url(&queue_url)
            .message_body("test message")
            .send()
            .await
            .unwrap();

        let messages = client
            .receive_message()
            .queue_url(&queue_url)
            .max_number_of_messages(1)
            .send()
            .await
            .unwrap()
            .messages
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].body.as_ref().unwrap(), "test message");
    }

    #[tokio::test]
    async fn test_gcp_sdk_compatibility() {
        // Test with GCP Pub/Sub SDK
    }
}
```

### 8.3 Performance Tests

```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_send(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let backend = rt.block_on(async {
        InMemoryBackend::new(Default::default())
    });

    c.bench_with_input(
        BenchmarkId::new("send_message", "memory"),
        &backend,
        |b, backend| {
            b.to_async(&rt).iter(|| async {
                backend.send_message("test-queue", Message::default()).await
            });
        },
    );
}

criterion_group!(benches, benchmark_send);
criterion_main!(benches);
```

## 9. Security Considerations

### 9.1 Authentication (Optional)

```rust
#[derive(Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub mode: AuthMode,
}

pub enum AuthMode {
    None,
    ApiKey { keys: HashSet<String> },
    Token { secret: String },
    AwsSignatureV4,
}

pub struct AuthMiddleware {
    config: AuthConfig,
}

impl AuthMiddleware {
    pub fn verify(&self, request: &Request) -> Result<(), AuthError> {
        if !self.config.enabled {
            return Ok(());
        }

        match &self.config.mode {
            AuthMode::None => Ok(()),
            AuthMode::ApiKey { keys } => {
                let api_key = request.headers()
                    .get("X-API-Key")
                    .and_then(|v| v.to_str().ok())
                    .ok_or(AuthError::MissingApiKey)?;

                if keys.contains(api_key) {
                    Ok(())
                } else {
                    Err(AuthError::InvalidApiKey)
                }
            }
            AuthMode::AwsSignatureV4 => {
                // Verify AWS Signature V4
                self.verify_aws_signature(request)
            }
            AuthMode::Token { secret } => {
                // Verify JWT token
                self.verify_token(request, secret)
            }
        }
    }
}
```

### 9.2 TLS Support

```rust
use tokio_rustls::rustls::{ServerConfig as TlsServerConfig, Certificate, PrivateKey};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub ca_cert_path: Option<PathBuf>,
}

impl TlsConfig {
    pub fn load(&self) -> Result<TlsServerConfig> {
        let certs = load_certs(&self.cert_path)?;
        let key = load_private_key(&self.key_path)?;

        let config = TlsServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;

        Ok(config)
    }
}
```

## 10. Deployment and Operations

### 10.1 CLI Tool

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lclq")]
#[command(about = "Local Cloud Queue - AWS SQS and GCP Pub/Sub compatible queue system")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "lclq.toml")]
    config: PathBuf,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the lclq server
    Start {
        #[arg(long)]
        sqs_port: Option<u16>,

        #[arg(long)]
        pubsub_http_port: Option<u16>,

        #[arg(long)]
        pubsub_grpc_port: Option<u16>,

        #[arg(long)]
        backend: Option<String>,
    },

    /// Manage queues
    Queue {
        #[command(subcommand)]
        action: QueueAction,
    },

    /// Health check
    Health,

    /// Show statistics
    Stats,
}

#[derive(Subcommand)]
enum QueueAction {
    List,
    Create { name: String },
    Delete { name: String },
    Purge { name: String },
    Stats { name: String },
}
```

### 10.2 Docker Support

**Dockerfile:**
```dockerfile
FROM rust:1.90-slim as builder

WORKDIR /app
COPY Cargo.* ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/lclq /usr/local/bin/

EXPOSE 4566 8085 8086 9000 9090

ENTRYPOINT ["lclq"]
CMD ["start"]
```

**Docker Compose:**
```yaml
version: '3.8'

services:
  lclq:
    image: lclq:latest
    ports:
      - "4566:4566"   # SQS
      - "8085:8085"   # Pub/Sub HTTP
      - "8086:8086"   # Pub/Sub gRPC
      - "9000:9000"   # Admin
      - "9090:9090"   # Metrics
    volumes:
      - ./lclq.toml:/etc/erans/lclq.toml
      - lclq-data:/data
    environment:
      - LCLQ_CONFIG=/etc/erans/lclq.toml
      - RUST_LOG=info

volumes:
  lclq-data:
```

## 11. Implementation Phases

### Phase 1: Core Foundation (Weeks 1-2)
- [ ] Project setup and structure
- [ ] Core data models
- [ ] In-memory backend implementation
- [ ] Basic queue operations
- [ ] Unit test framework

### Phase 2: SQS Implementation (Weeks 3-4)
- [ ] SQS HTTP/REST handler
- [ ] Standard queue support
- [ ] FIFO queue support
- [ ] Dead letter queue
- [ ] AWS SDK compatibility tests

### Phase 3: SQLite Backend (Week 5)
- [ ] SQLite schema design
- [ ] SQLite backend implementation
- [ ] Migration system
- [ ] Performance optimization

### Phase 4: Pub/Sub gRPC (Weeks 6-7)
- [ ] Protocol buffer definitions
- [ ] gRPC server implementation
- [ ] Topic/subscription model
- [ ] Message ordering
- [ ] GCP SDK compatibility tests

### Phase 5: Pub/Sub HTTP (Week 8)
- [ ] REST API implementation
- [ ] HTTP-gRPC feature parity
- [ ] Push subscription support
- [ ] Message filtering (CEL)

### Phase 6: Management & Operations (Week 9)
- [ ] CLI tool
- [ ] Admin API
- [ ] Metrics and monitoring
- [ ] Configuration system

### Phase 7: Performance & Polish (Week 10)
- [ ] Performance optimization
- [ ] Load testing
- [ ] Documentation
- [ ] Docker packaging

## 12. Success Metrics

### 12.1 Functional Metrics
- [ ] 100% compatibility with AWS SQS basic operations
- [ ] 100% compatibility with GCP Pub/Sub basic operations
- [ ] All major SDK tests passing (Python, Node, Go, Ruby, Java)
- [ ] Both gRPC and HTTP protocols fully functional for Pub/Sub

### 12.2 Performance Metrics
- [ ] Memory backend: >10,000 msg/sec throughput
- [ ] SQLite backend: >1,000 msg/sec throughput
- [ ] P99 latency <10ms for memory backend
- [ ] Support 1,000+ concurrent connections
- [ ] Startup time <100ms

### 12.3 Quality Metrics
- [ ] >90% code coverage
- [ ] Zero critical bugs
- [ ] Complete API documentation
- [ ] Comprehensive user guide

## 13. Risk Mitigation

### 13.1 Technical Risks

| Risk | Mitigation |
|------|-----------|
| gRPC-HTTP feature parity | Comprehensive test suite ensuring identical behavior |
| Protocol buffer version compatibility | Pin specific protobuf versions, compatibility tests |
| SQLite write contention | WAL mode, connection pooling, document limitations |
| Memory leaks | Regular profiling, Valgrind testing, fuzz testing |
| AWS signature verification complexity | Make optional, focus on endpoint compatibility first |

### 13.2 Compatibility Risks

| Risk | Mitigation |
|------|-----------|
| SDK version fragmentation | Test against multiple SDK versions in CI |
| Protocol edge cases | Extensive integration tests with real SDKs |
| Message size handling | Strict validation matching cloud service limits |
| Ordering guarantees | Comprehensive FIFO and ordering tests |

## 14. Appendices

### A. Resource Name Validation

**AWS SQS Queue Names:**
```rust
fn validate_sqs_queue_name(name: &str) -> Result<()> {
    // 1-80 characters
    if name.len() < 1 || name.len() > 80 {
        return Err(ValidationError::InvalidLength);
    }

    // Alphanumeric, hyphens, underscores only
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(ValidationError::InvalidCharacters);
    }

    // FIFO queues must end with .fifo
    if name.ends_with(".fifo") {
        let base = &name[..name.len()-5];
        if base.len() < 1 || base.len() > 75 {
            return Err(ValidationError::InvalidFifoLength);
        }
    }

    Ok(())
}
```

**GCP Pub/Sub Resource Names:**
```rust
fn validate_pubsub_resource_id(id: &str) -> Result<()> {
    // 3-255 characters
    if id.len() < 3 || id.len() > 255 {
        return Err(ValidationError::InvalidLength);
    }

    // Must start with letter
    if !id.chars().next().unwrap().is_alphabetic() {
        return Err(ValidationError::MustStartWithLetter);
    }

    // Cannot start with "goog"
    if id.starts_with("goog") {
        return Err(ValidationError::ReservedPrefix);
    }

    // Valid characters: letters, numbers, -, _, ~, %, +
    let valid_chars = |c: char| {
        c.is_alphanumeric() || matches!(c, '-' | '_' | '~' | '%' | '+' | '.')
    };

    if !id.chars().all(valid_chars) {
        return Err(ValidationError::InvalidCharacters);
    }

    Ok(())
}
```

### B. Message Size Validation

```rust
const SQS_MAX_MESSAGE_SIZE: usize = 262_144;      // 256 KB
const PUBSUB_MAX_MESSAGE_SIZE: usize = 10_485_760; // 10 MB

fn validate_sqs_message_size(body: &[u8], attributes: &HashMap<String, MessageAttributeValue>) -> Result<()> {
    let mut total_size = body.len();

    // Count attribute sizes
    for (key, value) in attributes {
        total_size += key.len();
        total_size += value.data_type.len();
        match value {
            MessageAttributeValue { string_value: Some(s), .. } => total_size += s.len(),
            MessageAttributeValue { binary_value: Some(b), .. } => total_size += b.len(),
            _ => {}
        }
    }

    if total_size > SQS_MAX_MESSAGE_SIZE {
        return Err(ValidationError::MessageTooLarge);
    }

    Ok(())
}

fn validate_pubsub_message_size(data: &[u8], attributes: &HashMap<String, String>) -> Result<()> {
    let mut total_size = data.len();

    for (key, value) in attributes {
        total_size += key.len();
        total_size += value.len();
    }

    if total_size > PUBSUB_MAX_MESSAGE_SIZE {
        return Err(ValidationError::MessageTooLarge);
    }

    Ok(())
}
```

### C. Receipt Handle Format

```rust
// Receipt handle encodes all info needed to delete message
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceiptHandleData {
    message_id: String,
    queue_id: String,
    receive_timestamp: i64,
    nonce: String,  // Prevents reuse
}

fn generate_receipt_handle(message_id: &str, queue_id: &str) -> String {
    let data = ReceiptHandleData {
        message_id: message_id.to_string(),
        queue_id: queue_id.to_string(),
        receive_timestamp: now_timestamp(),
        nonce: Uuid::new_v4().to_string(),
    };

    let json = serde_json::to_string(&data).unwrap();
    STANDARD.encode(json.as_bytes())
}

fn parse_receipt_handle(handle: &str) -> Result<ReceiptHandleData> {
    let decoded = STANDARD.decode(handle)
        .map_err(|_| Error::InvalidReceiptHandle)?;
    let json = String::from_utf8(decoded)
        .map_err(|_| Error::InvalidReceiptHandle)?;
    serde_json::from_str(&json)
        .map_err(|_| Error::InvalidReceiptHandle)
}
```

---

**Document Version:** 1.0
**Last Updated:** October 2025
**Authors:** Engineering Team
**Status:** Ready for Implementation
