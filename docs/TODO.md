# lclq Implementation TODO

**Status:** Ready for Implementation
**Last Updated:** October 2025 (Phase 5: REST API Infrastructure - COMPLETE ✅)

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
  - [x] Install cargo-audit for security auditing (via CI/CD)
  - [x] Install cargo-deny for dependency checking (via CI/CD)
- [x] Configure CI/CD ✅ COMPLETE
  - [x] Create `.github/workflows/test.yml` (main CI pipeline)
  - [x] Add build workflow for Linux, macOS, Windows
  - [x] Add test workflow (unit + integration tests)
  - [x] Add clippy and rustfmt checks
  - [x] Create `.github/workflows/security.yml` (security audit)
  - [x] Create `.github/workflows/release.yml` (automated releases)
  - [x] Add Docker build verification
  - [x] Add integration tests (Python, JavaScript, Go)
  - [x] Create deny.toml for cargo-deny configuration
  - [x] Create PR template and issue templates
  - [x] Create CODEOWNERS file

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
  - [x] `receive_messages` - Automatic expired message requeuing
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
- [x] Add tests for in-memory backend ✅ COMPLETE
  - [x] Test basic send/receive flow
  - [x] Test FIFO ordering
  - [x] Test deduplication
  - [x] Test visibility timeout expiration
  - [x] Test concurrent access
  - [x] Test capacity limits and eviction

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

## Phase 2: AWS SQS Implementation (Weeks 3-4) ✅ COMPLETE

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
  - [x] JSON array expansion for boto3 compatibility
- [x] Implement response generation in `src/sqs/response.rs`
  - [x] XML response builder
  - [x] JSON response conversion (for AWS JSON 1.0 protocol)
  - [x] Error response formatting
  - [x] Success response formatting
  - [x] Batch response formatting
  - [x] Include request IDs
  - [x] XML entity unescaping for JSON responses
  - [x] Message attributes extraction in ReceiveMessage JSON responses

### 2.3 SQS Action Implementations
- [x] Implement queue management actions
  - [x] `CreateQueue` - create standard or FIFO queue
  - [x] `DeleteQueue` - remove queue
  - [x] `GetQueueUrl` - get URL from queue name
  - [x] `GetQueueAttributes` - retrieve queue attributes
  - [x] `SetQueueAttributes` - modify queue attributes
  - [x] `ListQueues` - list queues with prefix filter
  - [x] `PurgeQueue` - remove all messages
  - [x] `TagQueue` - add tags to queue
  - [x] `UntagQueue` - remove tags from queue
  - [x] `ListQueueTags` - list queue tags
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
  - [x] Redrive allow policy
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
  - [x] Background task to delete expired messages

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
  - [x] Test with AWS SDK for JavaScript v3
  - [x] Test with AWS SDK for Go v2
  - [x] Test with AWS SDK for Rust
- [x] Test scenarios (boto3) - **7/7 advanced tests passing**
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
  - [x] Dead letter queue - receive count tracking and visibility timeout
  - [x] RedriveAllowPolicy - DLQ access control
  - [x] Long polling - WaitTimeSeconds support
  - [x] Delay queues - queue-level and per-message delays
  - [x] Queue attributes - GetQueueAttributes and SetQueueAttributes
- [x] Test scenarios (JavaScript SDK v3) - **7/7 tests passing**
  - [x] Basic Queue Operations - create, send, receive, delete
  - [x] Message Attributes - send and receive with custom attributes
  - [x] FIFO Queue - ordering verification
  - [x] Batch Operations - SendMessageBatch, DeleteMessageBatch
  - [x] Queue Attributes - GetQueueAttributes, SetQueueAttributes
  - [x] Change Message Visibility - single and batch operations
  - [x] Delay Queue - queue-level delays with timing verification
- [x] Test scenarios (Go SDK v2) - **7/7 tests passing**
  - [x] Basic Queue Operations - create, send, receive, delete
  - [x] Message Attributes - send and receive with custom attributes
  - [x] FIFO Queue - ordering verification
  - [x] Batch Operations - SendMessageBatch, DeleteMessageBatch
  - [x] Queue Attributes - GetQueueAttributes, SetQueueAttributes
  - [x] Change Message Visibility - single and batch operations
  - [x] Delay Queue - queue-level delays with timing verification
