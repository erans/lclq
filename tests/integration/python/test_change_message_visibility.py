#!/usr/bin/env python3
"""
Test ChangeMessageVisibility SQS action with boto3.
"""

import boto3
import time

SQS_ENDPOINT = "http://localhost:9324"
REGION = "us-east-1"

sqs = boto3.client(
    'sqs',
    endpoint_url=SQS_ENDPOINT,
    region_name=REGION,
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

def test_change_message_visibility():
    """Test ChangeMessageVisibility action."""
    print("\n🧪 Test: ChangeMessageVisibility")

    # Create queue
    queue_name = f'test-visibility-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Send a message
    print("\n📝 Sending message...")
    sqs.send_message(QueueUrl=queue_url, MessageBody='Test message')
    print("✅ Message sent")

    # Receive message with short visibility timeout
    print("\n📥 Receiving message with 10 second visibility timeout...")
    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1,
        VisibilityTimeout=10
    )

    assert 'Messages' in response
    assert len(response['Messages']) == 1
    message = response['Messages'][0]
    receipt_handle = message['ReceiptHandle']
    print(f"✅ Message received: {message['MessageId']}")

    # Change visibility timeout to 60 seconds
    print("\n⏱️  Changing visibility timeout to 60 seconds...")
    sqs.change_message_visibility(
        QueueUrl=queue_url,
        ReceiptHandle=receipt_handle,
        VisibilityTimeout=60
    )
    print("✅ Visibility timeout changed")

    # Try to receive message immediately - should get nothing (still invisible)
    print("\n📥 Attempting to receive message (should be invisible)...")
    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1,
        WaitTimeSeconds=1
    )

    assert 'Messages' not in response or len(response['Messages']) == 0
    print("✅ Message still invisible (visibility timeout working)")

    # Change visibility to 0 to make it immediately visible
    print("\n⏱️  Changing visibility timeout to 0 (make visible)...")
    sqs.change_message_visibility(
        QueueUrl=queue_url,
        ReceiptHandle=receipt_handle,
        VisibilityTimeout=0
    )
    print("✅ Visibility timeout set to 0")

    # Now we should be able to receive it again
    print("\n📥 Receiving message again (should be visible now)...")
    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=1
    )

    assert 'Messages' in response
    assert len(response['Messages']) == 1
    print(f"✅ Message received again: {response['Messages'][0]['MessageId']}")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("✅ Queue deleted")


if __name__ == '__main__':
    try:
        test_change_message_visibility()
        print("\n" + "=" * 60)
        print("✅ ALL TESTS PASSED!")
        print("=" * 60)
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        exit(1)
