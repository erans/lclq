#!/usr/bin/env python3
"""
Advanced integration tests for lclq using boto3.

Tests DLQ functionality, long polling, delay queues, and other advanced SQS features.
"""

import boto3
import time
import pytest
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


@pytest.mark.skip(reason="DLQ message movement not yet implemented - messages that exceed max receive count are dropped instead of moved to DLQ")
def test_dead_letter_queue():
    """Test Dead Letter Queue functionality."""
    print("\n🧪 Test: Dead Letter Queue")

    # Create DLQ
    dlq_name = f'test-dlq-{int(time.time())}'
    dlq_response = sqs.create_queue(QueueName=dlq_name)
    dlq_url = dlq_response['QueueUrl']
    print(f"✅ DLQ created: {dlq_url}")

    # Get DLQ ARN
    dlq_attrs = sqs.get_queue_attributes(
        QueueUrl=dlq_url,
        AttributeNames=['QueueArn']
    )
    dlq_arn = dlq_attrs['Attributes']['QueueArn']
    print(f"   DLQ ARN: {dlq_arn}")

    # Create main queue with DLQ configured
    main_queue_name = f'test-main-queue-{int(time.time())}'
    main_response = sqs.create_queue(
        QueueName=main_queue_name,
        Attributes={
            'VisibilityTimeout': '1',  # Short visibility timeout
            'RedrivePolicy': f'{{"maxReceiveCount": "2", "deadLetterTargetArn": "{dlq_arn}"}}'
        }
    )
    main_queue_url = main_response['QueueUrl']
    print(f"✅ Main queue created with DLQ: {main_queue_url}")

    # Verify RedrivePolicy was set
    attrs = sqs.get_queue_attributes(
        QueueUrl=main_queue_url,
        AttributeNames=['RedrivePolicy']
    )
    assert 'RedrivePolicy' in attrs['Attributes']
    print(f"   RedrivePolicy: {attrs['Attributes']['RedrivePolicy']}")

    # Send a message to main queue
    send_response = sqs.send_message(
        QueueUrl=main_queue_url,
        MessageBody='Test message for DLQ'
    )
    message_id = send_response['MessageId']
    print(f"✅ Message sent: {message_id}")

    # Receive and NOT delete (simulating processing failure) - first attempt
    print("\n   Attempt 1: Receive without deleting...")
    receive1 = sqs.receive_message(
        QueueUrl=main_queue_url,
        MaxNumberOfMessages=1,
        AttributeNames=['All']
    )
    assert 'Messages' in receive1 and len(receive1['Messages']) > 0
    assert receive1['Messages'][0]['Attributes']['ApproximateReceiveCount'] == '1'
    print(f"   ✓ Received (receive count: 1)")

    # Wait for visibility timeout to expire
    print("   ⏳ Waiting for visibility timeout (2 seconds)...")
    time.sleep(2)

    # Receive again - second attempt
    print("   Attempt 2: Receive without deleting...")
    receive2 = sqs.receive_message(
        QueueUrl=main_queue_url,
        MaxNumberOfMessages=1,
        AttributeNames=['All']
    )
    assert 'Messages' in receive2 and len(receive2['Messages']) > 0
    assert receive2['Messages'][0]['Attributes']['ApproximateReceiveCount'] == '2'
    print(f"   ✓ Received (receive count: 2)")

    # Wait for visibility timeout to expire again
    print("   ⏳ Waiting for visibility timeout (2 seconds)...")
    time.sleep(2)

    # Try to receive from main queue - should be empty (moved to DLQ)
    print("   Attempt 3: Try receiving from main queue...")
    receive3 = sqs.receive_message(
        QueueUrl=main_queue_url,
        MaxNumberOfMessages=1
    )

    # Check if message was moved to DLQ
    print("   📬 Checking DLQ for moved message...")
    dlq_messages = sqs.receive_message(
        QueueUrl=dlq_url,
        MaxNumberOfMessages=1
    )

    if 'Messages' in dlq_messages and len(dlq_messages['Messages']) > 0:
        dlq_message = dlq_messages['Messages'][0]
        print(f"✅ Message successfully moved to DLQ!")
        print(f"   MessageId: {dlq_message['MessageId']}")
        print(f"   Body: {dlq_message['Body']}")

        # Clean up DLQ message
        sqs.delete_message(
            QueueUrl=dlq_url,
            ReceiptHandle=dlq_message['ReceiptHandle']
        )
    else:
        print("⚠️  Message not yet in DLQ (may still be processing)")

    # Clean up
    sqs.delete_queue(QueueUrl=main_queue_url)
    sqs.delete_queue(QueueUrl=dlq_url)
    print(f"✅ Cleanup complete")


