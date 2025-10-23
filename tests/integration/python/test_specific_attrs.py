#!/usr/bin/env python3
"""
Debug specific attributes request.
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

# Create queue
queue_name = f'test-debug-{int(time.time())}'
create_response = sqs.create_queue(QueueName=queue_name, Attributes={'DelaySeconds': '10'})
queue_url = create_response['QueueUrl']
print(f"Queue created: {queue_url}")

# Request specific attributes
response = sqs.get_queue_attributes(
    QueueUrl=queue_url,
    AttributeNames=['VisibilityTimeout', 'DelaySeconds']
)

print(f"\nRequested: ['VisibilityTimeout', 'DelaySeconds']")
print(f"Received {len(response['Attributes'])} attributes:")
for key, value in sorted(response['Attributes'].items()):
    print(f"  {key}: {value}")

# Clean up
sqs.delete_queue(QueueUrl=queue_url)
