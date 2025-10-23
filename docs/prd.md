# Product Requirements Document: lclq (Local Cloud Queue)

**Version:** 1.0  
**Date:** October 2025  
**Status:** Draft

## Executive Summary

lclq (Local Cloud Queue) is a versatile, lightweight queue system designed to provide local implementations of popular cloud messaging services. It offers pluggable storage backends and supports multiple cloud queue dialects, enabling developers to test and develop cloud-native applications locally without incurring cloud costs or requiring internet connectivity.

## 1. Product Overview

### 1.1 Vision Statement
To provide developers with a seamless local development experience for cloud queue systems, eliminating the friction between local development and cloud deployment while maintaining API compatibility with major cloud providers.

### 1.2 Problem Statement
Developers building applications that use cloud queue services face several challenges:
- **Development Costs:** Using actual cloud services during development incurs unnecessary costs
- **Internet Dependency:** Requires constant internet connection for development and testing
- **Speed:** Network latency slows down development iteration cycles
- **Testing Complexity:** Integration testing with cloud services is complex and time-consuming
- **Multi-Cloud Support:** Applications targeting multiple cloud providers need separate testing environments

### 1.3 Solution
lclq provides a unified local queue system that:
- Implements accurate API dialects for GCP Pub/Sub and AWS SQS
- Offers multiple storage backends for different use cases
- Runs entirely locally with zero cloud dependencies
- Maintains API compatibility for seamless migration to production

## 2. Goals and Objectives

### 2.1 Primary Goals
- **P0:** Provide 100% API-compatible local implementations of AWS SQS and GCP Pub/Sub
- **P0:** Support pluggable storage backends (in-memory and SQLite)
- **P0:** Enable zero-configuration startup for rapid prototyping
- **P1:** Achieve sub-millisecond message processing latency
- **P1:** Support all major message patterns (FIFO, standard queues, topics/subscriptions)

### 2.2 Success Metrics
- API compatibility coverage: >95% of commonly used endpoints
- Message throughput: >10,000 messages/second (in-memory backend)
- Startup time: <100ms
- Memory footprint: <50MB for basic operations
- Developer adoption: 1,000+ GitHub stars within first year

## 3. User Personas

### 3.1 Primary Personas

**Backend Developer (Primary)**
- Builds microservices using cloud queue systems
- Needs local testing environment that mirrors production
- Values fast iteration and debugging capabilities
- Frustrated by cloud service costs during development

**DevOps Engineer (Secondary)**
- Sets up CI/CD pipelines with integration tests
- Needs reliable, deterministic queue behavior for testing
- Values configuration flexibility and monitoring capabilities

**Full-Stack Developer (Tertiary)**
- Occasionally works with queue systems
- Needs simple, zero-configuration setup
- Values clear documentation and examples

### 3.2 User Stories

**As a backend developer:**
- I want to run my application locally without cloud dependencies
- I want to switch between AWS SQS and GCP Pub/Sub without code changes
- I want to inspect queue contents during debugging
- I want to simulate error conditions and edge cases

**As a DevOps engineer:**
- I want to run integration tests in CI without cloud services
- I want to configure queue behavior for different test scenarios
- I want to export metrics for monitoring during tests

## 4. Functional Requirements

### 4.1 Core Queue Operations

#### 4.1.1 Message Operations
- **Send Message:** Support single and batch message sending
- **Receive Message:** Pull-based message consumption with configurable batch size
- **Delete Message:** Explicit message acknowledgment
- **Message Visibility:** Configurable visibility timeout for in-flight messages
- **Message Attributes:** Support for message metadata and custom attributes

#### 4.1.2 Queue Management
- **Create Queue/Topic:** Dynamic queue/topic creation
- **Delete Queue/Topic:** Clean queue removal with message purging
- **List Queues/Topics:** Enumerate available queues with filtering
- **Queue Configuration:** Set and modify queue attributes

### 4.2 AWS SQS Dialect

#### 4.2.1 Queue Types
- **Standard Queues:** Best-effort ordering, at-least-once delivery
- **FIFO Queues:** Strict ordering, exactly-once processing
- **Dead Letter Queues:** Automatic message retry with configurable redrive policy

#### 4.2.2 SQS-Specific Features
- **Message Deduplication:** Content-based and ID-based deduplication for FIFO queues
- **Message Groups:** Ordered message processing within groups
- **Long Polling:** Reduce empty responses with configurable wait time
- **Delay Queues:** Message delivery delay (0-900 seconds)
- **Message Retention:** Configurable retention period (1 minute - 14 days)

