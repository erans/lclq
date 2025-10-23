# lclq Pub/Sub Integration Tests

Integration tests for the lclq Pub/Sub implementation using the official Google Cloud Pub/Sub Python client.

## Setup

1. Install Poetry (if not already installed):
```bash
curl -sSL https://install.python-poetry.org | python3 -
```

2. Install dependencies:
```bash
cd tests/integration/python_pubsub
poetry install
```

## Running Tests

1. Start the lclq server in another terminal:
```bash
cargo run -- start
```

2. Run the tests:
```bash
poetry run pytest test_pubsub.py -v
```

Or run with verbose output:
```bash
poetry run pytest test_pubsub.py -v -s
```

## Test Coverage

The test suite covers:

- ✅ Topic Management
  - Create topic
  - Get topic
  - List topics
  - Delete topic

- ✅ Subscription Management
  - Create subscription
  - Get subscription
  - List subscriptions
  - Delete subscription

- ✅ Message Publishing
  - Publish single message
  - Publish with attributes
  - Publish with ordering keys

- ✅ Message Consumption
  - Pull messages
  - Message attributes
  - Message ordering
  - Acknowledge messages
  - Modify ack deadline

## Configuration

The tests connect to lclq using the `PUBSUB_EMULATOR_HOST` environment variable:
- Default: `localhost:8085`
- This tells the Google Cloud client to use the local lclq server instead of GCP

## Test Results

All tests should pass when the lclq server is running.
