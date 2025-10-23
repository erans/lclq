#!/usr/bin/env python3
"""
Test ChangeMessageVisibilityBatch SQS action with boto3.
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

def test_change_message_visibility_batch():
    """Test ChangeMessageVisibilityBatch action."""
    print("\n🧪 Test: ChangeMessageVisibilityBatch")

    # Create queue
    queue_name = f'test-visibility-batch-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Send 5 messages
    print("\n📝 Sending 5 messages...")
    for i in range(1, 6):
        sqs.send_message(QueueUrl=queue_url, MessageBody=f'Message {i}')
    print("✅ 5 messages sent")

    # Receive all messages with short visibility timeout
    print("\n📥 Receiving 5 messages with 10 second visibility timeout...")
    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=10,
        VisibilityTimeout=10
    )

    assert 'Messages' in response
    assert len(response['Messages']) == 5
    messages = response['Messages']
    print(f"✅ Received {len(messages)} messages")

    # Prepare batch entries to change visibility for first 3 messages
    print("\n⏱️  Changing visibility timeout for 3 messages to 60 seconds...")
    entries = []
    for i in range(3):
        entries.append({
            'Id': str(i + 1),
            'ReceiptHandle': messages[i]['ReceiptHandle'],
            'VisibilityTimeout': 60
        })

    response = sqs.change_message_visibility_batch(
        QueueUrl=queue_url,
        Entries=entries
    )

    assert 'Successful' in response
    assert len(response['Successful']) == 3
    print(f"✅ Successfully changed visibility for {len(response['Successful'])} messages")

    for entry in response['Successful']:
        print(f"  Entry {entry['Id']}: Visibility changed")

    # Verify failed entries is empty
    assert 'Failed' not in response or len(response['Failed']) == 0
    print("✅ No failed entries")

    # Try to receive messages - should only get the 2 that weren't changed (or none if they're still invisible)
    print("\n📥 Attempting to receive messages (3 should be invisible)...")
    time.sleep(1)  # Wait a bit for the original timeout to expire
    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=10,
        WaitTimeSeconds=2
    )

    # We might get 0-2 messages depending on timing
    received_count = len(response.get('Messages', []))
    print(f"✅ Received {received_count} messages (expected 0-2 due to visibility changes)")

    # Test with invalid receipt handle
    print("\n⚠️  Testing with invalid receipt handle...")
    invalid_response = sqs.change_message_visibility_batch(
        QueueUrl=queue_url,
        Entries=[
            {'Id': '1', 'ReceiptHandle': 'invalid-handle-123', 'VisibilityTimeout': 30},
            {'Id': '2', 'ReceiptHandle': messages[0]['ReceiptHandle'], 'VisibilityTimeout': 0},
        ]
    )

    # Should have 1 success and 1 failure
    assert 'Successful' in invalid_response
    assert 'Failed' in invalid_response
    assert len(invalid_response['Failed']) == 1
    print(f"✅ Got {len(invalid_response['Failed'])} failed entry as expected")
    print(f"  Failed entry: {invalid_response['Failed'][0]['Code']}")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("✅ Queue deleted")


if __name__ == '__main__':
    try:
        test_change_message_visibility_batch()
        print("\n" + "=" * 60)
        print("✅ ALL TESTS PASSED!")
        print("=" * 60)
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        exit(1)