- [x] Test scenarios (Rust SDK) - **7/7 tests passing**
  - [x] Basic Queue Operations - create, send, receive, delete
  - [x] Message Attributes - send and receive with custom attributes
  - [x] FIFO Queue - ordering verification
  - [x] Batch Operations - SendMessageBatch, DeleteMessageBatch
  - [x] Queue Attributes - GetQueueAttributes, SetQueueAttributes
  - [x] Change Message Visibility - single and batch operations
  - [x] Delay Queue - queue-level delays with timing verification

---

## Phase 3: SQLite Backend (Week 5) ✅ COMPLETE

### 3.1 SQLite Schema Design
- [x] Create schema in `migrations/20250101000000_initial_schema.sql`
  - [x] `queues` table - queue metadata
  - [x] `messages` table - message storage
  - [x] `subscriptions` table - Pub/Sub subscriptions
  - [x] `subscription_messages` table - subscription state
  - [x] `deduplication_cache` table - FIFO deduplication
- [x] Create indexes
  - [x] Index on `messages.queue_id`
  - [x] Index on `messages.visibility_timeout`
  - [x] Index on `messages.message_group_id` for FIFO
  - [x] Index on `subscription_messages` for lookups

### 3.2 SQLite Backend Implementation
- [x] Implement `SqliteBackend` in `src/storage/sqlite/mod.rs`
  - [x] Connection pool setup with sqlx
  - [x] WAL mode configuration
  - [x] Busy timeout configuration
  - [x] Migration system
- [x] Implement queue operations
  - [x] `create_queue` - insert into queues table
  - [x] `get_queue` - select by ID
  - [x] `delete_queue` - delete with cascade
  - [x] `list_queues` - query with filters
  - [x] `update_queue` - update queue config
- [x] Implement message operations
  - [x] `send_message` - insert message with transaction
  - [x] `send_messages` - batch insert
  - [x] `receive_messages` - select and update visibility
  - [x] `delete_message` - delete by receipt handle
  - [x] `change_visibility` - update timeout
- [x] Implement FIFO support
  - [x] Deduplication check with cache table
  - [x] Message group ordering in queries
  - [x] Sequence number handling
- [x] Implement maintenance operations
  - [x] `purge_queue` - delete all messages for queue
  - [x] `get_stats` - aggregate statistics
  - [x] `process_expired_visibility` - find and reset expired
  - [x] Background cleanup of deduplication cache
  - [x] CleanupManager for automatic background maintenance

### 3.3 Migration System
- [x] Implement migration framework
  - [x] Using sqlx::migrate! macro
  - [x] Migration runner
  - [x] Forward-only migrations
  - [x] Automatic migration on startup
- [x] Create initial migration
  - [x] V001: Create all tables (20250101000000_initial_schema.sql)
  - [x] V001: Create all indexes

### 3.4 SQLite Tests
- [x] Integration tests for SQLite backend
  - [x] Test all CRUD operations
  - [x] Test FIFO ordering with SQLite
  - [x] Test message send/receive/delete cycle
  - [x] Test visibility timeout changes
  - [x] Test queue purge operation
  - [x] 5 comprehensive integration tests passing
- [ ] Performance tests
  - [ ] Measure throughput
  - [ ] Measure latency
  - [ ] Test with large message counts
  - [ ] Test WAL mode vs. DELETE mode

---

## Phase 4: GCP Pub/Sub gRPC (Weeks 6-7) ✅ CORE COMPLETE

### 4.1 Protocol Buffer Definitions ✅
- [x] Set up proto files in `proto/`
  - [x] Copy official Google Pub/Sub proto definitions
  - [x] `google/pubsub/v1/pubsub.proto`
  - [x] Include dependencies (google.protobuf, etc.)
- [x] Configure build.rs for proto compilation
  - [x] Use tonic-build
  - [x] Generate Rust code from protos
  - [x] Configure output paths

### 4.2 Pub/Sub Data Types ✅
- [x] Implement Pub/Sub types in `src/pubsub/types.rs`
  - [x] `ResourceName` enum (Topic, Subscription, Snapshot)
  - [x] Resource name parsing/formatting
  - [x] Validation functions (topic ID, subscription ID, project ID)
  - [x] Message size validation
  - [x] Generated proto types in `src/pubsub/proto`

### 4.3 Publisher Service Implementation ✅
- [x] Implement Publisher in `src/pubsub/publisher.rs` (8/8 methods)
  - [x] `CreateTopic` - create new topic
  - [x] `UpdateTopic` - modify topic settings
  - [x] `Publish` - publish messages to topic with attributes and ordering keys
  - [x] `GetTopic` - retrieve topic details
  - [x] `ListTopics` - list topics in project (with filtering)
  - [x] `ListTopicSubscriptions` - list subscriptions for topic
  - [x] `DeleteTopic` - remove topic
  - [x] `DetachSubscription` - detach subscription from topic (stub)

