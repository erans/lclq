#!/usr/bin/env python3
"""
Integration tests for lclq using boto3.

Tests the AWS SQS compatibility of lclq.
"""

import boto3
import time
from datetime import datetime

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


def test_create_queue():
    """Test creating a queue."""
    print("\n🧪 Test 1: CreateQueue")

    queue_name = f'test-queue-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)

    assert 'QueueUrl' in response
    assert queue_name in response['QueueUrl']

    print(f"✅ Queue created: {response['QueueUrl']}")
    return response['QueueUrl']


def test_get_queue_url(queue_name):
    """Test getting queue URL."""
    print("\n🧪 Test 2: GetQueueUrl")

    response = sqs.get_queue_url(QueueName=queue_name)

    assert 'QueueUrl' in response
    assert queue_name in response['QueueUrl']

    print(f"✅ Queue URL retrieved: {response['QueueUrl']}")
    return response['QueueUrl']


def test_list_queues():
    """Test listing queues."""
    print("\n🧪 Test 3: ListQueues")

    response = sqs.list_queues()

    assert 'QueueUrls' in response
    assert len(response['QueueUrls']) > 0

    print(f"✅ Found {len(response['QueueUrls'])} queue(s):")
    for url in response['QueueUrls']:
        print(f"   - {url}")

    return response['QueueUrls']


def test_send_message(queue_url):
    """Test sending a message."""
    print("\n🧪 Test 4: SendMessage")

    message_body = f"Hello from boto3! Timestamp: {datetime.now().isoformat()}"

    response = sqs.send_message(
        QueueUrl=queue_url,
        MessageBody=message_body,
        MessageAttributes={
            'Author': {
                'StringValue': 'boto3-test',
                'DataType': 'String'
            },
            'Priority': {
                'StringValue': 'high',
                'DataType': 'String'
            }
        }
    )

    assert 'MessageId' in response
    assert 'MD5OfMessageBody' in response

    print(f"✅ Message sent:")
    print(f"   MessageId: {response['MessageId']}")
    print(f"   MD5: {response['MD5OfMessageBody']}")

    return response['MessageId']


def test_receive_message(queue_url):
    """Test receiving a message."""
    print("\n🧪 Test 5: ReceiveMessage")

    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1,
        MessageAttributeNames=['All'],
        AttributeNames=['All']
    )

    assert 'Messages' in response
    assert len(response['Messages']) > 0

    message = response['Messages'][0]

    assert 'MessageId' in message
    assert 'ReceiptHandle' in message
    assert 'Body' in message
    assert 'MD5OfBody' in message

    print(f"✅ Message received:")
    print(f"   MessageId: {message['MessageId']}")
    print(f"   Body: {message['Body']}")
    print(f"   MD5: {message['MD5OfBody']}")

    if 'MessageAttributes' in message:
        print(f"   Attributes: {message['MessageAttributes']}")

    if 'Attributes' in message:
        print(f"   System Attributes:")
        for key, value in message['Attributes'].items():
            print(f"     {key}: {value}")

    return message['ReceiptHandle']


def test_delete_message(queue_url, receipt_handle):
    """Test deleting a message."""
    print("\n🧪 Test 6: DeleteMessage")

    sqs.delete_message(
        QueueUrl=queue_url,
        ReceiptHandle=receipt_handle
    )

    print(f"✅ Message deleted successfully")


def test_send_multiple_messages(queue_url, count=3):
    """Test sending multiple messages."""
    print(f"\n🧪 Test 7: Send {count} Messages")

    for i in range(count):
        response = sqs.send_message(
            QueueUrl=queue_url,
            MessageBody=f"Message #{i+1}: {datetime.now().isoformat()}"
        )
        print(f"   ✓ Sent message {i+1}: {response['MessageId']}")

    print(f"✅ Sent {count} messages")


def test_receive_multiple_messages(queue_url):
    """Test receiving multiple messages."""
    print("\n🧪 Test 8: ReceiveMessage (multiple)")

    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=10
    )

    if 'Messages' in response:
        print(f"✅ Received {len(response['Messages'])} message(s)")
        for i, msg in enumerate(response['Messages'], 1):
            print(f"   {i}. {msg['MessageId']}: {msg['Body'][:50]}...")
        return response['Messages']
    else:
        print("⚠️  No messages received")
        return []


def test_delete_queue(queue_url):
    """Test deleting a queue."""
    print("\n🧪 Test 9: DeleteQueue")

    sqs.delete_queue(QueueUrl=queue_url)

    print(f"✅ Queue deleted: {queue_url}")


def test_fifo_queue():
    """Test FIFO queue functionality."""
    print("\n🧪 Test 10: FIFO Queue")

    queue_name = f'test-fifo-{int(time.time())}.fifo'

    # Create FIFO queue
    response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={
            'FifoQueue': 'true',
            'ContentBasedDeduplication': 'true'
        }
    )

    queue_url = response['QueueUrl']
    print(f"✅ FIFO queue created: {queue_url}")

    # Send message to FIFO queue
    response = sqs.send_message(
        QueueUrl=queue_url,
        MessageBody='FIFO message 1',
        MessageGroupId='group1'
    )

    print(f"✅ FIFO message sent: {response['MessageId']}")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print(f"✅ FIFO queue deleted")


def run_all_tests():
    """Run all integration tests."""
    print("=" * 60)
    print("🚀 lclq Integration Tests with boto3")
    print("=" * 60)
    print(f"Endpoint: {SQS_ENDPOINT}")
    print(f"Region: {REGION}")

    try:
        # Test basic queue operations
        queue_url = test_create_queue()
        queue_name = queue_url.split('/')[-1]

        test_get_queue_url(queue_name)
        test_list_queues()

        # Test message operations
        message_id = test_send_message(queue_url)
        receipt_handle = test_receive_message(queue_url)
        test_delete_message(queue_url, receipt_handle)

        # Test multiple messages
        test_send_multiple_messages(queue_url, count=5)
        messages = test_receive_multiple_messages(queue_url)

        # Clean up messages
        for msg in messages:
            sqs.delete_message(QueueUrl=queue_url, ReceiptHandle=msg['ReceiptHandle'])

        # Test FIFO queue
        test_fifo_queue()

        # Clean up
        test_delete_queue(queue_url)

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
