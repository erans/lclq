# lclq Implementation TODO

**Status:** Ready for Implementation
**Last Updated:** October 2025

This document tracks all implementation tasks for lclq based on the PRD and Technical PRD.

---

## Phase 0: Project Setup & Infrastructure

### 0.1 Repository and Build Setup
- [x] Initialize Rust project with Cargo
  - [x] Create `Cargo.toml` with Rust 2024 edition
  - [x] Set up workspace structure (`src/`, `tests/`, `benches/`)
  - [x] Configure rustfmt with `.rustfmt.toml`
  - [x] Configure clippy with `clippy.toml`
  - [x] Add `.gitignore` for Rust projects
- [ ] Set up development tooling
  - [ ] Install cargo-make for task automation
  - [ ] Create `Makefile.toml` with common tasks
  - [ ] Install cross for cross-compilation
  - [ ] Install cargo-tarpaulin for code coverage
  - [ ] Install cargo-audit for security auditing
- [ ] Configure CI/CD
  - [ ] Create `.github/workflows/ci.yml`
  - [ ] Add build workflow for Linux, macOS, Windows
  - [ ] Add test workflow
  - [ ] Add clippy and rustfmt checks
  - [ ] Add security audit workflow
  - [ ] Add code coverage reporting

### 0.2 Project Structure
- [x] Define module structure
  - [x] `src/main.rs` - CLI entry point
  - [x] `src/lib.rs` - Library exports
  - [x] `src/config/` - Configuration system
  - [x] `src/storage/` - Storage backends
  - [x] `src/core/` - Core queue engine
  - [x] `src/sqs/` - AWS SQS dialect
  - [x] `src/pubsub/` - GCP Pub/Sub dialect
  - [x] `src/server/` - HTTP/gRPC servers
  - [x] `src/metrics/` - Metrics and monitoring
  - [x] `src/types/` - Common data types
  - [x] `src/error/` - Error types
- [x] Create initial dependencies in `Cargo.toml`
  - [x] Add tokio with full features
  - [x] Add serde and serde_json
  - [x] Add thiserror and anyhow
  - [x] Add tracing and tracing-subscriber
  - [x] Add uuid and chrono

### 0.3 Documentation Setup
- [x] Create README.md with project overview
- [ ] Create CONTRIBUTING.md with development guidelines
- [x] Create LICENSE file (choose appropriate license)
- [ ] Set up rustdoc configuration
- [ ] Create examples directory structure

---

## Phase 1: Core Foundation (Weeks 1-2)

### 1.1 Core Data Types
- [x] Implement common types in `src/types/`
  - [x] `Message` struct with all fields
  - [x] `MessageId` wrapper type
  - [x] `MessageAttributes` type
  - [x] `QueueConfig` struct
  - [x] `QueueType` enum (SqsStandard, SqsFifo, PubSubTopic)
  - [x] `ReceiveOptions` struct
  - [x] `QueueStats` struct
  - [x] `SubscriptionConfig` struct (for Pub/Sub)
- [x] Implement validation functions
  - [x] SQS queue name validation (1-80 chars, alphanumeric/-/_)
  - [x] FIFO queue name validation (.fifo suffix, 75 char base)
  - [x] Pub/Sub topic ID validation (3-255 chars, must start with letter)
  - [x] Pub/Sub subscription ID validation
  - [x] Message size validation (SQS: 256KB, Pub/Sub: 10MB)
  - [x] Message attribute validation
- [x] Create error types in `src/error/`
  - [x] `Error` enum with all error variants
  - [x] `Result<T>` type alias
  - [x] `ValidationError` enum
  - [x] Error conversion implementations

### 1.2 Storage Backend Trait
- [x] Define `StorageBackend` trait in `src/storage/mod.rs`
  - [x] Queue management methods (create, get, delete, list, update)
  - [x] Message operations (send, send_batch, receive, delete)
  - [x] Subscription operations (create, get, delete, list)
  - [x] Maintenance operations (purge, get_stats)
  - [x] Visibility timeout management
  - [x] Dead letter queue operations
  - [x] Health check method
- [x] Define supporting types
  - [x] `QueueFilter` for listing queues
  - [x] `ReceivedMessage` struct
  - [x] `InFlightMessage` struct
  - [x] `HealthStatus` enum

### 1.3 In-Memory Backend Implementation
- [x] Implement `InMemoryBackend` in `src/storage/memory.rs`
  - [x] Define `InMemoryBackend` struct with RwLock-wrapped HashMaps
  - [x] Define `QueueData` struct
  - [x] Define `StoredMessage` struct
  - [x] Define `InFlightMessage` struct
  - [x] Define `InMemoryConfig` struct
- [x] Implement queue operations
  - [x] `create_queue` - Create new queue with validation
  - [x] `get_queue` - Retrieve queue by ID
  - [x] `delete_queue` - Remove queue and all messages
  - [x] `list_queues` - List queues with optional filtering
  - [x] `update_queue` - Modify queue configuration