### 4.4 Subscriber Service Implementation ✅
- [x] Implement Subscriber in `src/pubsub/subscriber.rs` (9/15 methods core, 6 stubs)
  - [x] `CreateSubscription` - create new subscription with full config
  - [x] `GetSubscription` - retrieve subscription details
  - [x] `UpdateSubscription` - modify subscription settings (stub)
  - [x] `ListSubscriptions` - list subscriptions in project (with filtering)
  - [x] `DeleteSubscription` - remove subscription
  - [x] `Pull` - pull messages from subscription with visibility timeout
  - [x] `StreamingPull` - bidirectional streaming pull with message ordering ✅ COMPLETE
  - [x] `Acknowledge` - ack received messages
  - [x] `ModifyAckDeadline` - extend ack deadline
  - [x] `ModifyPushConfig` - update push configuration (stub)
  - [x] `GetSnapshot`, `ListSnapshots`, `CreateSnapshot`, `UpdateSnapshot`, `DeleteSnapshot` (stubs)
  - [x] `Seek` - seek to timestamp or snapshot (stub)

### 4.5 Pub/Sub-Specific Features
- [x] Message ordering support
  - [x] Ordering keys in messages (ordering_key field)
  - [x] Maps to message_group_id in storage
  - [x] Enable message ordering in subscriptions
  - [x] Client-side ordering configuration support
- [ ] Advanced subscription filtering
  - [ ] CEL expression parser (basic subset)
  - [ ] `SubscriptionFilter` struct
  - [ ] Attribute matching
  - [ ] Filter evaluation on pull
- [x] Dead letter topics (via existing DLQ infrastructure)
  - [x] Max delivery attempts tracking (via receive_count)
  - [x] Dead letter policy configuration
  - [x] Automatic movement to dead letter topic
- [x] Message retention
  - [x] Topic-level retention duration
  - [x] Subscription-level retention
  - [x] Background cleanup via CleanupManager

### 4.6 gRPC Server Setup ✅
- [x] Implement gRPC server in `src/pubsub/grpc_server.rs`
  - [x] Set up Tonic server with Axum transport
  - [x] Register Publisher service
  - [x] Register Subscriber service
  - [x] Graceful shutdown support
  - [x] Error handling and status codes
  - [x] Logging and tracing
  - [x] Integrated into main application (port 8085)
- [ ] Advanced streaming support (StreamingPull)
  - [ ] StreamingPull bidirectional stream (stub implemented - needs full implementation)
  - [ ] Flow control for streaming
  - [ ] Heartbeat mechanism

### 4.8 StreamingPull Implementation (Future Enhancement)
**Status:** Stub exists, full implementation pending
**Priority:** Medium (nice-to-have for advanced use cases)

#### 4.8.1 Core StreamingPull Logic
- [ ] Replace stub with full bidirectional stream handler in `src/pubsub/subscriber.rs`
  - [ ] Implement `streaming_pull` method with proper stream handling
  - [ ] Handle incoming `StreamingPullRequest` stream from client
  - [ ] Send outgoing `StreamingPullResponse` stream to client
  - [ ] Maintain stream state per connection
  - [ ] Handle graceful stream closure
  - [ ] Handle abrupt disconnections

#### 4.8.2 Request Processing
- [ ] Parse and handle `StreamingPullRequest` messages
  - [ ] Extract and validate `subscription` field
  - [ ] Process `ack_ids` for acknowledging messages
  - [ ] Process `modify_deadline_seconds` for deadline extensions
  - [ ] Process `modify_deadline_ack_ids` for specific message deadlines
  - [ ] Handle `client_id` for connection tracking
  - [ ] Handle `max_outstanding_messages` for flow control
  - [ ] Handle `max_outstanding_bytes` for flow control
  - [ ] Handle `stream_ack_deadline_seconds` for stream-level deadline

#### 4.8.3 Response Streaming
- [ ] Implement continuous message streaming
  - [ ] Create background task to poll subscription for new messages
  - [ ] Send `StreamingPullResponse` with `received_messages`
  - [ ] Batch messages efficiently (respect max_outstanding limits)
  - [ ] Handle empty responses (no messages available)
  - [ ] Implement backpressure when client limits reached
  - [ ] Track which messages are in-flight per stream