#### 4.2.3 API Compatibility
- **HTTP Interface:** Full AWS SQS HTTP/REST API support
- **Protocol:** HTTP/1.1 and HTTP/2 support
- **Authentication:** AWS Signature Version 4 (simulated)
- Support for AWS SDK v2 and v3
- IAM permission simulation (simplified)
- Request signing compatibility (optional)

### 4.3 GCP Pub/Sub Dialect

#### 4.3.1 Core Concepts
- **Topics:** Message categories for publish/subscribe pattern
- **Subscriptions:** Durable message streams with independent consumption
- **Push/Pull Delivery:** Both subscription models supported

#### 4.3.2 Pub/Sub-Specific Features
- **Message Ordering:** Per-key message ordering
- **Message Filtering:** Subscription-level attribute filtering
- **Acknowledgment Deadline:** Configurable ack deadline with extension support
- **Dead Letter Topics:** Failed message handling
- **Snapshots:** Point-in-time subscription state (simplified)
- **Message Retention:** Topic-level retention configuration

#### 4.3.3 API Compatibility
- **Dual Protocol Support:**
  - **gRPC Interface:** Full gRPC API implementation for high-performance communication
  - **HTTP/REST Interface:** Complete REST API for compatibility with HTTP-only clients
- **Protocol Details:**
  - gRPC: Support for streaming and unary calls
  - HTTP: JSON-based REST API with full feature parity
  - Automatic protocol detection based on client request
- Google Cloud Client Library compatibility
- Service account simulation (simplified)
- Both protocols share the same backend and provide identical functionality

### 4.4 Storage Backends

#### 4.4.1 In-Memory Backend
- **Use Case:** Development, testing, ephemeral workloads
- **Characteristics:**
  - Highest performance (sub-millisecond latency)
  - No persistence
  - Configurable memory limits
  - LRU eviction for memory management

#### 4.4.2 SQLite Backend
- **Use Case:** Persistent local development, integration testing
- **Characteristics:**
  - ACID compliance
  - Persistent storage
  - Concurrent access support
  - Automatic schema migration
  - Configurable WAL mode for performance

#### 4.4.3 Backend Interface
```go
type Backend interface {
    // Queue operations
    CreateQueue(ctx context.Context, config QueueConfig) error
    DeleteQueue(ctx context.Context, queueID string) error
    ListQueues(ctx context.Context, filter Filter) ([]Queue, error)
    
    // Message operations
    SendMessage(ctx context.Context, queueID string, msg Message) (MessageID, error)
    ReceiveMessages(ctx context.Context, queueID string, opts ReceiveOpts) ([]Message, error)
    DeleteMessage(ctx context.Context, queueID string, receipt string) error
    
    // Maintenance
    Purge(ctx context.Context, queueID string) error
    GetStats(ctx context.Context, queueID string) (QueueStats, error)
}
```

### 4.5 Configuration and Management

#### 4.5.1 Configuration Options
- **Global Settings:**
  - Default backend selection
  - Port configuration for each dialect
  - Protocol settings:
    - HTTP port for SQS (default: 4566)
    - HTTP port for Pub/Sub REST (default: 8085)
    - gRPC port for Pub/Sub (default: 8086)
    - HTTP/2 and HTTP/1.1 support toggles
  - TLS/SSL settings for both HTTP and gRPC
  - Logging level and output
  
- **Per-Queue Settings:**
  - Message retention period
  - Maximum message size
  - Visibility timeout defaults
  - Dead letter queue configuration

#### 4.5.2 Management Interface
- **REST API:** Administrative endpoints for queue management
- **CLI Tool:** Command-line interface for operations
- **Web UI:** Optional browser-based management console
- **Metrics Export:** Prometheus-compatible metrics endpoint

## 5. Non-Functional Requirements

### 5.1 Performance Requirements
- **Throughput:** 
  - In-memory: >10,000 msgs/sec
  - SQLite: >1,000 msgs/sec
- **Latency:**
  - P50: <1ms (in-memory)
  - P99: <10ms (in-memory)
- **Concurrent Connections:** Support 1,000+ concurrent clients
- **Message Size:** Support up to 256KB per message (configurable)