- [x] Implement message operations
  - [x] `send_message` - Add message to queue
  - [x] `send_messages` - Batch message sending
  - [x] `receive_messages` - Pull messages with visibility timeout
  - [x] `delete_message` - Remove message using receipt handle
  - [x] `change_visibility` - Update visibility timeout
- [x] Implement FIFO support
  - [x] Message deduplication (5-minute window)
  - [x] Message group ordering
  - [x] Sequence number assignment
  - [x] Content-based deduplication (SHA-256)
- [x] Implement maintenance operations
  - [x] `purge_queue` - Remove all messages from queue
  - [x] `get_stats` - Return queue statistics
  - [x] `process_expired_visibility` - Requeue expired messages
  - [x] `health_check` - Return backend health status
- [x] Implement eviction policies
  - [x] LRU (Least Recently Used)
  - [x] FIFO (First In First Out)
  - [x] RejectNew (reject when full)
- [ ] Add tests for in-memory backend
  - [ ] Test basic send/receive flow
  - [ ] Test FIFO ordering
  - [ ] Test deduplication
  - [ ] Test visibility timeout expiration
  - [ ] Test concurrent access
  - [ ] Test capacity limits and eviction

### 1.4 Core Queue Engine
- [ ] Implement `MessageRouter` in `src/core/router.rs`
  - [ ] Backend registration and management
  - [ ] Queue-to-backend mapping
  - [ ] Route message operations to correct backend
- [x] Implement `VisibilityManager` in `src/core/visibility.rs`
  - [x] Background task for checking expired visibility timeouts
  - [x] Configurable check interval
  - [x] Requeue expired messages
  - [x] Move to DLQ if receive count exceeded
- [x] Implement `DlqHandler` in `src/core/dlq.rs`
  - [x] Check receive count against max attempts
  - [x] Move messages to configured DLQ
  - [x] Track DLQ statistics
- [x] Implement receipt handle generation
  - [x] Create `ReceiptHandleData` struct
  - [x] `generate_receipt_handle` function (base64 encoding)
  - [x] `parse_receipt_handle` function (base64 decoding)
  - [x] Include nonce for security

### 1.5 Configuration System
- [x] Implement configuration types in `src/config/`
  - [x] `LclqConfig` main configuration struct
  - [x] `ServerConfig` - server settings
  - [x] `SqsConfig` - SQS-specific settings
  - [x] `PubsubConfig` - Pub/Sub-specific settings
  - [x] `StorageConfig` - backend configuration
  - [x] `BackendConfig` enum (InMemory, Sqlite)
  - [x] `LoggingConfig` - logging settings
  - [x] `MetricsConfig` - metrics settings
- [ ] Implement configuration loading
  - [ ] TOML file parsing
  - [ ] Environment variable overrides
  - [ ] CLI argument overrides
  - [x] Default configuration
- [ ] Create example `lclq.toml` configuration file
- [ ] Add configuration validation

### 1.6 Logging and Tracing
- [x] Set up tracing infrastructure
  - [x] Initialize tracing subscriber
  - [x] Configure log levels from config
  - [x] Support JSON and text formats
  - [ ] Add file output option
- [x] Add tracing to all major operations
  - [x] Message send/receive
  - [x] Queue create/delete
  - [x] Backend operations
  - [x] Error conditions

---

## Phase 2: AWS SQS Implementation (Weeks 3-4)

### 2.1 SQS Data Types
- [x] Implement SQS-specific types in `src/sqs/types.rs`
  - [x] `SqsAction` enum with all actions
  - [x] `SqsMessage` struct
  - [x] `MessageAttributeValue` struct
  - [x] `QueueAttribute` enum
  - [x] `SqsError` enum
  - [x] MD5 hash calculation for message bodies
  - [x] MD5 hash calculation for attributes

### 2.2 SQS Request/Response Handling
- [x] Implement request parsing in `src/sqs/request.rs`
  - [x] Parse form-encoded POST bodies (query protocol)
  - [x] Parse JSON POST bodies (AWS JSON 1.0 protocol)
  - [x] Extract Action parameter from body or X-Amz-Target header
  - [x] Parse queue URLs
  - [x] Parse message attributes (both protocols)
  - [x] Parse queue attributes (both protocols)
  - [x] Handle batch operations
- [x] Implement response generation in `src/sqs/response.rs`
  - [x] XML response builder
  - [x] JSON response conversion (for AWS JSON 1.0 protocol)
  - [x] Error response formatting
  - [x] Success response formatting
  - [x] Batch response formatting
  - [x] Include request IDs

### 2.3 SQS Action Implementations
- [x] Implement queue management actions
  - [x] `CreateQueue` - create standard or FIFO queue
  - [x] `DeleteQueue` - remove queue
  - [x] `GetQueueUrl` - get URL from queue name
  - [x] `GetQueueAttributes` - retrieve queue attributes
  - [x] `SetQueueAttributes` - modify queue attributes
  - [x] `ListQueues` - list queues with prefix filter
  - [x] `PurgeQueue` - remove all messages
  - [ ] `TagQueue` - add tags to queue
  - [ ] `UntagQueue` - remove tags from queue
  - [ ] `ListQueueTags` - list queue tags