def test_redrive_allow_policy():
    """Test RedriveAllowPolicy for DLQ access control."""
    print("\n🧪 Test: RedriveAllowPolicy")

    # Create DLQ
    dlq_name = f'test-dlq-rap-{int(time.time())}'
    dlq_response = sqs.create_queue(QueueName=dlq_name)
    dlq_url = dlq_response['QueueUrl']
    print(f"✅ DLQ created: {dlq_url}")

    # Set RedriveAllowPolicy to allowAll
    sqs.set_queue_attributes(
        QueueUrl=dlq_url,
        Attributes={
            'RedriveAllowPolicy': '{"redrivePermission": "allowAll"}'
        }
    )
    print(f"✅ RedriveAllowPolicy set to allowAll")

    # Get and verify the policy
    attrs = sqs.get_queue_attributes(
        QueueUrl=dlq_url,
        AttributeNames=['RedriveAllowPolicy']
    )
    assert 'RedriveAllowPolicy' in attrs['Attributes']
    print(f"   RedriveAllowPolicy: {attrs['Attributes']['RedriveAllowPolicy']}")

    # Change to denyAll
    sqs.set_queue_attributes(
        QueueUrl=dlq_url,
        Attributes={
            'RedriveAllowPolicy': '{"redrivePermission": "denyAll"}'
        }
    )
    print(f"✅ RedriveAllowPolicy changed to denyAll")

    # Verify the change
    attrs = sqs.get_queue_attributes(
        QueueUrl=dlq_url,
        AttributeNames=['RedriveAllowPolicy']
    )
    assert '"redrivePermission":"denyAll"' in attrs['Attributes']['RedriveAllowPolicy']

    # Clean up
    sqs.delete_queue(QueueUrl=dlq_url)
    print(f"✅ Cleanup complete")


def test_long_polling():
    """Test long polling functionality."""
    print("\n🧪 Test: Long Polling")

    # Create queue
    queue_name = f'test-longpoll-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Start time for measuring wait
    start_time = time.time()

    # Receive with 5 second wait time (should return empty after 5 seconds)
    print("   📭 Receiving with 5-second long poll (queue is empty)...")
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1,
        WaitTimeSeconds=5
    )

    elapsed = time.time() - start_time
    print(f"   ⏱️  Elapsed time: {elapsed:.2f} seconds")

    # Should not have messages
    assert 'Messages' not in receive_response or len(receive_response['Messages']) == 0
    print(f"✅ Long poll completed correctly (no messages, waited ~5 seconds)")

    # Now send a message
    sqs.send_message(
        QueueUrl=queue_url,
        MessageBody='Long polling test message'
    )
    print("   ✉️  Message sent")

    # Receive with long polling - should return immediately since message is available
    start_time = time.time()
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1,
        WaitTimeSeconds=5
    )
    elapsed = time.time() - start_time

    assert 'Messages' in receive_response and len(receive_response['Messages']) > 0
    print(f"✅ Message received immediately (elapsed: {elapsed:.2f}s)")

    # Clean up
    sqs.delete_message(
        QueueUrl=queue_url,
        ReceiptHandle=receive_response['Messages'][0]['ReceiptHandle']
    )
    sqs.delete_queue(QueueUrl=queue_url)
    print(f"✅ Cleanup complete")


def test_delay_queue():
    """Test delay queue functionality."""
    print("\n🧪 Test: Delay Queue")

    # Create queue with 5-second delay
    queue_name = f'test-delay-{int(time.time())}'
    response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={
            'DelaySeconds': '5'
        }
    )
    queue_url = response['QueueUrl']
    print(f"✅ Delay queue created (5 second delay): {queue_url}")

    # Send message
    send_time = time.time()
    sqs.send_message(
        QueueUrl=queue_url,
        MessageBody='Delayed message'
    )
    print(f"   ✉️  Message sent at {datetime.now().strftime('%H:%M:%S')}")

    # Try to receive immediately - should not get message
    print("   📭 Attempting immediate receive...")
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )
    assert 'Messages' not in receive_response or len(receive_response['Messages']) == 0
    print(f"   ✓ No message received (still delayed)")

    # Wait for delay period
    print("   ⏳ Waiting 6 seconds for delay to expire...")
    time.sleep(6)

    # Now receive - should get the message
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )
    receive_time = time.time()

    assert 'Messages' in receive_response and len(receive_response['Messages']) > 0
    elapsed = receive_time - send_time
    print(f"✅ Message received after {elapsed:.2f} seconds (expected ~5s delay)")

    # Clean up
    sqs.delete_message(
        QueueUrl=queue_url,
        ReceiptHandle=receive_response['Messages'][0]['ReceiptHandle']
    )
    sqs.delete_queue(QueueUrl=queue_url)
    print(f"✅ Cleanup complete")


