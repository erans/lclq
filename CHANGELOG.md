# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-10-28

### Added

#### AWS SQS Compatibility
- **Queue Operations**
  - `CreateQueue` - Create standard and FIFO queues with full attribute support
  - `DeleteQueue` - Delete queues with proper cleanup
  - `GetQueueUrl` - Queue discovery by name
  - `ListQueues` - List all queues with name prefix filtering
  - `PurgeQueue` - Clear all messages from a queue
  - `TagQueue` / `UntagQueue` / `ListQueueTags` - Queue tagging support

- **Message Operations**
  - `SendMessage` - Send messages with attributes and MD5 verification
  - `SendMessageBatch` - Send up to 10 messages in a single request
  - `ReceiveMessage` - Receive messages with visibility timeout and attributes
  - `DeleteMessage` - Delete messages using receipt handles
  - `DeleteMessageBatch` - Delete up to 10 messages in a single request
  - `ChangeMessageVisibility` - Modify message visibility timeout
  - `ChangeMessageVisibilityBatch` - Batch visibility timeout modifications

- **Queue Attributes**
  - `GetQueueAttributes` - Retrieve queue configuration and statistics
  - `SetQueueAttributes` - Modify queue configuration including redrive policies
  - Support for all standard SQS attributes (visibility timeout, retention period, delay, etc.)

- **Advanced Features**
  - **Dead Letter Queues (DLQ)** - Automatic message movement after max receive count
  - **RedriveAllowPolicy** - DLQ access control configuration
  - **FIFO Queues** - Message ordering and deduplication
  - **Message Attributes** - Custom metadata for messages
  - **System Attributes** - SenderId, timestamps, receive count tracking
  - **Long Polling** - Up to 20 seconds wait time for messages
  - **Delay Queues** - 0-900 seconds message delay
  - **Message Retention** - 60 seconds to 14 days configurable retention
  - **Content-based Deduplication** - Automatic deduplication for FIFO queues
  - **Visibility Timeout Processing** - Active visibility timeout management

#### GCP Pub/Sub Compatibility
- **Topic Operations**
  - Create, delete, get, and list topics
  - Topic publishing with message attributes
  - Message ordering with ordering keys

- **Subscription Operations**
  - Create, delete, get, and list subscriptions
  - Pull and StreamingPull message delivery
  - Acknowledgment and negative acknowledgment (nack)
  - ModifyAckDeadline - Adjust acknowledgment deadlines
  - Subscription filtering

- **Advanced Features**
  - **Dead Letter Topics** - Failed message handling
  - **Message Retention** - Configurable message retention
  - **Message Ordering** - Guaranteed ordering with ordering keys

#### Storage Backends
- **In-Memory Backend (Default)**
  - 1.8M+ messages/sec throughput
  - Sub-10µs latency (P50: 7.4µs)
  - Configurable eviction policies (LRU, FIFO, RejectNew)
  - Maximum message limit configuration
  - Zero external dependencies

- **SQLite Backend (Persistent)**
  - Full ACID guarantees
  - Single-file database storage
  - Automatic schema migrations
  - Background maintenance and cleanup
  - Message retention enforcement

#### Security
- **HMAC-based Receipt Handle Protection**
  - Tamper-proof receipt handles using HMAC-SHA256
  - Configurable secret via `LCLQ_RECEIPT_SECRET` environment variable
  - Prevents unauthorized message deletion

#### Server & Infrastructure
- **HTTP Server** - SQS REST API on port 9324
- **gRPC Server** - Pub/Sub gRPC API on port 8085
- **REST Server** - Pub/Sub HTTP API on port 8086
- **Admin API** - Health checks and statistics on port 9000
- **Prometheus Metrics** - Monitoring endpoint on port 9090
- **CORS Support** - Browser-friendly API access
- **Structured Logging** - Tracing middleware with multiple output formats

#### CLI Commands
- `lclq start` - Start all services
- `lclq health` - Health check endpoint
- `lclq queue list` - List all queues
- `lclq queue create` - Create new queue
- `lclq queue delete` - Delete queue
- `lclq queue stats` - Queue statistics
- `lclq queue purge` - Clear all messages

#### Configuration
- **Config File Support** - TOML configuration (`lclq.toml`)
- **Environment Variables** - Full environment variable configuration
- **Command-line Flags** - Runtime configuration options
- **Zero-config Default** - Works out of the box with sensible defaults

#### Monitoring & Observability
- **Prometheus Metrics**
  - `lclq_messages_sent_total` - Total messages sent by queue
  - `lclq_messages_received_total` - Total messages received
  - `lclq_queue_depth` - Current queue message count
  - `lclq_send_latency_seconds` - Send operation latency histogram
  - `lclq_receive_latency_seconds` - Receive operation latency histogram

- **Admin API Endpoints**
  - `GET /health` - Server health status
  - `GET /stats` - System-wide statistics
  - `GET /queues` - List all queues with details
  - `GET /queues/{name}` - Individual queue information

#### Testing & Quality
- **408 Unit Tests** - 66% code coverage
- **60 Integration Tests** - Multi-language SDK testing
  - Python (boto3) - 7/7 SQS tests passing
  - JavaScript (AWS SDK v3) - SQS and Pub/Sub tests passing
  - Go - Pub/Sub integration tests
  - Rust - Type-safe API coverage

#### Distribution & Deployment
- **Binary Releases**
  - Linux x86_64
  - Linux ARM64 (aarch64)
  - macOS x86_64 (Intel)
  - macOS ARM64 (Apple Silicon)
  - Windows x86_64

- **Docker Image**
  - Multi-architecture support (linux/amd64)
  - Published to Docker Hub: `erans/lclq`
  - Tagged releases and `latest`

- **GitHub Release Workflow**
  - Automated binary builds and uploads
  - Cross-compilation support
  - Docker image publishing

#### Documentation
- Comprehensive README with quick start guide
- Performance benchmarks documentation
- Product and technical roadmaps
- CI/CD integration examples (GitHub Actions, Docker Compose)
- SDK usage examples (Python, JavaScript, Go, Rust)

### Performance
- **182x faster than AWS SQS** - 1.82M messages/sec vs 10K/sec
- **286x better latency** - 7.4µs P50 vs 10ms+ cloud latency
- **Batch Operations** - 4.67M messages/sec with batch size 10
- **End-to-End** - 894K messages/sec for send+receive cycles

### Fixed
- ARM64 cross-compilation using correct strip binary (`aarch64-linux-gnu-strip`)
- Visibility timeout=0 handling for immediate message availability
- Dead letter queue visibility timeout processing
- Receipt handle encoding/decoding security
- Message retention cleanup and enforcement
- SQLite schema migrations organization
- CI/CD test reliability across platforms

[0.1.0]: https://github.com/erans/lclq/releases/tag/v0.1.0