### 5.2 Scalability Requirements
- **Horizontal Scaling:** Multiple lclq instances can run independently
- **Queue Limits:** Support 10,000+ queues per instance
- **Message Limits:** 1M+ messages per queue (backend-dependent)

### 5.3 Reliability Requirements
- **Availability:** 99.9% uptime for long-running instances
- **Data Durability:** No message loss for persistent backends
- **Crash Recovery:** Automatic recovery with persistent backends
- **Graceful Shutdown:** Complete in-flight operations

### 5.4 Security Requirements
- **Authentication:** Optional API key or token-based auth
- **Authorization:** Basic ACL support for multi-tenant scenarios
- **Encryption:** Optional TLS for client connections
- **Audit Logging:** Configurable operation logging

### 5.5 Compatibility Requirements
- **Platform Support:** Linux, macOS, Windows
- **Architecture Support:** x86_64, ARM64
- **Container Support:** Docker image with minimal base
- **Language SDKs:** Official AWS and GCP SDKs should work without modification

### 5.6 Network Protocol Requirements
- **HTTP/REST Support:**
  - HTTP/1.1 for maximum compatibility
  - HTTP/2 for improved performance
  - Chunked transfer encoding
  - Gzip compression support
  - WebSocket support for long polling (future)
- **gRPC Support:**
  - gRPC over HTTP/2
  - Protocol Buffers v3
  - Streaming RPCs (server, client, and bidirectional)
  - gRPC health checking protocol
  - gRPC reflection for debugging
- **Protocol Selection:**
  - Automatic detection based on Content-Type and request format
  - Simultaneous multi-protocol support on different ports
  - Protocol-specific connection pooling
  - Consistent behavior across protocols

## 6. Technical Architecture

### 6.1 System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Client Applications                  │
└─────────────┬───────────────────────┬───────────────────────┘
              │                       │
              ▼                       ▼
┌─────────────────────┐ ┌─────────────────────────────────────┐
│  AWS SQS SDK/CLI    │ │    GCP Pub/Sub SDK/CLI              │
└─────────────────────┘ └─────────────────────────────────────┘
              │                       │
              ▼                       ▼
┌─────────────────────────────────────────────────────────────┐
│                      lclq Service Layer                      │
├─────────────────────┬───────────────────────────────────────┤
│   SQS Dialect       │      Pub/Sub Dialect                  │
│   HTTP/REST         │      gRPC + HTTP/REST                 │
│   Handler           │      Dual Protocol Handler            │
└─────────────────────┴───────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Core Queue Engine                         │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │ Message  │ │  Queue   │ │   DLQ    │ │   ACK    │      │
│  │ Router   │ │ Manager  │ │ Handler  │ │ Tracker  │      │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘      │
└─────────────────────────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Storage Backend Interface                  │
├─────────────────────┬───────────────────────────────────────┤
│   In-Memory         │        SQLite                          │
│   Backend           │        Backend                         │
└─────────────────────┴───────────────────────────────────────┘
```

### 6.2 Component Specifications

#### 6.2.1 Dialect Handlers
- **Protocol Support:**
  - SQS: HTTP/REST with XML response format
  - Pub/Sub: Dual protocol (gRPC and HTTP/REST with JSON)
  - Automatic protocol detection and routing
- **Request Processing:**
  - Parse and validate dialect-specific requests
  - Transform between dialect format and internal representation
  - Generate dialect-compliant responses
  - Handle authentication/authorization simulation
- **Protocol-Specific Features:**
  - HTTP: Connection keep-alive, compression support
  - gRPC: Bidirectional streaming, protocol buffers
  - Unified error handling across protocols

#### 6.2.2 Core Queue Engine
- Message routing and delivery
- Visibility timeout management
- Dead letter queue processing
- Message deduplication (FIFO)
- Subscription management (Pub/Sub)

#### 6.2.3 Storage Abstraction Layer
- Unified interface for all backends
- Transaction management
- Connection pooling
- Query optimization

### 6.3 Data Models

#### 6.3.1 Core Entities
```sql
-- Queue/Topic
CREATE TABLE queues (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    type TEXT NOT NULL, -- 'sqs_standard', 'sqs_fifo', 'pubsub_topic'
    config JSON NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Messages
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    queue_id TEXT NOT NULL,
    body TEXT NOT NULL,
    attributes JSON,
    visibility_timeout TIMESTAMP,
    receive_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (queue_id) REFERENCES queues(id)
);