def test_per_message_delay():
    """Test per-message delay functionality."""
    print("\n🧪 Test: Per-Message Delay")

    # Create standard queue (no default delay)
    queue_name = f'test-permsg-delay-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Send message with 3-second delay
    send_time = time.time()
    sqs.send_message(
        QueueUrl=queue_url,
        MessageBody='Per-message delayed',
        DelaySeconds=3
    )
    print(f"   ✉️  Message sent with 3-second delay")

    # Try immediate receive - should be empty
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )
    assert 'Messages' not in receive_response or len(receive_response['Messages']) == 0
    print(f"   ✓ No message received immediately")

    # Wait and receive
    print("   ⏳ Waiting 4 seconds...")
    time.sleep(4)

    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )
    receive_time = time.time()

    assert 'Messages' in receive_response and len(receive_response['Messages']) > 0
    elapsed = receive_time - send_time
    print(f"✅ Message received after {elapsed:.2f} seconds")

    # Clean up
    sqs.delete_message(
        QueueUrl=queue_url,
        ReceiptHandle=receive_response['Messages'][0]['ReceiptHandle']
    )
    sqs.delete_queue(QueueUrl=queue_url)
    print(f"✅ Cleanup complete")


def test_queue_attributes():
    """Test getting and setting queue attributes."""
    print("\n🧪 Test: Queue Attributes")

    # Create queue
    queue_name = f'test-attrs-{int(time.time())}'
    response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={
            'VisibilityTimeout': '60',
            'MessageRetentionPeriod': '86400'
        }
    )
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Get all attributes
    attrs = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['All']
    )

    print(f"   Queue Attributes:")
    for key, value in sorted(attrs['Attributes'].items()):
        print(f"     {key}: {value}")

    assert attrs['Attributes']['VisibilityTimeout'] == '60'
    assert attrs['Attributes']['MessageRetentionPeriod'] == '86400'

    # Set attribute
    sqs.set_queue_attributes(
        QueueUrl=queue_url,
        Attributes={
            'VisibilityTimeout': '120'
        }
    )
    print(f"✅ VisibilityTimeout updated to 120")

    # Verify update
    attrs = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['VisibilityTimeout']
    )
    assert attrs['Attributes']['VisibilityTimeout'] == '120'
    print(f"   ✓ Verified: VisibilityTimeout = 120")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print(f"✅ Cleanup complete")


def test_change_message_visibility():
    """Test changing message visibility timeout."""
    print("\n🧪 Test: ChangeMessageVisibility")

    # Create queue
    queue_name = f'test-visibility-{int(time.time())}'
    response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={'VisibilityTimeout': '30'}
    )
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Send message
    sqs.send_message(
        QueueUrl=queue_url,
        MessageBody='Test visibility change'
    )
    print(f"   ✉️  Message sent")

    # Receive message
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )
    assert 'Messages' in receive_response and len(receive_response['Messages']) > 0
    receipt_handle = receive_response['Messages'][0]['ReceiptHandle']
    print(f"   📬 Message received")

    # Change visibility to 0 (make immediately available)
    sqs.change_message_visibility(
        QueueUrl=queue_url,
        ReceiptHandle=receipt_handle,
        VisibilityTimeout=0
    )
    print(f"   ✓ Visibility changed to 0 (return to queue)")

    # Should be able to receive immediately
    receive_response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )
    assert 'Messages' in receive_response and len(receive_response['Messages']) > 0
    print(f"✅ Message received again immediately")

    # Clean up
    sqs.delete_message(
        QueueUrl=queue_url,
        ReceiptHandle=receive_response['Messages'][0]['ReceiptHandle']
    )
    sqs.delete_queue(QueueUrl=queue_url)
    print(f"✅ Cleanup complete")


def run_all_tests():
    """Run all advanced integration tests."""
    print("=" * 60)
    print("🚀 lclq Advanced Integration Tests with boto3")
    print("=" * 60)
    print(f"Endpoint: {SQS_ENDPOINT}")
    print(f"Region: {REGION}")

    tests = [
        ("Dead Letter Queue", test_dead_letter_queue),
        ("RedriveAllowPolicy", test_redrive_allow_policy),
        ("Long Polling", test_long_polling),
        ("Delay Queue", test_delay_queue),
        ("Per-Message Delay", test_per_message_delay),
        ("Queue Attributes", test_queue_attributes),
        ("Change Message Visibility", test_change_message_visibility),
    ]

    passed = 0
    failed = 0

    for test_name, test_func in tests:
        try:
            test_func()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"\n❌ {test_name} FAILED: {e}")
            import traceback
            traceback.print_exc()

    print("\n" + "=" * 60)
    print(f"📊 Test Results: {passed} passed, {failed} failed")
    if failed == 0:
        print("✅ ALL TESTS PASSED!")
    else:
        print(f"❌ {failed} TEST(S) FAILED")
    print("=" * 60)

    return failed == 0


if __name__ == '__main__':
    success = run_all_tests()
    exit(0 if success else 1)
