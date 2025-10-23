#!/usr/bin/env python3
"""Debug boto3 parsing."""

import boto3

SQS_ENDPOINT = "http://localhost:9324"
REGION = "us-east-1"

sqs = boto3.client(
    'sqs',
    endpoint_url=SQS_ENDPOINT,
    region_name=REGION,
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

# Create queue with RedriveAllowPolicy
response = sqs.create_queue(
    QueueName='debug-rap',
    Attributes={'RedriveAllowPolicy': '{"redrivePermission": "allowAll"}'}
)
queue_url = response['QueueUrl']
print(f"Queue URL: {queue_url}")

# Get attributes
attrs = sqs.get_queue_attributes(
    QueueUrl=queue_url,
    AttributeNames=['All']
)

print(f"\nAll Attributes:")
for key, value in attrs['Attributes'].items():
    print(f"  {key}: {value}")

# Try specifically requesting RedriveAllowPolicy
attrs2 = sqs.get_queue_attributes(
    QueueUrl=queue_url,
    AttributeNames=['RedriveAllowPolicy']
)

print(f"\nRedriveAllowPolicy only:")
print(f"  Keys: {list(attrs2['Attributes'].keys())}")
for key, value in attrs2['Attributes'].items():
    print(f"  {key}: {value}")

# Check message attributes
sqs.send_message(QueueUrl=queue_url, MessageBody="Test")
msgs = sqs.receive_message(QueueUrl=queue_url, AttributeNames=['All'])

print(f"\nReceive Message response:")
print(f"  Keys: {list(msgs.keys())}")
if 'Messages' in msgs:
    msg = msgs['Messages'][0]
    print(f"  Message keys: {list(msg.keys())}")
    if 'Attributes' in msg:
        print(f"  Attributes: {msg['Attributes']}")
    else:
        print("  NO ATTRIBUTES!")

# Cleanup
sqs.delete_queue(QueueUrl=queue_url)
