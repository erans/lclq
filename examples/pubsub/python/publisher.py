#!/usr/bin/env python3
"""
Simple Pub/Sub Publisher Example

This example demonstrates how to:
1. Connect to lclq (local Pub/Sub)
2. Create a topic
3. Publish messages with attributes
"""

import os
import sys
import time
from datetime import datetime
from google.cloud import pubsub_v1

# Point to lclq instead of GCP
os.environ['PUBSUB_EMULATOR_HOST'] = 'localhost:8085'

def main():
    project_id = 'example-project'
    topic_id = 'example-topic'

    try:
        # Create publisher client
        publisher = pubsub_v1.PublisherClient()
        topic_path = publisher.topic_path(project_id, topic_id)

        # Create topic
        print(f"Creating topic: {topic_path}")
        topic = publisher.create_topic(request={"name": topic_path})
        print(f"Topic created: {topic.name}\n")

        # Publish messages
        print("Publishing messages...")
        for i in range(10):
            message_data = f"Message {i} sent at {datetime.now().isoformat()}"

            # Publish with attributes
            future = publisher.publish(
                topic_path,
                message_data.encode('utf-8'),
                message_number=str(i),
                sender='publisher.py',
                timestamp=datetime.now().isoformat()
            )

            message_id = future.result()
            print(f"  ✓ Published: {message_data}")
            print(f"    Message ID: {message_id}")
            time.sleep(0.5)  # Small delay between messages

        print(f"\n✓ Successfully published 10 messages to {topic_id}")
        print(f"\nRun subscriber.py to receive these messages:")
        print(f"  python subscriber.py")

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
