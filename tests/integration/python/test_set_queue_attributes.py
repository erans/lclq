#!/usr/bin/env python3
"""
Test SetQueueAttributes SQS action with boto3.
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


def test_set_queue_attributes():
    """Test SetQueueAttributes action."""
    print("\n🧪 Test: SetQueueAttributes")

    # Create a test queue with default attributes
    queue_name = f'test-set-attrs-{int(time.time())}'
    create_response = sqs.create_queue(QueueName=queue_name)
    queue_url = create_response['QueueUrl']
    print(f"✅ Queue created: {queue_url}")

    # Get initial attributes
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['VisibilityTimeout', 'DelaySeconds', 'MessageRetentionPeriod']
    )
    initial_attrs = response['Attributes']
    print(f"\nInitial attributes:")
    for key, value in sorted(initial_attrs.items()):
        print(f"  {key}: {value}")

    # Update attributes
    print("\n📝 Updating attributes...")
    sqs.set_queue_attributes(
        QueueUrl=queue_url,
        Attributes={
            'VisibilityTimeout': '120',
            'DelaySeconds': '10',
            'MessageRetentionPeriod': '604800'  # 7 days
        }
    )
    print("✅ Attributes updated")

    # Verify attributes were updated
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['VisibilityTimeout', 'DelaySeconds', 'MessageRetentionPeriod']
    )
    updated_attrs = response['Attributes']
    print(f"\nUpdated attributes:")
    for key, value in sorted(updated_attrs.items()):
        print(f"  {key}: {value}")

    # Verify the changes
    assert updated_attrs['VisibilityTimeout'] == '120'
    assert updated_attrs['DelaySeconds'] == '10'
    assert updated_attrs['MessageRetentionPeriod'] == '604800'
    print("\n✅ All attribute updates verified")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("✅ Queue deleted")


def test_set_redrive_policy():
    """Test setting RedrivePolicy (DLQ) via SetQueueAttributes."""
    print("\n🧪 Test: SetQueueAttributes with RedrivePolicy")

    # Create DLQ
    dlq_name = f'test-dlq-{int(time.time())}'
    dlq_response = sqs.create_queue(QueueName=dlq_name)
    dlq_url = dlq_response['QueueUrl']
    print(f"✅ DLQ created: {dlq_url}")

    # Create main queue
    queue_name = f'test-main-{int(time.time())}'
    main_response = sqs.create_queue(QueueName=queue_name)
    main_url = main_response['QueueUrl']
    print(f"✅ Main queue created: {main_url}")

    # Set redrive policy
    redrive_policy = {
        'deadLetterTargetArn': f'arn:aws:sqs:us-east-1:000000000000:{dlq_name}',
        'maxReceiveCount': '3'
    }

    print(f"\n📝 Setting RedrivePolicy...")
    sqs.set_queue_attributes(
        QueueUrl=main_url,
        Attributes={
            'RedrivePolicy': str(redrive_policy).replace("'", '"')
        }
    )
    print("✅ RedrivePolicy set")

    # Verify redrive policy
    response = sqs.get_queue_attributes(
        QueueUrl=main_url,
        AttributeNames=['RedrivePolicy']
    )

    if 'RedrivePolicy' in response['Attributes']:
        import json
        policy = json.loads(response['Attributes']['RedrivePolicy'])
        print(f"\nRedrivePolicy:")
        print(f"  deadLetterTargetArn: {policy['deadLetterTargetArn']}")
        print(f"  maxReceiveCount: {policy['maxReceiveCount']}")

        assert policy['maxReceiveCount'] == 3
        assert dlq_name in policy['deadLetterTargetArn']
        print("✅ RedrivePolicy verified")
    else:
        print("❌ RedrivePolicy not found")

    # Clean up
    sqs.delete_queue(QueueUrl=main_url)
    sqs.delete_queue(QueueUrl=dlq_url)
    print("✅ Queues deleted")


def test_fifo_content_based_dedup():
    """Test setting ContentBasedDeduplication for FIFO queue."""
    print("\n🧪 Test: SetQueueAttributes for FIFO Queue")

    # Create FIFO queue
    queue_name = f'test-fifo-{int(time.time())}.fifo'
    create_response = sqs.create_queue(
        QueueName=queue_name,
        Attributes={'FifoQueue': 'true'}
    )
    queue_url = create_response['QueueUrl']
    print(f"✅ FIFO queue created: {queue_url}")

    # Enable content-based deduplication
    print("\n📝 Enabling ContentBasedDeduplication...")
    sqs.set_queue_attributes(
        QueueUrl=queue_url,
        Attributes={'ContentBasedDeduplication': 'true'}
    )
    print("✅ ContentBasedDeduplication enabled")

    # Verify
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['ContentBasedDeduplication']
    )

    assert response['Attributes']['ContentBasedDeduplication'] == 'true'
    print("✅ ContentBasedDeduplication verified")

    # Clean up
    sqs.delete_queue(QueueUrl=queue_url)
    print("✅ Queue deleted")


def run_all_tests():
    """Run all SetQueueAttributes tests."""
    print("=" * 60)
    print("🚀 SetQueueAttributes Integration Tests")
    print("=" * 60)
    print(f"Endpoint: {SQS_ENDPOINT}")
    print(f"Region: {REGION}")

    try:
        test_set_queue_attributes()
        test_set_redrive_policy()
        test_fifo_content_based_dedup()

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