- [x] Implement message operations
  - [x] `SendMessage` - send single message
  - [x] `SendMessageBatch` - send up to 10 messages
  - [x] `ReceiveMessage` - receive messages with long polling
  - [x] `DeleteMessage` - delete single message
  - [x] `DeleteMessageBatch` - delete up to 10 messages
  - [x] `ChangeMessageVisibility` - update visibility timeout
  - [x] `ChangeMessageVisibilityBatch` - batch visibility update

### 2.4 SQS-Specific Features
- [x] Implement FIFO queue support
  - [x] Message group ID handling
  - [x] Message deduplication ID handling
  - [x] Content-based deduplication
  - [x] Sequence number generation
  - [x] Ordering within message groups
- [x] Implement Dead Letter Queue
  - [x] Redrive policy parsing
  - [x] Max receive count tracking
  - [x] Automatic message movement to DLQ
  - [ ] Redrive allow policy
- [x] Implement long polling
  - [x] WaitTimeSeconds parameter support
  - [x] Block until message available (up to 20 seconds)
  - [x] Return empty response after timeout
- [x] Implement delay queues
  - [x] DelaySeconds parameter (0-900)
  - [x] Per-message delay
  - [x] Scheduled delivery
- [x] Implement message retention
  - [x] MessageRetentionPeriod attribute (60-1209600 seconds)
  - [ ] Background task to delete expired messages

### 2.5 SQS HTTP Server
- [x] Implement HTTP server in `src/sqs/server.rs`
  - [x] Set up Axum router
  - [x] Handle POST requests
  - [x] Parse form-encoded bodies
  - [x] Route to action handlers
  - [x] Return XML responses
  - [x] Error handling middleware
- [ ] Implement AWS Signature V4 verification (optional)
  - [ ] Parse Authorization header
  - [ ] Extract signature components
  - [ ] Verify signature (when enabled)
  - [ ] Make verification optional for local development
- [x] Add request logging and metrics
- [x] Add CORS support for browser access
- [ ] Add compression support (gzip)

### 2.6 SQS Integration Tests
- [x] Create integration test suite in `tests/integration/python/`
  - [x] Test with boto3 (Python SDK)
  - [x] Poetry-based project setup
  - [x] 10 comprehensive test functions
  - [ ] Test with AWS SDK for JavaScript v3
  - [ ] Test with AWS SDK for Go v2
  - [ ] Test with AWS SDK for Rust
- [x] Test scenarios (boto3)
  - [x] Create queue and send/receive messages
  - [x] FIFO queue ordering
  - [x] Content-based deduplication
  - [x] Message attributes
  - [x] Batch send/receive operations
  - [x] DeleteMessage and DeleteQueue
  - [x] GetQueueAttributes - retrieve queue configuration
  - [x] SetQueueAttributes - modify queue settings
  - [x] DeleteMessageBatch - batch delete operations
  - [x] ChangeMessageVisibility - single message visibility changes
  - [x] ChangeMessageVisibilityBatch - batch visibility changes
  - [ ] Dead letter queue functionality
  - [ ] Long polling
  - [ ] Delay queues with per-message delays

---

## Phase 3: SQLite Backend (Week 5)

### 3.1 SQLite Schema Design
- [ ] Create schema in `src/storage/sqlite/schema.rs`
  - [ ] `queues` table - queue metadata
  - [ ] `messages` table - message storage
  - [ ] `subscriptions` table - Pub/Sub subscriptions
  - [ ] `subscription_messages` table - subscription state
  - [ ] `deduplication_cache` table - FIFO deduplication
- [ ] Create indexes
  - [ ] Index on `messages.queue_id`
  - [ ] Index on `messages.visibility_timeout`
  - [ ] Index on `messages.message_group_id` for FIFO
  - [ ] Index on `subscription_messages` for lookups

### 3.2 SQLite Backend Implementation
- [ ] Implement `SqliteBackend` in `src/storage/sqlite/mod.rs`
  - [ ] Connection pool setup with sqlx
  - [ ] WAL mode configuration
  - [ ] Busy timeout configuration
  - [ ] Migration system
- [ ] Implement queue operations
  - [ ] `create_queue` - insert into queues table
  - [ ] `get_queue` - select by ID
  - [ ] `delete_queue` - delete with cascade
  - [ ] `list_queues` - query with filters
  - [ ] `update_queue` - update queue config
- [ ] Implement message operations
  - [ ] `send_message` - insert message with transaction
  - [ ] `send_messages` - batch insert
  - [ ] `receive_messages` - select and update visibility
  - [ ] `delete_message` - delete by receipt handle
  - [ ] `change_visibility` - update timeout
- [ ] Implement FIFO support
  - [ ] Deduplication check with cache table
  - [ ] Message group ordering in queries
  - [ ] Sequence number handling
- [ ] Implement maintenance operations
  - [ ] `purge_queue` - delete all messages for queue
  - [ ] `get_stats` - aggregate statistics
  - [ ] `process_expired_visibility` - find and reset expired
  - [ ] Background cleanup of deduplication cache