#### 4.8.4 Flow Control
- [ ] Implement client-specified flow control
  - [ ] Track outstanding message count per stream
  - [ ] Track outstanding bytes per stream
  - [ ] Block sending when `max_outstanding_messages` reached
  - [ ] Block sending when `max_outstanding_bytes` reached
  - [ ] Resume sending when client acknowledges messages
  - [ ] Default limits if not specified by client

#### 4.8.5 Stream Connection Management
- [ ] Track active streaming connections
  - [ ] Create `StreamConnection` struct with connection state
  - [ ] Store connection metadata (client_id, subscription_id)
  - [ ] Maintain map of active streams in subscriber service
  - [ ] Clean up on disconnect (return messages to available state)
  - [ ] Handle connection lease renewal
  - [ ] Implement connection timeout/heartbeat

#### 4.8.6 Message Lease Management
- [ ] Implement streaming-specific message leasing
  - [ ] Lock messages when sent via stream
  - [ ] Track message lease per connection
  - [ ] Extend lease based on `stream_ack_deadline_seconds`
  - [ ] Release lease on acknowledgment
  - [ ] Release lease on disconnect (make available for other clients)
  - [ ] Implement lease expiration and redelivery

#### 4.8.7 Heartbeat & Keep-Alive
- [ ] Implement heartbeat mechanism
  - [ ] Send periodic empty responses to detect dead connections
  - [ ] Client should respond with empty requests
  - [ ] Detect connection failure (no response to heartbeat)
  - [ ] Configure heartbeat interval (default: 30 seconds)
  - [ ] Close stream if heartbeat fails

#### 4.8.8 Error Handling
- [ ] Handle stream errors gracefully
  - [ ] Invalid subscription errors
  - [ ] Client exceeding flow control limits
  - [ ] Malformed requests
  - [ ] Backend errors (storage failures)
  - [ ] Network errors and retries
  - [ ] Return proper gRPC status codes

#### 4.8.9 Concurrency & Thread Safety
- [ ] Ensure thread-safe stream handling
  - [ ] Use tokio channels for message passing
  - [ ] Use Arc/RwLock for shared stream state
  - [ ] Handle concurrent acknowledgments
  - [ ] Handle concurrent deadline modifications
  - [ ] Prevent race conditions in message delivery

#### 4.8.10 Integration with Storage Backend
- [ ] Extend storage backend for streaming support
  - [ ] Add method to subscribe to new messages (watch pattern)
  - [ ] Implement efficient polling or notification mechanism
  - [ ] Consider using tokio::sync::watch or broadcast channels
  - [ ] Optimize for low latency message delivery
  - [ ] Batch fetch messages efficiently

#### 4.8.11 Testing
- [ ] Unit tests for StreamingPull
  - [ ] Test basic streaming flow (send/receive/ack)
  - [ ] Test acknowledgments via stream
  - [ ] Test deadline modifications via stream
  - [ ] Test flow control limits
  - [ ] Test stream closure scenarios
  - [ ] Test error conditions
- [ ] Integration tests
  - [ ] Test with google-cloud-pubsub Python SDK (streaming subscriber)
  - [ ] Test with @google-cloud/pubsub Node.js SDK (streaming)
  - [ ] Test concurrent streaming clients
  - [ ] Test connection drops and recovery
  - [ ] Test message redelivery after disconnect
  - [ ] Test long-running streams (hours)

#### 4.8.12 Performance & Optimization
- [ ] Optimize streaming performance
  - [ ] Profile message delivery latency
  - [ ] Minimize allocations in hot path
  - [ ] Batch operations where possible
  - [ ] Tune flow control defaults
  - [ ] Benchmark throughput vs. synchronous Pull
  - [ ] Memory usage profiling

#### 4.8.13 Documentation
- [ ] Document StreamingPull usage
  - [ ] Add examples to docs/quickstart.md
  - [ ] Document flow control parameters
  - [ ] Document heartbeat behavior
  - [ ] Add troubleshooting guide
  - [ ] Update API reference
  - [ ] Add comparison: StreamingPull vs. Pull

**Estimated Effort:** 2-3 weeks
**Dependencies:** None (can be implemented independently)
**Benefits:**
- Lower latency for message delivery
- More efficient for high-throughput scenarios
- Better resource utilization (persistent connections)
- Closer parity with production Pub/Sub behavior

### 4.7 Pub/Sub gRPC Integration Tests ✅ COMPLETE
- [x] Create integration test suite
  - [x] Test with google-cloud-pubsub (Python) - **15/15 tests passing**
  - [x] Test with @google-cloud/pubsub (Node.js) - **16/16 tests passing**
  - [x] Test with cloud.google.com/go/pubsub (Go) - **13/13 tests passing** ✅
