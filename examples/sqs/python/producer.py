#!/usr/bin/env python3
"""
Simple SQS Producer Example

This example demonstrates how to:
1. Connect to lclq (local SQS)
2. Create a queue
3. Send messages with attributes
"""

import boto3
import sys
import time
from datetime import datetime

# Configure SQS client to use lclq
sqs = boto3.client(
    'sqs',
    endpoint_url='http://localhost:9324',
    region_name='us-east-1',
    aws_access_key_id='dummy',
    aws_secret_access_key='dummy'
)

def main():
    queue_name = 'example-queue'

    try:
        # Create queue
        print(f"Creating queue: {queue_name}")
        response = sqs.create_queue(QueueName=queue_name)
        queue_url = response['QueueUrl']
        print(f"Queue URL: {queue_url}\n")

        # Send messages
        print("Sending messages...")
        for i in range(10):
            message_body = f"Message {i} sent at {datetime.now().isoformat()}"

            response = sqs.send_message(
                QueueUrl=queue_url,
                MessageBody=message_body,
                MessageAttributes={
                    'MessageNumber': {
                        'StringValue': str(i),
                        'DataType': 'Number'
                    },
                    'Sender': {
                        'StringValue': 'producer.py',
                        'DataType': 'String'
                    },
                    'Timestamp': {
                        'StringValue': datetime.now().isoformat(),
                        'DataType': 'String'
                    }
                }
            )

            print(f"  ✓ Sent: {message_body}")
            print(f"    Message ID: {response['MessageId']}")
            time.sleep(0.5)  # Small delay between messages

        print(f"\n✓ Successfully sent 10 messages to {queue_name}")
        print(f"\nRun consumer.py to receive these messages:")
        print(f"  python consumer.py")

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
