#!/usr/bin/env python3
"""
Test DeleteMessageBatch SQS action with boto3.
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

def test_delete_message_batch():
    """Test DeleteMessageBatch action."""
    print("\n🧪 Test: DeleteMessageBatch")

    # Create queue
    queue_name = f'test-delete-batch-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Send 7 messages
    print("\n📝 Sending 7 messages...")
    for i in range(1, 8):
        sqs.send_message(QueueUrl=queue_url, MessageBody=f'Message {i}')
    print("✅ 7 messages sent")

    # Verify queue has 7 messages
    attrs = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ApproximateNumberOfMessages']
    )
    assert attrs['Attributes']['ApproximateNumberOfMessages'] == '7'
    print("✅ Queue has 7 messages")

    # Receive 7 messages
    print("\n📥 Receiving 7 messages...")
    response = sqs.receive_message(
        QueueUrl=queue_url,
        MaxNumberOfMessages=10
    )

    assert 'Messages' in response
    assert len(response['Messages']) == 7
    messages = response['Messages']
    print(f"✅ Received {len(messages)} messages")

    # Delete first 5 messages in batch
    print("\n🗑️  Deleting 5 messages in batch...")
    entries = []
    for i in range(5):
        entries.append({
            'Id': str(i + 1),
            'ReceiptHandle': messages[i]['ReceiptHandle']
        })

    response = sqs.delete_message_batch(
        QueueUrl=queue_url,
        Entries=entries
    )

    assert 'Successful' in response
    assert len(response['Successful']) == 5
    print(f"✅ Successfully deleted {len(response['Successful'])} messages")

    for entry in response['Successful']:
        print(f"  Entry {entry['Id']}: Deleted")

    # Verify failed entries is empty
    assert 'Failed' not in response or len(response['Failed']) == 0
    print("✅ No failed entries")

    # Verify queue now has 2 messages in-flight (7 - 5 = 2)
    attrs = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ApproximateNumberOfMessages', 'ApproximateNumberOfMessagesNotVisible']
    )
    # The 2 remaining messages are still in-flight (not visible)
    assert attrs['Attributes']['ApproximateNumberOfMessagesNotVisible'] == '2'
    print("✅ Queue now has 2 messages in-flight (not visible)")

    # Test with mix of valid and invalid receipt handles
    print("\n⚠️  Testing batch with invalid receipt handle...")
    invalid_response = sqs.delete_message_batch(
        QueueUrl=queue_url,
        Entries=[
            {'Id': '1', 'ReceiptHandle': 'invalid-receipt-handle-123'},
            {'Id': '2', 'ReceiptHandle': messages[5]['ReceiptHandle']},
            {'Id': '3', 'ReceiptHandle': messages[6]['ReceiptHandle']},
        ]
    )

    # Should have 2 successes and 1 failure
    assert 'Successful' in invalid_response
    assert 'Failed' in invalid_response
    assert len(invalid_response['Successful']) == 2
    assert len(invalid_response['Failed']) == 1
    print(f"✅ Got {len(invalid_response['Successful'])} successes and {len(invalid_response['Failed'])} failure")
    print(f"  Failed entry: {invalid_response['Failed'][0]['Code']}")

    # Verify queue is now empty
    attrs = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ApproximateNumberOfMessages']
    )
    assert attrs['Attributes']['ApproximateNumberOfMessages'] == '0'
    print("✅ Queue is now empty")

    # Test deleting more than 10 messages (should fail)
    print("\n⚠️  Testing batch with more than 10 entries (should fail)...")
    try:
        large_batch = [{'Id': str(i), 'ReceiptHandle': f'handle-{i}'} for i in range(11)]
        sqs.delete_message_batch(
            QueueUrl=queue_url,
            Entries=large_batch
        )
        print("❌ Should have raised an error for >10 entries")
        assert False, "Expected error for batch size > 10"
    except Exception as e:
        print(f"✅ Got expected error: {type(e).__name__}")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("✅ Queue deleted")


if __name__ == '__main__':
    try:
        test_delete_message_batch()
        print("\n" + "=" * 60)
        print("✅ ALL TESTS PASSED!")
        print("=" * 60)
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        exit(1)