- [x] Test scenarios (Python SDK)
  - [x] Create topic and subscription
  - [x] Get topic and subscription
  - [x] List topics and subscriptions
  - [x] Delete topic and subscription
  - [x] Publish single message
  - [x] Publish with attributes
  - [x] Publish and pull messages (full cycle)
  - [x] Message ordering with ordering keys
  - [x] Message attributes handling
  - [x] Acknowledge messages and verify deletion
  - [x] Modify ack deadline
- [x] Test scenarios (JavaScript SDK)
  - [x] All topic management operations
  - [x] All subscription management operations
  - [x] Message publishing (single, with attributes, with ordering)
  - [x] Pull messages with v1 client
  - [x] Message ordering verification
  - [x] Acknowledge and modify ack deadline
- [x] Bug fixes and improvements
  - [x] Fixed list_topics project filtering
  - [x] Fixed list_subscriptions project filtering
  - [x] Fixed modify_ack_deadline receipt handle issue
  - [x] Fixed message ordering client configuration
  - [x] Fixed StreamingPull message ordering via subscription_properties flag
- [x] Test scenarios (Go SDK) - **13/13 tests passing**
  - [x] Create and get topics
  - [x] List topics
  - [x] Delete topics
  - [x] Create and get subscriptions
  - [x] List subscriptions
  - [x] Delete subscriptions
  - [x] Publish single message
  - [x] Publish with attributes
  - [x] Pull messages (via StreamingPull) ✅
  - [x] Message ordering (via StreamingPull) ✅
  - [x] Modify ack deadline (via StreamingPull) ✅

---

## Phase 5: GCP Pub/Sub HTTP/REST (Week 8) ✅ COMPLETE

### 5.1 REST API Implementation ✅ COMPLETE
- [x] **Create REST API infrastructure in `src/pubsub/rest.rs`** (~1030 lines)
  - [x] Route configuration with Axum (all 11 endpoints)
  - [x] JSON request/response handling with serde
  - [x] Complete type definitions (Topic, Subscription, Message, etc.)
  - [x] Error formatting (Google Cloud error format)
  - [x] Base64 encoding/decoding for message data
  - [x] Graceful shutdown support
  - [x] Fixed Axum 0.8 route syntax (`:param` → `{param}`)
  - [x] Fixed route conflicts with action handlers
  - [x] Optional name fields for SDK compatibility

### 5.2 REST Endpoints - Topics ✅ COMPLETE
- [x] Implement topic endpoint handlers
  - [x] `PUT /v1/projects/{project}/topics/{topic}` - create topic
  - [x] `GET /v1/projects/{project}/topics/{topic}` - get topic
  - [x] `DELETE /v1/projects/{project}/topics/{topic}` - delete topic
  - [x] `GET /v1/projects/{project}/topics` - list topics
  - [x] `POST /v1/projects/{project}/topics/{topic}:publish` - publish messages

