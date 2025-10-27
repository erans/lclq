# lclq Examples

This directory contains example applications demonstrating how to use lclq with various SDKs and languages.

## Available Examples

### AWS SQS

- **[Python (boto3)](sqs/python/)** - Producer/consumer example using boto3
  - `producer.py` - Sends messages to a queue
  - `consumer.py` - Receives and processes messages
  - Complete with message attributes and long polling

### GCP Pub/Sub

- **[Python (google-cloud-pubsub)](pubsub/python/)** - Publisher/subscriber example using google-cloud-pubsub
  - `publisher.py` - Publishes messages to a topic
  - `subscriber.py` - Subscribes and pulls messages
  - Complete with message attributes and acknowledgments

## Quick Start

### 1. Start lclq

```bash
# Start all services
lclq start

# Or using Docker
docker run -p 9324:9324 -p 8085:8085 -p 9000:9000 erans/lclq:latest
```

### 2. Choose an Example

Pick an example based on your needs:

**For AWS SQS:**
```bash
cd sqs/python
pip install boto3
python producer.py  # In one terminal
python consumer.py  # In another terminal
```

**For GCP Pub/Sub:**
```bash
cd pubsub/python
pip install google-cloud-pubsub
python publisher.py   # In one terminal
python subscriber.py  # In another terminal
```

## Example Patterns

All examples demonstrate these patterns:

### SQS Examples
- ✅ Connecting to lclq instead of AWS
- ✅ Creating queues dynamically
- ✅ Sending messages with attributes
- ✅ Receiving messages with long polling
- ✅ Deleting messages after processing
- ✅ Proper error handling

### Pub/Sub Examples
- ✅ Setting emulator host for lclq
- ✅ Creating topics and subscriptions
- ✅ Publishing messages with attributes
- ✅ Pulling messages synchronously
- ✅ Acknowledging messages
- ✅ Proper error handling

## Common Use Cases

### Local Development

Run examples locally to:
- Test queue-based applications without cloud costs
- Develop offline without internet connection
- Iterate quickly with instant queue creation
- Debug message flows easily

### Integration Testing

Use examples in CI/CD pipelines:
- Start lclq in CI environment
- Run integration tests against local queues
- No need for AWS/GCP credentials
- Fast and reliable tests

### Learning

Use examples to learn:
- AWS SQS and GCP Pub/Sub concepts
- Message queue patterns
- Producer/consumer architectures
- Without needing cloud accounts

## Advanced Examples (Coming Soon)

- Node.js examples for SQS and Pub/Sub
- Go examples for SQS and Pub/Sub
- FIFO queue examples with ordering
- Dead Letter Queue (DLQ) examples
- Batch operations examples
- Message ordering with Pub/Sub
- Error handling and retry patterns

## Running in Docker

If you're running lclq in Docker, update the endpoint URLs:

**SQS (Python):**
```python
sqs = boto3.client(
    'sqs',
    endpoint_url='http://lclq:9324',  # Use container name
    ...
)
```

**Pub/Sub (Python):**
```python
os.environ['PUBSUB_EMULATOR_HOST'] = 'lclq:8085'  # Use container name
```

## Troubleshooting

### "Connection refused"

Make sure lclq is running:
```bash
# Check if lclq is running
curl http://localhost:9000/health

# Check queues
curl http://localhost:9000/queues
```

### "Queue does not exist" (SQS)

Queues are created on first use. The producer examples create queues automatically.

### "Topic does not exist" (Pub/Sub)

Topics and subscriptions are created by the examples. Make sure to run the publisher first.

### Port conflicts

If ports are in use, start lclq on different ports:
```bash
lclq start --sqs-port 9325 --pubsub-port 8086
```

Then update the example code to use the new ports.

## Contributing Examples

Have an example to share? We'd love to include it!

Examples we're looking for:
- Node.js/TypeScript examples
- Go examples
- Rust examples
- Advanced patterns (DLQ, FIFO, retries)
- Real-world use cases

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

## Resources

- **[Quick Start Guide](../docs/quickstart.md)** - Detailed setup instructions
- **[Configuration Guide](../docs/configuration.md)** - Advanced configuration
- **[API Reference](../docs/api-reference.md)** - Complete API documentation
- **[GitHub Repository](https://github.com/erans/lclq)** - Source code and issues

---

**Need help?** Open an issue at https://github.com/erans/lclq/issues
