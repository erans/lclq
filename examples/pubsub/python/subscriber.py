#!/usr/bin/env python3
"""
Simple Pub/Sub Subscriber Example

This example demonstrates how to:
1. Connect to lclq (local Pub/Sub)
2. Create a subscription
3. Pull and acknowledge messages
"""

import os
import sys
import time
from google.cloud import pubsub_v1

# Point to lclq instead of GCP
os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'

def main():
    project_id = 'example-project'
    topic_id = 'example-topic'
    subscription_id = 'example-subscription'

    try:
        # Create clients
        publisher = pubsub_v1.PublisherClient()
        subscriber = pubsub_v1.SubscriberClient()

        # Get paths
        topic_path = publisher.topic_path(project_id, topic_id)
        subscription_path = subscriber.subscription_path(project_id, subscription_id)

        # Create subscription (if doesn't exist)
        print(f"Creating subscription: {subscription_path}")
        try:
            subscription = subscriber.create_subscription(
                request={"name": subscription_path, "topic": topic_path}
            )
            print(f"Subscription created: {subscription.name}\n")
        except Exception as e:
            if "ALREADY_EXISTS" in str(e):
                print(f"Subscription already exists: {subscription_path}\n")
            else:
                raise

        print("Polling for messages (press Ctrl+C to stop)...\n")

        message_count = 0

        while True:
            # Pull messages
            response = subscriber.pull(
                request={"subscription": subscription_path, "max_messages": 10}
            )

            if not response.received_messages:
                print("  (waiting for messages...)")
                time.sleep(2)
                continue

            # Process messages
            for received_message in response.received_messages:
                message_count += 1
                msg = received_message.message

                print(f"Received message #{message_count}:")
                print(f"  Data: {msg.data.decode('utf-8')}")
                print(f"  Message ID: {msg.message_id}")

                if msg.attributes:
                    print(f"  Attributes:")
                    for key, value in msg.attributes.items():
                        print(f"    {key}: {value}")

                print(f"  ✓ Acknowledged\n")

            # Acknowledge all received messages
            ack_ids = [msg.ack_id for msg in response.received_messages]
            subscriber.acknowledge(
                request={"subscription": subscription_path, "ack_ids": ack_ids}
            )

    except KeyboardInterrupt:
        print(f"\n\n✓ Processed {message_count} messages")
        print("Exiting...")
        sys.exit(0)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