### 5.3 REST Endpoints - Subscriptions ✅ COMPLETE
- [x] Implement subscription endpoint handlers
  - [x] `PUT /v1/projects/{project}/subscriptions/{subscription}` - create subscription
  - [x] `GET /v1/projects/{project}/subscriptions/{subscription}` - get subscription
  - [x] `DELETE /v1/projects/{project}/subscriptions/{subscription}` - delete subscription
  - [x] `GET /v1/projects/{project}/subscriptions` - list subscriptions
  - [x] `POST /v1/projects/{project}/subscriptions/{subscription}:pull` - pull messages
  - [x] `POST /v1/projects/{project}/subscriptions/{subscription}:acknowledge` - ack messages
  - [x] `POST /v1/projects/{project}/subscriptions/{subscription}:modifyAckDeadline` - modify deadline
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:modifyPushConfig` - modify push config (future)
  - [ ] `POST /v1/projects/{project}/subscriptions/{subscription}:seek` - seek subscription (future)

### 5.4 Push Subscription Support
- [ ] Implement push delivery in `src/pubsub/push.rs` (deferred to future phase)
  - [ ] HTTP client for pushing to endpoints
  - [ ] Push configuration validation (HTTPS required)
  - [ ] Retry logic with exponential backoff
  - [ ] Push authentication (optional)
  - [ ] Background push worker task

### 5.5 HTTP/REST Integration Tests ✅ COMPLETE (Python)
- [x] Test HTTP REST API with real SDKs
  - [x] Test with Python SDK using HTTP transport - **9/9 tests passing**
  - [ ] Test with Node.js SDK using HTTP transport (pending)
- [x] Verify feature parity with gRPC
  - [x] All core operations work identically
  - [x] Same error handling
  - [x] Same message format
- [x] Python SDK test scenarios (google-cloud-pubsub v2.23.0 with REST transport)
  - [x] Create and get topic
  - [x] List topics
  - [x] Publish and pull messages
  - [x] Create and get subscription
  - [x] List subscriptions
  - [x] Modify ack deadline
  - [x] Message ordering
  - [x] Empty pull handling
  - [x] Batch publish operations
- [ ] Test push subscriptions (deferred to future phase)
  - [ ] Set up test HTTP server to receive pushes
  - [ ] Verify message delivery
  - [ ] Test retry behavior

---

## Phase 6: Management & Operations (Week 9) ✅ COMPLETE

### 6.1 CLI Tool ✅ COMPLETE
- [x] Implement CLI in `src/main.rs` and `src/cli/`
  - [x] Use clap for argument parsing
  - [x] `lclq start` - start server
  - [x] `lclq queue` - queue management subcommands
    - [x] `lclq queue list` - list all queues
    - [x] `lclq queue create <name>` - create queue
    - [x] `lclq queue delete <name>` - delete queue
    - [x] `lclq queue purge <name>` - purge queue
    - [x] `lclq queue stats <name>` - show queue stats
  - [x] `lclq health` - health check
  - [x] `lclq stats` - overall statistics
  - [x] `lclq config` - show current configuration (stub implementation)
- [x] CLI output formatting
  - [x] Human-readable table format
  - [x] JSON output option
  - [x] Color support with colored crate
- [x] Connect CLI commands to Admin API

### 6.2 Admin API ✅ COMPLETE
- [x] Implement admin API in `src/server/admin.rs`
  - [x] `GET /health` - health check endpoint
  - [x] `GET /stats` - system statistics
  - [x] `GET /queues` - list all queues
  - [x] `GET /queues/{name}` - queue details
  - [x] `POST /queues` - create queue
  - [x] `DELETE /queues/{name}` - delete queue
  - [x] `POST /queues/{name}/purge` - purge queue
  - [ ] `GET /config` - current configuration
- [x] Admin server setup
  - [x] Separate port (9000)
  - [ ] Optional authentication
  - [x] CORS support

### 6.3 Metrics and Monitoring ✅ COMPLETE (Infrastructure)
- [x] Implement Prometheus metrics in `src/metrics/`
  - [x] `Metrics` struct with all metrics
  - [x] Counter metrics
    - [x] `lclq_messages_sent_total` by queue_id, queue_name, and dialect
    - [x] `lclq_messages_received_total` by queue_id, queue_name, and dialect
    - [x] `lclq_messages_deleted_total` by queue_id, queue_name, and dialect
    - [x] `lclq_messages_to_dlq_total` by queue_id and queue_name
    - [x] `lclq_backend_errors_total` by backend and operation
    - [x] `lclq_api_requests_total` by api, method, endpoint, and status
  - [x] Histogram metrics
    - [x] `lclq_send_latency_seconds` by backend
    - [x] `lclq_receive_latency_seconds` by backend
    - [x] `lclq_api_latency_seconds` by api and endpoint
  - [x] Gauge metrics
    - [x] `lclq_queue_depth` by queue_id and queue_name
    - [x] `lclq_in_flight_messages` by queue_id and queue_name
    - [x] `lclq_queue_count` total
    - [x] `lclq_active_connections` by dialect
- [x] Metrics HTTP server
  - [x] `GET /metrics` endpoint (port 9090)
  - [x] Prometheus exposition format
- [ ] Add metric recording throughout codebase (future enhancement)

### 6.4 Graceful Shutdown ✅ COMPLETE
- [x] Implement shutdown handling in `src/server/shutdown.rs`
  - [x] Listen for SIGTERM/SIGINT
  - [x] Stop accepting new connections
  - [x] Complete in-flight requests
  - [ ] Flush metrics
  - [ ] Close database connections
  - [x] Configurable timeout (default 30s)
- [x] Integration with all servers
  - [x] SQS HTTP server shutdown coordination
  - [x] Admin API server shutdown coordination
  - [x] Metrics server shutdown coordination
  - [x] Broadcast shutdown signal to all servers
  - [x] Wait for graceful shutdown with timeout
- [x] Testing
  - [x] SIGTERM signal handling verified
  - [x] All servers shut down gracefully
  - [x] Proper logging at each shutdown step

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

### 7.2 Load Testing ✅ BENCHMARKS COMPLETE
- [x] **Create load test suite in `benches/`**
  - [x] Use criterion for benchmarking
  - [x] Benchmark send operations (100B to 100KB)
  - [x] Benchmark receive operations (1 to 100 messages)
  - [x] Benchmark full send-receive cycles
  - [x] Test with various message sizes
  - [x] Benchmark batch operations (1 to 1000 messages)
  - [x] Benchmark concurrent operations (1 to 100 threads)
  - [x] Benchmark message operations (serialization, hashing, encoding)
  - [x] Document results in `docs/benchmarks.md`
- [ ] Multi-client load testing (external tools)
  - [ ] Use tool like k6 or wrk
  - [ ] Test 1,000+ concurrent connections
  - [ ] Measure throughput and latency
  - [ ] Test both SQS and Pub/Sub endpoints
- [x] **Verify performance targets** ✅
  - [x] Memory backend: >10,000 msg/sec ✅ **ACHIEVED: 1.82M msg/sec (182x target)**
  - [ ] SQLite backend: >1,000 msg/sec (not yet benchmarked)
  - [x] P50 latency <1ms (memory) ✅ **ACHIEVED: <10µs (100x better)**
  - [x] P99 latency <10ms (memory) ✅ **ACHIEVED: <35µs (286x better)**
  - [ ] Startup time <100ms (not yet measured)

### 7.3 Security Hardening
- [x] **Critical security fixes (Phase 3 hardening)**
  - [x] Fix SQL injection vulnerabilities in list_queues
  - [x] Add HMAC-SHA256 signatures to receipt handles
  - [x] Implement constant-time signature verification
  - [x] Wrap batch operations in database transactions
  - [x] Add receipt handle forgery protection
  - [x] Prevent timing attacks on signature verification
- [ ] Implement optional TLS support
  - [ ] TLS configuration in config file
  - [ ] Certificate loading
  - [ ] TLS for HTTP servers (SQS, Pub/Sub HTTP)
  - [ ] TLS for gRPC server
- [ ] Implement authentication (optional)
  - [ ] API key authentication
  - [ ] Token-based authentication
  - [ ] AWS Signature V4 verification
- [x] Input validation
  - [x] Review all validation functions
  - [ ] Add fuzzing tests
  - [x] Prevent injection attacks (SQL injection fixed)
  - [ ] Limit request sizes

### 7.4 Docker and Container Support ✅ COMPLETE
- [x] Create Dockerfile
  - [x] Multi-stage build
  - [x] Minimal base image (debian:bookworm-slim)
  - [x] Copy binary from builder
  - [x] Expose all ports
  - [x] Set up ENTRYPOINT and CMD
- [x] Create docker-compose.yml
  - [x] Service definition
  - [x] Port mappings
  - [x] Volume mounts
  - [x] Environment variables
- [x] Create .dockerignore for optimized build context
- [x] Create lclq.toml.example configuration file
- [x] Add bind-address CLI argument and environment variable support
- [x] Update all servers to accept bind_address parameter
- [x] Test Docker image
  - [x] Build and run
  - [x] Test all endpoints (SQS, Admin API, Metrics)
  - [x] Verify persistence with volumes (SQLite backend)

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
- [x] Test coverage for all modules ✅ IN PROGRESS (28.27% → target 90%)
  - [x] Use cargo-tarpaulin to measure
  - [ ] Target >90% coverage (current: 28.27%)
- [x] Core module tests ✅ PARTIAL
  - [ ] Message router tests
  - [x] Visibility manager tests (3/21 lines)
  - [x] DLQ handler tests (7/27 lines)
- [x] Storage backend tests ✅ COMPLETE
  - [x] In-memory backend tests (7 comprehensive tests, 174/285 lines - 61%)
  - [x] SQLite backend tests (5 comprehensive tests, 303/448 lines - 68%)
  - [x] Test all CRUD operations
  - [x] Test concurrent access
- [x] Validation function tests ✅ PARTIAL (35/56 lines)
  - [x] Queue name validation
  - [x] Topic name validation
  - [x] Message size validation
  - [x] Attribute validation
- [ ] Configuration tests
  - [ ] TOML parsing tests
  - [ ] Default configuration tests
  - [ ] Validation tests
- [x] SQS handler tests ✅ COMPLETE (16 tests, 250/810 lines - 31%)
  - [x] CreateQueue (standard & FIFO)
  - [x] GetQueueUrl
  - [x] DeleteQueue
  - [x] ListQueues (with prefix filtering)
  - [x] SendMessage (basic & with attributes)
  - [x] SendMessageBatch
  - [x] ReceiveMessage
  - [x] DeleteMessage
  - [x] PurgeQueue
  - [x] GetQueueAttributes
  - [x] SetQueueAttributes

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
- [x] SDK version compatibility (Pub/Sub)
  - [x] Python google-cloud-pubsub v2.23.0 - 15/15 tests passing ✓
  - [x] JavaScript @google-cloud/pubsub v4.9.0 - 16/16 tests passing ✓
  - [ ] Go cloud.google.com/go/pubsub
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
- [x] **Quick Start Guide** ✅ COMPLETE
  - [x] Installation instructions (docs/quickstart.md)
  - [x] First queue example (SQS) - Python, JavaScript, Go, Rust
  - [x] First topic/subscription example (Pub/Sub) - Python, JavaScript, Go
  - [x] Configuration basics
  - [x] Common patterns (DLQ, long polling)
  - [x] Troubleshooting section
- [x] **README.md** ✅ UPDATED
  - [x] Emphasize local dev and CI/CD use case
  - [x] Performance benchmarks (1.82M msg/sec, 182x faster)
  - [x] Side-by-side code comparisons (production vs local)
  - [x] CI/CD integration examples (GitHub Actions, GitLab CI, Docker Compose)
  - [x] Updated project status (Phases 0-4, 6-7 complete)
  - [x] Comprehensive FAQ
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
- [ ] Troubleshooting Guide (partially complete in quickstart.md)
  - [x] Common issues and solutions
  - [x] Debugging tips
  - [ ] Performance tuning
  - [x] FAQ (in README.md)

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
- [x] **Create example applications in `examples/`** ✅ COMPLETE
  - [x] Simple SQS producer/consumer (Python) - examples/sqs/python/
  - [ ] Simple SQS producer/consumer (Node.js)
  - [ ] Simple SQS producer/consumer (Go)
  - [ ] FIFO queue example
  - [ ] Dead letter queue example
  - [x] Pub/Sub publisher/subscriber (Python) - examples/pubsub/python/
  - [ ] Pub/Sub publisher/subscriber (Node.js)
  - [ ] Pub/Sub publisher/subscriber (Go)
  - [ ] Message ordering example
  - [ ] Message filtering example
  - [ ] Push subscription example
  - [x] Examples README with overview and instructions

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
- [x] AWS SQS compatibility ✓
  - [x] CreateQueue, DeleteQueue, GetQueueUrl, ListQueues ✓
  - [x] SendMessage, SendMessageBatch ✓
  - [x] ReceiveMessage, DeleteMessage, DeleteMessageBatch ✓
  - [x] ChangeMessageVisibility ✓
  - [x] GetQueueAttributes, SetQueueAttributes ✓
  - [x] PurgeQueue ✓
  - [x] FIFO queues with ordering ✓
  - [x] Dead letter queues ✓
  - [x] Message attributes ✓
- [x] GCP Pub/Sub compatibility (Core gRPC) ✓
  - [x] CreateTopic, GetTopic, DeleteTopic, ListTopics ✓
  - [x] Publish (with attributes and ordering keys) ✓
  - [x] CreateSubscription, GetSubscription, DeleteSubscription, ListSubscriptions ✓
  - [x] Pull (synchronous message retrieval) ✓
  - [ ] StreamingPull (bidirectional streaming - stub)
  - [x] Acknowledge, ModifyAckDeadline ✓
  - [x] Message ordering ✓
  - [ ] Advanced subscription filtering (CEL)
  - [ ] Push subscriptions
- [x] SDK compatibility ✓
  - [x] Python (boto3 - SQS 7/7, google-cloud-pubsub 15/15) ✓
  - [x] JavaScript (AWS SDK v3 - SQS 7/7, @google-cloud/pubsub 16/16) ✓
  - [x] Go (AWS SDK v2 - SQS 7/7) ✓
  - [x] Rust (AWS SDK - SQS 7/7) ✓
  - [ ] Ruby (AWS SDK v3, google-cloud-pubsub)
  - [ ] Java (AWS SDK v2, google-cloud-pubsub)
- [x] Pub/Sub protocols
  - [x] gRPC fully functional (31 tests passing) ✓
  - [ ] HTTP/REST implementation pending
  - [ ] Feature parity verification pending

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
