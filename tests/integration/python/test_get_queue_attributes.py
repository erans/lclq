#!/usr/bin/env python3
"""
Test GetQueueAttributes SQS action with boto3.
"""

import boto3
import time

# Configure boto3 to use local lclq server
SQS_ENDPOINT = "http://localhost:9324"
REGION = "us-east-1"

# Create SQS client pointing to local lclq
sqs = boto3.client(
    'sqs',
    endpoint_url=SQS_ENDPOINT,
    region_name=REGION,
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)


def test_get_queue_attributes():
    """Test GetQueueAttributes action."""
    print("\n🧪 Test: GetQueueAttributes")

    # Create a test queue
    queue_name = f'test-attrs-{int(time.time())}'
    create_response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={
            'VisibilityTimeout': '60',
            'MessageRetentionPeriod': '86400',
            'DelaySeconds': '5',
        }
    )
    queue_url = create_response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Test 1: Get all attributes
    print("\n  Test 1: Get all attributes")
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['All']
    )

    assert 'Attributes' in response
    attrs = response['Attributes']

    print(f"  Received {len(attrs)} attributes:")
    for key, value in sorted(attrs.items()):
        print(f"    {key}: {value}")

    # Verify expected attributes
    assert 'VisibilityTimeout' in attrs
    assert attrs['VisibilityTimeout'] == '60'
    assert 'MessageRetentionPeriod' in attrs
    assert attrs['MessageRetentionPeriod'] == '86400'
    assert 'DelaySeconds' in attrs
    assert attrs['DelaySeconds'] == '5'
    assert 'ApproximateNumberOfMessages' in attrs
    assert attrs['ApproximateNumberOfMessages'] == '0'
    assert 'QueueArn' in attrs

    print("  ✅ All attributes verified")

    # Test 2: Get specific attributes
    print("\n  Test 2: Get specific attributes")
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['VisibilityTimeout', 'DelaySeconds']
    )

    attrs = response['Attributes']
    assert len(attrs) == 2
    assert attrs['VisibilityTimeout'] == '60'
    assert attrs['DelaySeconds'] == '5'
    print("  ✅ Specific attributes verified")

    # Test 3: Send some messages and check counters
    print("\n  Test 3: Send messages and check counters")
    for i in range(3):
        sqs.send_message(QueueUrl=queue_url, MessageBody=f"Message {i}")

    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ApproximateNumberOfMessages']
    )

    attrs = response['Attributes']
    assert attrs['ApproximateNumberOfMessages'] == '3'
    print(f"  ✅ Message count: {attrs['ApproximateNumberOfMessages']}")

    # Test 4: Receive messages and check in-flight counter
    print("\n  Test 4: Receive messages and check in-flight")
    sqs.receive_message(QueueUrl=queue_url, MaxNumberOfMessages=2)

    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ApproximateNumberOfMessages', 'ApproximateNumberOfMessagesNotVisible']
    )

    attrs = response['Attributes']
    print(f"  Available: {attrs['ApproximateNumberOfMessages']}")
    print(f"  In-flight: {attrs['ApproximateNumberOfMessagesNotVisible']}")
    assert attrs['ApproximateNumberOfMessages'] == '1'
    assert attrs['ApproximateNumberOfMessagesNotVisible'] == '2'
    print("  ✅ In-flight counter verified")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("\n✅ Queue deleted")


def test_fifo_queue_attributes():
    """Test GetQueueAttributes with FIFO queue."""
    print("\n🧪 Test: GetQueueAttributes for FIFO Queue")

    # Create FIFO queue
    queue_name = f'test-fifo-attrs-{int(time.time())}.fifo'
    create_response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={
            'FifoQueue': 'true',
            'ContentBasedDeduplication': 'true'
        }
    )
    queue_url = create_response['QueueUrl']
    print(f"✅ FIFO Queue created: {queue_url}")

    # Get attributes
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['All']
    )

    attrs = response['Attributes']
    print(f"  Received {len(attrs)} attributes")

    # Verify FIFO-specific attributes
    assert 'FifoQueue' in attrs
    assert attrs['FifoQueue'] == 'true'
    assert 'ContentBasedDeduplication' in attrs
    assert attrs['ContentBasedDeduplication'] == 'true'

    print(f"  FifoQueue: {attrs['FifoQueue']}")
    print(f"  ContentBasedDeduplication: {attrs['ContentBasedDeduplication']}")
    print("  ✅ FIFO attributes verified")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("\n✅ FIFO Queue deleted")


def run_all_tests():
    """Run all GetQueueAttributes tests."""
    print("=" * 60)
    print("🚀 GetQueueAttributes Integration Tests")
    print("=" * 60)
    print(f"Endpoint: {SQS_ENDPOINT}")
    print(f"Region: {REGION}")

    try:
        test_get_queue_attributes()
        test_fifo_queue_attributes()

        print("\n" + "=" * 60)
        print("✅ ALL TESTS PASSED!")
        print("=" * 60)
        return True

    except Exception as e:
        print("\n" + "=" * 60)
        print(f"❌ TEST FAILED: {e}")
        print("=" * 60)
        import traceback
        traceback.print_exc()
        return False


if __name__ == '__main__':
    success = run_all_tests()
    exit(0 if success else 1)
