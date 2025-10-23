#!/usr/bin/env python3
"""
Simple SQS Consumer Example

This example demonstrates how to:
1. Connect to lclq (local SQS)
2. Receive messages from a queue
3. Process messages
4. Delete messages after processing
"""

import boto3
import sys
import time

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
        # Get queue URL
        print(f"Getting queue URL for: {queue_name}")
        response = sqs.get_queue_url(QueueName=queue_name)
        queue_url = response['QueueUrl']
        print(f"Queue URL: {queue_url}\n")

        print("Polling for messages (press Ctrl+C to stop)...\n")

        message_count = 0

        while True:
            # Receive messages (long polling for 5 seconds)
            response = sqs.receive_message(
                QueueUrl=queue_url,
                MaxNumberOfMessages=10,
                WaitTimeSeconds=5,
                MessageAttributeNames=['All']
            )

            messages = response.get('Messages', [])

            if not messages:
                print("  (waiting for messages...)")
                continue

            # Process each message
            for message in messages:
                message_count += 1
                body = message['Body']
                attrs = message.get('MessageAttributes', {})

                print(f"Received message #{message_count}:")
                print(f"  Body: {body}")
                print(f"  Message ID: {message['MessageId']}")

                if attrs:
                    print(f"  Attributes:")
                    for key, value in attrs.items():
                        print(f"    {key}: {value.get('StringValue', value)}")

                # Delete the message after processing
                sqs.delete_message(
                    QueueUrl=queue_url,
                    ReceiptHandle=message['ReceiptHandle']
                )
                print(f"  ✓ Deleted\n")

    except KeyboardInterrupt:
        print(f"\n\n✓ Processed {message_count} messages")
        print("Exiting...")
        sys.exit(0)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