-- Subscriptions (Pub/Sub)
CREATE TABLE subscriptions (
    id TEXT PRIMARY KEY,
    topic_id TEXT NOT NULL,
    name TEXT NOT NULL,
    push_endpoint TEXT,
    filter JSON,
    ack_deadline INTEGER DEFAULT 10,
    FOREIGN KEY (topic_id) REFERENCES queues(id)
);
```

## 7. Implementation Roadmap

### 7.1 Phase 1: MVP (Months 1-2)
- Core queue engine with in-memory backend
- Basic AWS SQS dialect (standard queues only)
- CLI tool for basic operations
- Unit test coverage >80%

### 7.2 Phase 2: SQS Complete (Months 2-3)
- FIFO queue support
- Dead letter queues
- SQLite backend
- Full AWS SDK compatibility testing

### 7.3 Phase 3: Pub/Sub (Months 3-4)
- GCP Pub/Sub dialect implementation
- Topic/subscription model
- Push and pull delivery modes
- Message filtering

### 7.4 Phase 4: Production Ready (Months 4-5)
- Performance optimization
- Web UI for management
- Comprehensive documentation
- Example applications
- Docker image and Helm chart

### 7.5 Phase 5: Advanced Features (Months 5-6)
- Additional backends (Redis, PostgreSQL)
- Multi-region simulation
- Advanced monitoring and tracing
- Plugin system for extensions

## 8. Testing Strategy

### 8.1 Test Levels
- **Unit Tests:** Component-level testing with mocks
- **Integration Tests:** Full stack testing with real SDKs
- **Compatibility Tests:** Validation against cloud services
- **Performance Tests:** Load and stress testing
- **Chaos Tests:** Failure scenario validation

### 8.2 Test Coverage Goals
- Code coverage: >90%
- API compatibility: 100% of documented endpoints
- Protocol coverage: Both HTTP/REST and gRPC interfaces fully tested
- SDK compatibility: AWS SDK (Go, Python, Java), GCP SDK (Go, Python, Java)
- Cross-protocol validation: Ensure identical behavior across HTTP and gRPC

## 9. Documentation Requirements

### 9.1 User Documentation
- Quick start guide
- API reference for each dialect
- Configuration guide
- Migration guide from cloud services
- Troubleshooting guide

### 9.2 Developer Documentation
- Architecture overview
- Backend development guide
- Contributing guidelines
- API extension guide

## 10. Success Criteria

### 10.1 Launch Criteria
- [ ] 100% compatibility with basic SQS operations
- [ ] 100% compatibility with basic Pub/Sub operations
- [ ] Performance meets requirements
- [ ] Documentation complete
- [ ] 90% test coverage

### 10.2 Adoption Metrics
- 1,000+ GitHub stars in first year
- 50+ contributors
- 10+ production adoptions
- Active community support

## 11. Risks and Mitigations

### 11.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|---------|------------|
| API compatibility issues | Medium | High | Continuous compatibility testing against real services |
| Performance degradation with scale | Medium | Medium | Regular performance testing, profiling |
| SQLite concurrent write limitations | High | Low | Document limitations, suggest alternatives for high throughput |
| Memory leaks in long-running instances | Low | High | Memory profiling, automated testing |

### 11.2 Market Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|---------|------------|
| Low adoption rate | Medium | High | Strong documentation, community engagement |
| Competition from cloud provider tools | Medium | Medium | Focus on multi-cloud support, simplicity |
| Maintenance burden | Medium | Medium | Build active contributor community |

## 12. Appendices

### A. Glossary
- **DLQ:** Dead Letter Queue
- **FIFO:** First In, First Out
- **ACK:** Acknowledgment
- **WAL:** Write-Ahead Logging
- **LRU:** Least Recently Used

### B. References
- [AWS SQS Documentation](https://docs.aws.amazon.com/sqs/)
- [GCP Pub/Sub Documentation](https://cloud.google.com/pubsub/docs)
- [SQLite Documentation](https://www.sqlite.org/docs.html)

### C. Alternative Approaches Considered
1. **Single dialect implementation:** Rejected for limited use case coverage
2. **Cloud service proxy:** Rejected due to continued cloud dependency
3. **Container-based approach:** Considered but adds complexity for users

---

**Document History:**
- v1.0 - Initial draft (October 2025)
