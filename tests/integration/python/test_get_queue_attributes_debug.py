#!/usr/bin/env python3
"""
Debug GetQueueAttributes SQS action with boto3.
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

# Create a test queue
queue_name = f'test-debug-{int(time.time())}'
create_response = sqs.create_queue(QueueName=queue_name)
queue_url = create_response['QueueUrl']
print(f"Queue created: {queue_url}")

# Test GetQueueAttributes
print("\nCalling GetQueueAttributes...")
try:
    response = sqs.get_queue_attributes(
        QueueUrl=queue_url,
        AttributeNames=['All']
    )
    print("Response:")
    print(response)
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()

# Clean up
sqs.delete_queue(QueueUrl=queue_url)
