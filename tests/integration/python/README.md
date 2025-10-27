# lclq Python Integration Tests

Integration tests for lclq using AWS SDK for Python (boto3).

## Requirements

- Python 3.12 or higher
- Poetry for dependency management

## Installation

```bash
poetry install
```

## Running Tests

```bash
poetry run pytest test_sqs_advanced.py -v
```

## Test Coverage

These tests validate lclq's compatibility with AWS SQS API using boto3.