### 3.3 Migration System
- [ ] Implement migration framework
  - [ ] Version tracking table
  - [ ] Migration runner
  - [ ] Forward-only migrations
  - [ ] Automatic migration on startup
- [ ] Create initial migration
  - [ ] V001: Create all tables
  - [ ] V001: Create all indexes

### 3.4 SQLite Tests
- [ ] Unit tests for SQLite backend
  - [ ] Test all CRUD operations
  - [ ] Test FIFO ordering with SQLite
  - [ ] Test concurrent access
  - [ ] Test transaction rollback
  - [ ] Test migration system
- [ ] Performance tests
  - [ ] Measure throughput
  - [ ] Measure latency
  - [ ] Test with large message counts
  - [ ] Test WAL mode vs. DELETE mode

---

## Phase 4: GCP Pub/Sub gRPC (Weeks 6-7)

### 4.1 Protocol Buffer Definitions
- [ ] Set up proto files in `proto/`
  - [ ] Copy official Google Pub/Sub proto definitions
  - [ ] `google/pubsub/v1/pubsub.proto`
  - [ ] Include dependencies (google.protobuf, etc.)
- [ ] Configure build.rs for proto compilation
  - [ ] Use tonic-build
  - [ ] Generate Rust code from protos
  - [ ] Configure output paths

### 4.2 Pub/Sub Data Types
- [ ] Implement Pub/Sub types in `src/pubsub/types.rs`
  - [ ] `PubsubMessage` struct
  - [ ] `Topic` struct
  - [ ] `Subscription` struct
  - [ ] `PushConfig` struct
  - [ ] `PullResponse` struct
  - [ ] `AckRequest` struct
  - [ ] `PublishRequest`/`PublishResponse`
  - [ ] Resource name parsing/formatting

### 4.3 Publisher Service Implementation
- [ ] Implement Publisher in `src/pubsub/publisher.rs`
  - [ ] `CreateTopic` - create new topic
  - [ ] `UpdateTopic` - modify topic settings
  - [ ] `Publish` - publish messages to topic
  - [ ] `GetTopic` - retrieve topic details
  - [ ] `ListTopics` - list topics in project
  - [ ] `ListTopicSubscriptions` - list subscriptions for topic
  - [ ] `DeleteTopic` - remove topic
  - [ ] `DetachSubscription` - detach subscription from topic

### 4.4 Subscriber Service Implementation
- [ ] Implement Subscriber in `src/pubsub/subscriber.rs`
  - [ ] `CreateSubscription` - create new subscription
  - [ ] `GetSubscription` - retrieve subscription details
  - [ ] `UpdateSubscription` - modify subscription settings
  - [ ] `ListSubscriptions` - list subscriptions in project
  - [ ] `DeleteSubscription` - remove subscription
  - [ ] `Pull` - pull messages from subscription
  - [ ] `StreamingPull` - bidirectional streaming pull
  - [ ] `Acknowledge` - ack received messages
  - [ ] `ModifyAckDeadline` - extend ack deadline
  - [ ] `ModifyPushConfig` - update push configuration
  - [ ] `Seek` - seek to timestamp or snapshot

### 4.5 Pub/Sub-Specific Features
- [ ] Implement message ordering
  - [ ] `OrderingQueue` struct in `src/pubsub/ordering.rs`
  - [ ] Per-ordering-key message queues
  - [ ] Block delivery until ack received
  - [ ] Ordering guarantee enforcement
- [ ] Implement subscription filtering
  - [ ] CEL expression parser (basic subset)
  - [ ] `SubscriptionFilter` struct
  - [ ] Attribute matching
  - [ ] Filter evaluation on pull
- [ ] Implement dead letter topics
  - [ ] Max delivery attempts tracking
  - [ ] Automatic movement to dead letter topic
  - [ ] Dead letter policy configuration
- [ ] Implement message retention
  - [ ] Topic-level retention duration
  - [ ] Background cleanup of expired messages
  - [ ] Retain acked messages (optional)

### 4.6 gRPC Server Setup
- [ ] Implement gRPC server in `src/pubsub/grpc_server.rs`
  - [ ] Set up Tonic server
  - [ ] Register Publisher service
  - [ ] Register Subscriber service
  - [ ] Configure reflection (for debugging)
  - [ ] Configure health checking
  - [ ] Error handling
  - [ ] Logging and tracing
- [ ] Implement streaming support
  - [ ] StreamingPull bidirectional stream
  - [ ] Flow control for streaming
  - [ ] Heartbeat mechanism

### 4.7 Pub/Sub gRPC Integration Tests
- [ ] Create integration test suite
  - [ ] Test with google-cloud-python
  - [ ] Test with @google-cloud/pubsub (Node.js)
  - [ ] Test with cloud.google.com/go/pubsub
- [ ] Test scenarios
  - [ ] Create topic and subscription
  - [ ] Publish and pull messages
  - [ ] Message ordering with ordering keys
  - [ ] Message filtering
  - [ ] Dead letter topic functionality
  - [ ] Streaming pull
  - [ ] Ack deadline modification

---

## Phase 5: GCP Pub/Sub HTTP/REST (Week 8)

