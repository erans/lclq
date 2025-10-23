# lclq Pub/Sub JavaScript Integration Tests

Integration tests for the lclq Pub/Sub implementation using the official Google Cloud Pub/Sub JavaScript/Node.js client.

## Setup

1. Install Node.js (v18 or later recommended)

2. Install dependencies:
```bash
cd tests/integration/js_pubsub
npm install
```

## Running Tests

1. Start the lclq server in another terminal:
```bash
cargo run -- start
```

2. Run the tests:
```bash
npm test
```

Or run with verbose output:
```bash
npm run test:verbose
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

## Test Framework

- **Testing Framework**: Jest
- **SDK**: @google-cloud/pubsub v4.x
- **Node.js**: ES Modules (type: "module")

## Test Results

All tests should pass when the lclq server is running.
