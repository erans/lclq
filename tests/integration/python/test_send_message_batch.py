#!/usr/bin/env python3
"""
Test SendMessageBatch SQS action with boto3.
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

def test_send_message_batch():
    """Test SendMessageBatch action."""
    print("\n🧪 Test: SendMessageBatch")

    # Create queue
    queue_name = f'test-batch-{int(time.time())}'
    response = sqs.create_queue(QueueName=queue_name)
    queue_url = response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Send batch of 5 messages
    print("\n📝 Sending batch of 5 messages...")
    response = sqs.send_message_batch(
        QueueUrl=queue_url,
        Entries=[
            {'Id': '1', 'MessageBody': 'Message 1'},
            {'Id': '2', 'MessageBody': 'Message 2'},
            {'Id': '3', 'MessageBody': 'Message 3'},
            {'Id': '4', 'MessageBody': 'Message 4'},
            {'Id': '5', 'MessageBody': 'Message 5'},
        ]
    )

    assert 'Successful' in response
    assert len(response['Successful']) == 5
    print(f"✅ {len(response['Successful'])} messages sent successfully")

    for entry in response['Successful']:
        print(f"  Entry {entry['Id']}: MessageId={entry['MessageId']}, MD5={entry['MD5OfMessageBody'][:8]}...")

    # Verify messages in queue
    attrs = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ApproximateNumberOfMessages']
    )
    assert attrs['Attributes']['ApproximateNumberOfMessages'] == '5'
    print(f"✅ Queue has 5 messages")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("✅ Queue deleted")


if __name__ == '__main__':
    try:
        test_send_message_batch()
        print("\n" + "=" * 60)
        print("✅ ALL TESTS PASSED!")
        print("=" * 60)
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        exit(1)