### 5.1 REST API Implementation
- [ ] Implement REST handlers in `src/pubsub/rest.rs`
  - [ ] Route configuration with Axum
  - [ ] JSON request/response handling
  - [ ] Resource name parsing from URL paths
  - [ ] Error formatting (Google Cloud error format)

### 5.2 REST Endpoints - Topics
- [ ] Implement topic endpoints
  - [ ] `PUT /v1/projects/{project}/topics/{topic}` - create topic
  - [ ] `GET /v1/projects/{project}/topics/{topic}` - get topic
  - [ ] `DELETE /v1/projects/{project}/topics/{topic}` - delete topic
  - [ ] `GET /v1/projects/{project}/topics` - list topics
  - [ ] `POST /v1/projects/{project}/topics/{topic}:publish` - publish messages

### 5.3 REST Endpoints - Subscriptions
- [ ] Implement subscription endpoints
  - [ ] `PUT /v1/projects/{project}/subscriptions/{subscription}` - create subscription
  - [ ] `GET /v1/projects/{project}/subscriptions/{subscription}` - get subscription
  - [ ] `DELETE /v1/projects/{project}/subscriptions/{subscription}` - delete subscription
  - [ ] `GET /v1/projects/{project}/subscriptions` - list subscriptions
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:pull` - pull messages
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:acknowledge` - ack messages
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:modifyAckDeadline` - modify deadline
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:modifyPushConfig` - modify push config
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:seek` - seek subscription

### 5.4 Push Subscription Support
- [ ] Implement push delivery in `src/pubsub/push.rs`
  - [ ] HTTP client for pushing to endpoints
  - [ ] Push configuration validation (HTTPS required)
  - [ ] Retry logic with exponential backoff
  - [ ] Push authentication (optional)
  - [ ] Background push worker task

### 5.5 HTTP/REST Integration Tests
- [ ] Test HTTP REST API with real SDKs
  - [ ] Test with Python SDK using HTTP transport
  - [ ] Test with Node.js SDK using HTTP transport
- [ ] Verify feature parity with gRPC
  - [ ] All operations work identically
  - [ ] Same error handling
  - [ ] Same message format
- [ ] Test push subscriptions
  - [ ] Set up test HTTP server to receive pushes
  - [ ] Verify message delivery
  - [ ] Test retry behavior

---

## Phase 6: Management & Operations (Week 9)

### 6.1 CLI Tool
- [ ] Implement CLI in `src/main.rs` and `src/cli/`
  - [ ] Use clap for argument parsing
  - [ ] `lclq start` - start server
  - [ ] `lclq queue` - queue management subcommands
    - [ ] `lclq queue list` - list all queues
    - [ ] `lclq queue create <name>` - create queue
    - [ ] `lclq queue delete <name>` - delete queue
    - [ ] `lclq queue purge <name>` - purge queue
    - [ ] `lclq queue stats <name>` - show queue stats
  - [ ] `lclq health` - health check
  - [ ] `lclq stats` - overall statistics
  - [ ] `lclq config` - show current configuration
- [ ] CLI output formatting
  - [ ] Human-readable table format
  - [ ] JSON output option
  - [ ] Color support with termcolor

### 6.2 Admin API
- [ ] Implement admin API in `src/server/admin.rs`
  - [ ] `GET /health` - health check endpoint
  - [ ] `GET /stats` - system statistics
  - [ ] `GET /queues` - list all queues
  - [ ] `GET /queues/{id}` - queue details
  - [ ] `DELETE /queues/{id}` - delete queue
  - [ ] `POST /queues/{id}/purge` - purge queue
  - [ ] `GET /config` - current configuration
- [ ] Admin server setup
  - [ ] Separate port (9000)
  - [ ] Optional authentication
  - [ ] CORS support

### 6.3 Metrics and Monitoring
- [ ] Implement Prometheus metrics in `src/metrics/`
  - [ ] `Metrics` struct with all metrics
  - [ ] Counter metrics
    - [ ] `lclq_messages_sent_total` by queue_id and dialect
    - [ ] `lclq_messages_received_total` by queue_id and dialect
    - [ ] `lclq_messages_deleted_total` by queue_id and dialect
    - [ ] `lclq_messages_to_dlq_total` by queue_id
    - [ ] `lclq_backend_errors_total` by backend and operation
  - [ ] Histogram metrics
    - [ ] `lclq_send_latency_seconds` by backend
    - [ ] `lclq_receive_latency_seconds` by backend
  - [ ] Gauge metrics
    - [ ] `lclq_queue_depth` by queue_id
    - [ ] `lclq_in_flight_messages` by queue_id
    - [ ] `lclq_queue_count` by backend
    - [ ] `lclq_active_connections` by dialect
- [ ] Metrics HTTP server
  - [ ] `GET /metrics` endpoint (port 9090)
  - [ ] Prometheus exposition format
- [ ] Add metric recording throughout codebase

### 6.4 Graceful Shutdown
- [ ] Implement shutdown handling in `src/server/shutdown.rs`
  - [ ] Listen for SIGTERM/SIGINT
  - [ ] Stop accepting new connections
  - [ ] Complete in-flight requests
  - [ ] Flush metrics
  - [ ] Close database connections
  - [ ] Configurable timeout (default 30s)

---

## Phase 7: Performance & Polish (Week 10)

### 7.1 Performance Optimization
- [ ] Profile application with perf/flamegraph
  - [ ] Identify hot paths
  - [ ] Optimize critical sections
- [ ] Optimize memory backend
  - [ ] Reduce allocations
  - [ ] Optimize data structures
  - [ ] Consider using slab allocator
- [ ] Optimize SQLite backend
  - [ ] Tune connection pool size
  - [ ] Optimize queries
  - [ ] Add prepared statement caching
  - [ ] Batch operations where possible
- [ ] Optimize serialization
  - [ ] Profile serde usage
  - [ ] Consider zero-copy where possible
  - [ ] Optimize XML/JSON generation

### 7.2 Load Testing
- [ ] Create load test suite in `benches/`
  - [ ] Use criterion for benchmarking
  - [ ] Benchmark send operations
  - [ ] Benchmark receive operations
  - [ ] Benchmark full send-receive cycles
  - [ ] Test with various message sizes
- [ ] Multi-client load testing
  - [ ] Use tool like k6 or wrk
  - [ ] Test 1,000+ concurrent connections
  - [ ] Measure throughput and latency
  - [ ] Test both SQS and Pub/Sub endpoints
- [ ] Verify performance targets
  - [ ] Memory backend: >10,000 msg/sec
  - [ ] SQLite backend: >1,000 msg/sec
  - [ ] P50 latency <1ms (memory)
  - [ ] P99 latency <10ms (memory)
  - [ ] Startup time <100ms

### 7.3 Security Hardening
- [ ] Implement optional TLS support
  - [ ] TLS configuration in config file
  - [ ] Certificate loading
  - [ ] TLS for HTTP servers (SQS, Pub/Sub HTTP)
  - [ ] TLS for gRPC server
- [ ] Implement authentication (optional)
  - [ ] API key authentication
  - [ ] Token-based authentication
  - [ ] AWS Signature V4 verification
- [ ] Input validation
  - [ ] Review all validation functions
  - [ ] Add fuzzing tests
  - [ ] Prevent injection attacks
  - [ ] Limit request sizes

### 7.4 Docker and Container Support
- [ ] Create Dockerfile
  - [ ] Multi-stage build
  - [ ] Minimal base image (debian:bookworm-slim)
  - [ ] Copy binary from builder
  - [ ] Expose all ports
  - [ ] Set up ENTRYPOINT and CMD
- [ ] Create docker-compose.yml
  - [ ] Service definition
  - [ ] Port mappings
  - [ ] Volume mounts
  - [ ] Environment variables
- [ ] Test Docker image
  - [ ] Build and run
  - [ ] Test all endpoints
  - [ ] Verify persistence with volumes

### 7.5 Release Preparation
- [ ] Version tagging and releases
  - [ ] Semantic versioning
  - [ ] Git tags for releases
  - [ ] GitHub releases
- [ ] Build release binaries
  - [ ] Linux x86_64
  - [ ] Linux ARM64
  - [ ] macOS x86_64
  - [ ] macOS ARM64 (Apple Silicon)
  - [ ] Windows x86_64
- [ ] Create release artifacts
  - [ ] Compressed binaries
  - [ ] Docker images on Docker Hub
  - [ ] SHA256 checksums

---

## Testing

### Unit Tests
- [ ] Test coverage for all modules
  - [ ] Target >90% coverage
  - [ ] Use cargo-tarpaulin to measure
- [ ] Core module tests
  - [ ] Message router tests
  - [ ] Visibility manager tests
  - [ ] DLQ handler tests
- [ ] Storage backend tests
  - [ ] In-memory backend tests
  - [ ] SQLite backend tests
  - [ ] Test all CRUD operations
  - [ ] Test concurrent access
- [ ] Validation function tests
  - [ ] Queue name validation
  - [ ] Topic name validation
  - [ ] Message size validation
  - [ ] Attribute validation
- [ ] Configuration tests
  - [ ] TOML parsing tests
  - [ ] Default configuration tests
  - [ ] Validation tests

### Integration Tests
- [ ] SQS integration tests
  - [ ] Test with multiple SDK languages
  - [ ] Python (boto3)
  - [ ] JavaScript (AWS SDK v3)
  - [ ] Go (AWS SDK v2)
  - [ ] Rust (AWS SDK)
  - [ ] Test all major operations
  - [ ] Standard queues
  - [ ] FIFO queues
  - [ ] Dead letter queues
  - [ ] Batch operations
- [ ] Pub/Sub integration tests
  - [ ] Test with multiple SDK languages
  - [ ] Python (google-cloud-python)
  - [ ] JavaScript (@google-cloud/pubsub)
  - [ ] Go (cloud.google.com/go/pubsub)
  - [ ] Test gRPC protocol
  - [ ] Test HTTP/REST protocol
  - [ ] Test message ordering
  - [ ] Test filtering
  - [ ] Test push subscriptions
- [ ] End-to-end tests
  - [ ] Multi-queue scenarios
  - [ ] Mixed SQS and Pub/Sub usage
  - [ ] High-load scenarios
  - [ ] Failure recovery

### Compatibility Tests
- [ ] Create compatibility test matrix
  - [ ] Test against AWS SQS (real service)
  - [ ] Test against GCP Pub/Sub (real service)
  - [ ] Verify identical behavior
- [ ] SDK version compatibility
  - [ ] Test with latest SDK versions
  - [ ] Test with minimum supported versions
  - [ ] Document compatible SDK versions

### Performance Tests
- [ ] Benchmarks in `benches/`
  - [ ] Send message benchmark
  - [ ] Receive message benchmark
  - [ ] Round-trip benchmark
  - [ ] Batch operations benchmark
- [ ] Load tests
  - [ ] Throughput testing
  - [ ] Latency testing
  - [ ] Concurrent connection testing
  - [ ] Memory usage testing
- [ ] Stress tests
  - [ ] Maximum queue count
  - [ ] Maximum message count
  - [ ] Maximum message size
  - [ ] Long-running stability

### Chaos/Failure Tests
- [ ] Simulate failure scenarios
  - [ ] Backend failures
  - [ ] Database corruption
  - [ ] Network failures
  - [ ] Out of memory
  - [ ] Disk full
- [ ] Recovery tests
  - [ ] Crash and restart
  - [ ] Message recovery
  - [ ] Queue state recovery

---

## Documentation

### User Documentation
- [ ] Quick Start Guide
  - [ ] Installation instructions
  - [ ] First queue example (SQS)
  - [ ] First topic/subscription example (Pub/Sub)
  - [ ] Configuration basics
- [ ] API Reference
  - [ ] SQS API documentation
  - [ ] Pub/Sub API documentation
  - [ ] Admin API documentation
- [ ] Configuration Guide
  - [ ] All configuration options
  - [ ] Configuration file format
  - [ ] Environment variables
  - [ ] CLI arguments
  - [ ] Examples for common scenarios
- [ ] Migration Guide
  - [ ] Migrating from AWS SQS to lclq
  - [ ] Migrating from GCP Pub/Sub to lclq
  - [ ] Code examples for each SDK
- [ ] Troubleshooting Guide
  - [ ] Common issues and solutions
  - [ ] Debugging tips
  - [ ] Performance tuning
  - [ ] FAQ

### Developer Documentation
- [ ] Architecture Overview
  - [ ] System architecture diagram
  - [ ] Component descriptions
  - [ ] Data flow diagrams
- [ ] Backend Development Guide
  - [ ] How to implement a new backend
  - [ ] StorageBackend trait documentation
  - [ ] Testing requirements
- [ ] Contributing Guidelines
  - [ ] Code style guide
  - [ ] Pull request process
  - [ ] Testing requirements
  - [ ] Documentation requirements
- [ ] API Extension Guide
  - [ ] How to add new SQS actions
  - [ ] How to add new Pub/Sub methods
  - [ ] How to add new configuration options

### Code Documentation
- [ ] Rustdoc comments for all public APIs
  - [ ] Module-level documentation
  - [ ] Struct/enum documentation
  - [ ] Function documentation
  - [ ] Example code in docs
- [ ] Generate and publish rustdoc
  - [ ] Set up docs.rs integration
  - [ ] Configure documentation build

### Examples
- [ ] Create example applications in `examples/`
  - [ ] Simple SQS producer/consumer (Python)
  - [ ] Simple SQS producer/consumer (Node.js)
  - [ ] Simple SQS producer/consumer (Go)
  - [ ] FIFO queue example
  - [ ] Dead letter queue example
  - [ ] Pub/Sub publisher/subscriber (Python)
  - [ ] Pub/Sub publisher/subscriber (Node.js)
  - [ ] Pub/Sub publisher/subscriber (Go)
  - [ ] Message ordering example
  - [ ] Message filtering example
  - [ ] Push subscription example

---

## Deployment & Distribution

### Package Distribution
- [ ] Publish to crates.io
  - [ ] Prepare package metadata
  - [ ] Add keywords and categories
  - [ ] Publish releases
- [ ] Homebrew formula (macOS)
  - [ ] Create formula
  - [ ] Submit to homebrew-core or create tap
- [ ] APT repository (Debian/Ubuntu)
  - [ ] Create .deb packages
  - [ ] Set up repository
- [ ] RPM repository (RedHat/Fedora)
  - [ ] Create .rpm packages
  - [ ] Set up repository
- [ ] Chocolatey package (Windows)
  - [ ] Create package
  - [ ] Submit to chocolatey.org

### Container Distribution
- [ ] Docker Hub
  - [ ] Publish official images
  - [ ] Tag versioned releases
  - [ ] Maintain latest tag
- [ ] GitHub Container Registry
  - [ ] Alternative registry
  - [ ] Automatic builds on release

### Kubernetes Support
- [ ] Create Helm chart
  - [ ] Chart structure
  - [ ] Configurable values
  - [ ] Service definitions
  - [ ] StatefulSet for persistence
  - [ ] ConfigMap for configuration
- [ ] Publish to Helm repository
- [ ] Create Kubernetes manifests
  - [ ] Deployment YAML
  - [ ] Service YAML
  - [ ] ConfigMap YAML

---

## Community & Marketing

### Repository Setup
- [ ] GitHub repository settings
  - [ ] Issue templates
  - [ ] Pull request template
  - [ ] Code of conduct
  - [ ] Security policy
  - [ ] Funding options
- [ ] Enable GitHub features
  - [ ] Discussions
  - [ ] Projects (roadmap)
  - [ ] Wiki (if needed)

### Community Building
- [ ] Create communication channels
  - [ ] Discord server or Slack workspace
  - [ ] GitHub Discussions
  - [ ] Mailing list (optional)
- [ ] Write blog post announcing project
- [ ] Create demo video/screencast
- [ ] Submit to relevant platforms
  - [ ] Hacker News
  - [ ] Reddit (r/rust, r/programming)
  - [ ] Dev.to
  - [ ] Lobsters

### Success Tracking
- [ ] Monitor metrics
  - [ ] GitHub stars
  - [ ] Downloads from crates.io
  - [ ] Docker pulls
  - [ ] Issue/PR activity
- [ ] Collect user feedback
  - [ ] Survey users
  - [ ] Monitor GitHub issues
  - [ ] Track feature requests

---

## Long-Term Roadmap (Post-Launch)

### Additional Backends
- [ ] Redis backend
  - [ ] Design schema
  - [ ] Implement StorageBackend
  - [ ] Performance optimization
  - [ ] Documentation
- [ ] PostgreSQL backend
  - [ ] Design schema
  - [ ] Implement StorageBackend
  - [ ] Handle high concurrency
  - [ ] Documentation

### Advanced Features
- [ ] Web UI for management
  - [ ] Queue browser
  - [ ] Message inspector
  - [ ] Real-time metrics dashboard
  - [ ] Configuration editor
- [ ] Multi-region simulation
  - [ ] Region awareness
  - [ ] Cross-region replication (simulated)
  - [ ] Regional endpoints
- [ ] Plugin system
  - [ ] Plugin API design
  - [ ] Dynamic loading
  - [ ] Example plugins
- [ ] Advanced monitoring
  - [ ] Distributed tracing
  - [ ] OpenTelemetry integration
  - [ ] Custom exporters

### Additional Cloud Dialects
- [ ] Azure Service Bus support
  - [ ] Protocol implementation
  - [ ] SDK compatibility
- [ ] RabbitMQ protocol support
  - [ ] AMQP implementation
  - [ ] RabbitMQ management API

---

## Success Criteria Tracking

### Functional Metrics
- [ ] AWS SQS compatibility
  - [ ] CreateQueue, DeleteQueue, GetQueueUrl, ListQueues
  - [ ] SendMessage, SendMessageBatch
  - [ ] ReceiveMessage, DeleteMessage, DeleteMessageBatch
  - [ ] ChangeMessageVisibility
  - [ ] GetQueueAttributes, SetQueueAttributes
  - [ ] PurgeQueue
  - [ ] FIFO queues with ordering
  - [ ] Dead letter queues
  - [ ] Message attributes
- [ ] GCP Pub/Sub compatibility
  - [ ] CreateTopic, GetTopic, DeleteTopic, ListTopics
  - [ ] Publish
  - [ ] CreateSubscription, GetSubscription, DeleteSubscription
  - [ ] Pull, StreamingPull
  - [ ] Acknowledge, ModifyAckDeadline
  - [ ] Message ordering
  - [ ] Message filtering
  - [ ] Push subscriptions
- [ ] SDK compatibility
  - [ ] Python (boto3, google-cloud-python) ✓
  - [ ] JavaScript (AWS SDK v3, @google-cloud/pubsub) ✓
  - [ ] Go (AWS SDK v2, cloud.google.com/go/pubsub) ✓
  - [ ] Ruby (AWS SDK v3, google-cloud-pubsub) ✓
  - [ ] Java (AWS SDK v2, google-cloud-pubsub) ✓
- [ ] Both protocols work for Pub/Sub
  - [ ] gRPC fully functional ✓
  - [ ] HTTP/REST fully functional ✓
  - [ ] Feature parity verified ✓

### Performance Metrics
- [ ] Memory backend throughput >10,000 msg/sec ✓
- [ ] SQLite backend throughput >1,000 msg/sec ✓
- [ ] P99 latency <10ms for memory backend ✓
- [ ] Support 1,000+ concurrent connections ✓
- [ ] Startup time <100ms ✓

### Quality Metrics
- [ ] >90% code coverage ✓
- [ ] Zero critical bugs ✓
- [ ] Complete API documentation ✓
- [ ] Comprehensive user guide ✓

### Adoption Metrics
- [ ] 1,000+ GitHub stars in first year
- [ ] 50+ contributors
- [ ] 10+ production adoptions
- [ ] Active community engagement

---

**Note:** This TODO list should be regularly updated as implementation progresses. Mark items as completed and add new items as needed based on learnings and feedback.
