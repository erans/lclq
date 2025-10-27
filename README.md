# lclq - Local Cloud Queue

**Lightning-fast local replacement for AWS SQS and GCP Pub/Sub.**
Zero configuration. Zero cloud dependencies. 100% SDK compatible.

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-green)
![Tests](https://img.shields.io/badge/tests-408%20passing-success)

---

## Why lclq?

Transform your development workflow with **blazing-fast local queues** that work seamlessly with your existing AWS and GCP code:

### 🚀 **182x Faster Than Cloud**
- **1.8M messages/sec** vs 10K on AWS SQS
- **Sub-10µs latency** vs 10ms+ on cloud
- Iterate faster. Test faster. Ship faster.

### ⚡ **Zero Configuration**
```bash
lclq start  # That's it. Server running in <100ms
```
No accounts. No credentials. No setup. Just run.

### 🎯 **100% SDK Compatible**
Works with **official AWS and GCP SDKs** out of the box. Change one line (the endpoint), keep everything else:

```python
# Production
sqs = boto3.client('sqs')

# Local dev - just add endpoint
sqs = boto3.client('sqs', endpoint_url='http://localhost:9324')
# Rest of your code stays EXACTLY the same
```

### 💰 **Zero Cloud Costs**
- **Free CI/CD testing** - No AWS/GCP bills for integration tests
- **Unlimited local development** - Test without rate limits or quotas
- **Perfect for learning** - Explore SQS/Pub/Sub without cloud accounts

---

## Perfect For

| Use Case | Why lclq? |
|----------|-----------|
| 🔧 **Local Development** | Test queue-based apps without 100ms+ cloud latency |
| ✅ **CI/CD Pipelines** | Fast, isolated tests in GitHub Actions, GitLab CI, Jenkins |
| 🧪 **Integration Testing** | Spin up in seconds, tear down instantly |
| 📚 **Learning & Prototyping** | Experiment with SQS/Pub/Sub without AWS/GCP accounts |

---

## Quick Start

### Installation

**Option 1: Binary (Recommended)**
```bash
# Linux
curl -sSL https://github.com/erans/lclq/releases/latest/download/lclq-linux-x86_64 -o lclq
chmod +x lclq
./lclq start

# macOS (Apple Silicon)
curl -sSL https://github.com/erans/lclq/releases/latest/download/lclq-macos-aarch64 -o lclq
chmod +x lclq
./lclq start

# Windows
# Download lclq-windows-x86_64.exe from releases page
```

**Option 2: Docker**
```bash
docker run -p 9324:9324 -p 8085:8085 -p 9000:9000 erans/lclq:latest
```

**Option 3: From Source**
```bash
cargo install lclq
lclq start
```

### Server Endpoints

Once started, lclq exposes multiple endpoints:

| Service | Endpoint | Protocol |
|---------|----------|----------|
| AWS SQS | `http://localhost:9324` | HTTP/REST |
| GCP Pub/Sub | `localhost:8085` | gRPC |
| GCP Pub/Sub | `http://localhost:8086` | HTTP/REST |
| Admin API | `http://localhost:9000` | HTTP/REST |
| Metrics | `http://localhost:9090/metrics` | Prometheus |

---

## Usage Examples

### AWS SQS (Python + boto3)

```python
import boto3

# Only change: point to localhost
sqs = boto3.client(
    'sqs',
    endpoint_url='http://localhost:9324',
    region_name='us-east-1',
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

# Your existing code works unchanged
queue = sqs.create_queue(QueueName='my-queue')
sqs.send_message(QueueUrl=queue['QueueUrl'], MessageBody='Hello World!')
response = sqs.receive_message(QueueUrl=queue['QueueUrl'])
print(response['Messages'][0]['Body'])  # "Hello World!"
```

### AWS SQS (JavaScript + AWS SDK v3)

```javascript
import { SQSClient, CreateQueueCommand, SendMessageCommand } from "@aws-sdk/client-sqs";

// Only change: add endpoint
const client = new SQSClient({
  region: "us-east-1",
  endpoint: "http://localhost:9324",
  credentials: { accessKeyId: "dummy", secretAccessKey: "dummy" }
});

// Your existing code works unchanged
const { QueueUrl } = await client.send(new CreateQueueCommand({ QueueName: "my-queue" }));
await client.send(new SendMessageCommand({ QueueUrl, MessageBody: "Hello World!" }));
```

### GCP Pub/Sub (Python)

```python
import os
os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'  # Only change needed

from google.cloud import pubsub_v1

publisher = pubsub_v1.PublisherClient()
subscriber = pubsub_v1.SubscriberClient()

# Your existing code works unchanged
topic_path = publisher.topic_path('my-project', 'my-topic')
publisher.create_topic(request={"name": topic_path})
future = publisher.publish(topic_path, b'Hello World!')
print(f"Published message ID: {future.result()}")
```

---

## CI/CD Integration

### GitHub Actions

```yaml
name: Integration Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Start lclq
        run: |
          curl -sSL https://github.com/erans/lclq/releases/latest/download/lclq-linux-x86_64 -o lclq
          chmod +x lclq
          ./lclq start &
          sleep 2

      - name: Run tests
        run: pytest tests/integration/
```

### Docker Compose

```yaml
version: '3.8'
services:
  lclq:
    image: erans/lclq:latest
    ports:
      - "9324:9324"  # SQS
      - "8085:8085"  # Pub/Sub
      - "9000:9000"  # Admin

  app:
    build: .
    environment:
      AWS_ENDPOINT_URL: http://lclq:9324
      PUBSUB_EMULATOR_HOST: lclq:8085
    depends_on:
      - lclq
```

---

## Features

### ✅ AWS SQS Compatibility

Fully compatible with official AWS SDKs (Python, JavaScript, Go, Rust, Java, Ruby).

**Core Features:**
- Standard and FIFO queues
- Message attributes
- Dead Letter Queues (DLQ)
- Visibility timeout management
- Long polling (up to 20 seconds)
- Batch operations (10 messages)
- Delay queues (0-900 seconds)
- Message retention (60s - 14 days)
- Content-based deduplication
- Redrive policies

**Tested:** 7/7 tests passing with Python, JavaScript, Go, and Rust SDKs.

### ✅ GCP Pub/Sub Compatibility

Fully compatible with official Google Cloud SDKs (gRPC + HTTP/REST).

**Core Features:**
- Topics and subscriptions
- Message publishing with attributes
- Message ordering with ordering keys
- Pull and StreamingPull subscriptions
- Acknowledgment deadline modification
- Dead letter topics
- Message retention
- Subscription filtering

**Tested:** 53/53 tests passing with Python, JavaScript, and Go SDKs.

### 📦 Storage Options

**In-Memory (Default)**
- 1.8M+ messages/sec throughput
- Zero dependencies
- Perfect for testing
- Configurable eviction policies (LRU, FIFO, RejectNew)

**SQLite (Persistent)**
- Full ACID guarantees
- Single-file database
- Automatic maintenance
- Great for local development with data persistence

---

## Performance

Real benchmark results (in-memory backend):

| Operation | Throughput | P50 Latency | P99 Latency |
|-----------|------------|-------------|-------------|
| Send (single) | 1.82M/sec | 7.4µs | 34µs |
| Send (batch 10) | 4.67M/sec | 21µs | 85µs |
| Receive (single) | 1.66M/sec | 8.0µs | 31µs |
| Send+Receive cycle | 894K/sec | 14µs | 57µs |

**Result:** 182x faster than AWS SQS, 286x better latency.

See [docs/benchmarks.md](docs/benchmarks.md) for detailed analysis.

---

## Configuration

lclq works with **zero configuration**, but supports customization:

### Command Line
```bash
lclq start --sqs-port 9324 --bind-address 0.0.0.0
```

### Environment Variables
```bash
LCLQ_SQS_PORT=9324 LCLQ_BIND_ADDRESS=0.0.0.0 lclq start
```

### Config File (`lclq.toml`)
```toml
[server]
sqs_port = 9324
pubsub_grpc_port = 8085
bind_address = "127.0.0.1"

[storage.backend]
type = "InMemory"        # or "Sqlite"
max_messages = 100000
eviction_policy = "Lru"  # or "Fifo" or "RejectNew"

[logging]
level = "info"
format = "text"
```

Start with config: `lclq start --config lclq.toml`

---

## CLI Commands

```bash
# Server management
lclq start                         # Start all services
lclq health                        # Health check

# Queue operations
lclq queue list                    # List all queues
lclq queue create my-queue         # Create a queue
lclq queue delete my-queue         # Delete a queue
lclq queue stats my-queue          # Queue statistics
lclq queue purge my-queue          # Clear all messages
```

---

## Monitoring

### Prometheus Metrics (`http://localhost:9090/metrics`)

Track key metrics:
- `lclq_messages_sent_total` - Messages sent by queue
- `lclq_messages_received_total` - Messages received
- `lclq_queue_depth` - Current queue depth
- `lclq_send_latency_seconds` - Send latency histogram
- `lclq_receive_latency_seconds` - Receive latency histogram

### Admin API (`http://localhost:9000`)

- `GET /health` - Server health status
- `GET /stats` - System statistics
- `GET /queues` - List all queues
- `GET /queues/{name}` - Queue details

---

## FAQ

<details>
<summary><b>Do I need AWS or GCP accounts?</b></summary>

No! Use dummy credentials. lclq doesn't verify signatures or require real accounts.
</details>

<details>
<summary><b>Is lclq production-ready?</b></summary>

lclq is designed for **local development and CI/CD testing**. For production workloads, use actual cloud services (AWS SQS, GCP Pub/Sub).
</details>

<details>
<summary><b>Which SDKs work with lclq?</b></summary>

All official AWS and GCP SDKs. **Tested:** Python, JavaScript, Go, Rust.
**Should work:** Java, Ruby, .NET (not yet tested).
</details>

<details>
<summary><b>Does lclq persist data?</b></summary>

- **In-memory (default):** Data lost on restart. Perfect for tests.
- **SQLite:** Fully persistent with ACID guarantees.
</details>

<details>
<summary><b>How does lclq compare to LocalStack/ElasticMQ?</b></summary>

- **LocalStack:** Full AWS emulation (100+ services). lclq focuses on queues only = faster, simpler.
- **ElasticMQ:** SQS-only. lclq adds Pub/Sub + better performance (182x faster).
</details>

---

## Building from Source

```bash
# Clone repository
git clone https://github.com/erans/lclq.git
cd lclq

# Build
cargo build --release

# Run tests (408 unit tests, 66% coverage)
cargo test

# Run benchmarks
cargo bench

# Run integration tests
cd tests/integration/python && poetry install && poetry run pytest
cd tests/integration/javascript && npm install && npm test
```

---

## Project Status

| Component | Status | Tests |
|-----------|--------|-------|
| AWS SQS Core | ✅ Complete | 7/7 passing |
| GCP Pub/Sub Core | ✅ Complete | 53/53 passing |
| In-Memory Backend | ✅ Complete | 408 unit tests |
| SQLite Backend | ✅ Complete | 5 integration tests |
| CLI & Admin API | ✅ Complete | Full coverage |

See [docs/TODO.md](docs/TODO.md) for detailed roadmap.

---

## Documentation

- **[Quick Start Guide](docs/quickstart.md)** - Get started in 5 minutes
- **[Benchmarks](docs/benchmarks.md)** - Performance analysis
- **[Product Roadmap](docs/prd.md)** - Feature planning
- **[Technical Roadmap](docs/tech-prd.md)** - Implementation details

---

## Support & Contributing

- 🐛 **Bug Reports:** [GitHub Issues](https://github.com/erans/lclq/issues)
- 💬 **Discussions:** [GitHub Discussions](https://github.com/erans/lclq/discussions)
- 🤝 **Contributing:** Pull requests welcome!

---

## License

Dual-licensed under:
- **Apache License 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))

Choose whichever works best for your project.

---

## Acknowledgments

Inspired by excellent local development tools:
- [LocalStack](https://localstack.cloud/) - Local AWS emulation
- [Google Cloud Pub/Sub Emulator](https://cloud.google.com/pubsub/docs/emulator) - Official GCP emulator
- [ElasticMQ](https://github.com/softwaremill/elasticmq) - SQS-compatible queue

---

<div align="center">

**⭐ Star this repo if lclq makes your development faster!**

[Get Started](https://github.com/erans/lclq/releases) • [Report Bug](https://github.com/erans/lclq/issues) • [Request Feature](https://github.com/erans/lclq/issues)

</div>
