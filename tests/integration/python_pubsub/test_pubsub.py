"""
Integration tests for lclq Pub/Sub implementation.

Tests the gRPC Pub/Sub API using the official google-cloud-pubsub Python client.
"""

import os
import time
import pytest
from google.cloud import pubsub_v1
from google.api_core import exceptions


# Configure to use local lclq server
os.environ["PUBSUB_EMULATOR_HOST"] = "localhost:8085"

PROJECT_ID = "test-project"
TOPIC_ID = "test-topic"
SUBSCRIPTION_ID = "test-subscription"


@pytest.fixture(scope="function")
def publisher_client():
    """Create a publisher client."""
    return pubsub_v1.PublisherClient()


@pytest.fixture(scope="function")
def subscriber_client():
    """Create a subscriber client."""
    return pubsub_v1.SubscriberClient()


@pytest.fixture(scope="function")
def topic_path(publisher_client):
    """Create a test topic and return its path."""
    topic_path = publisher_client.topic_path(PROJECT_ID, TOPIC_ID)
    try:
        publisher_client.delete_topic(request={"topic": topic_path})
    except exceptions.NotFound:
        pass

    publisher_client.create_topic(request={"name": topic_path})
    yield topic_path

    # Cleanup
    try:
        publisher_client.delete_topic(request={"topic": topic_path})
    except exceptions.NotFound:
        pass


@pytest.fixture(scope="function")
def subscription_path(subscriber_client, topic_path):
    """Create a test subscription and return its path."""
    subscription_path = subscriber_client.subscription_path(PROJECT_ID, SUBSCRIPTION_ID)
    try:
        subscriber_client.delete_subscription(request={"subscription": subscription_path})
    except exceptions.NotFound:
        pass

    subscriber_client.create_subscription(
        request={
            "name": subscription_path,
            "topic": topic_path,
            "ack_deadline_seconds": 30,
        }
    )
    yield subscription_path

    # Cleanup
    try:
        subscriber_client.delete_subscription(request={"subscription": subscription_path})
    except exceptions.NotFound:
        pass


def test_create_topic(publisher_client):
    """Test creating a topic."""
    topic_path = publisher_client.topic_path(PROJECT_ID, "test-create-topic")

    # Clean up if exists
    try:
        publisher_client.delete_topic(request={"topic": topic_path})
    except exceptions.NotFound:
        pass

    # Create topic
    topic = publisher_client.create_topic(request={"name": topic_path})

    assert topic.name == topic_path
    print(f"✓ Created topic: {topic.name}")

    # Cleanup
    publisher_client.delete_topic(request={"topic": topic_path})


def test_get_topic(publisher_client, topic_path):
    """Test getting a topic."""
    topic = publisher_client.get_topic(request={"topic": topic_path})

    assert topic.name == topic_path
    print(f"✓ Got topic: {topic.name}")


def test_list_topics(publisher_client, topic_path):
    """Test listing topics."""
    project_path = f"projects/{PROJECT_ID}"
    topics = list(publisher_client.list_topics(request={"project": project_path}))

    topic_names = [topic.name for topic in topics]
    assert topic_path in topic_names
    print(f"✓ Listed {len(topics)} topics")


def test_delete_topic(publisher_client):
    """Test deleting a topic."""
    topic_path = publisher_client.topic_path(PROJECT_ID, "test-delete-topic")

    # Create and delete
    publisher_client.create_topic(request={"name": topic_path})
    publisher_client.delete_topic(request={"topic": topic_path})

    # Verify deletion
    with pytest.raises(exceptions.NotFound):
        publisher_client.get_topic(request={"topic": topic_path})

    print(f"✓ Deleted topic: {topic_path}")


def test_publish_message(publisher_client, topic_path):
    """Test publishing a message."""
    message_data = b"Hello, Pub/Sub!"

    future = publisher_client.publish(topic_path, message_data)
    message_id = future.result()

    assert message_id is not None
    assert len(message_id) > 0
    print(f"✓ Published message with ID: {message_id}")


def test_publish_with_attributes(publisher_client, topic_path):
    """Test publishing a message with attributes."""
    message_data = b"Message with attributes"
    attributes = {
        "key1": "value1",
        "key2": "value2",
        "origin": "test",
    }

    future = publisher_client.publish(topic_path, message_data, **attributes)
    message_id = future.result()

    assert message_id is not None
    print(f"✓ Published message with attributes, ID: {message_id}")


def test_create_subscription(subscriber_client, topic_path):
    """Test creating a subscription."""
    subscription_path = subscriber_client.subscription_path(PROJECT_ID, "test-create-sub")

    # Clean up if exists
    try:
        subscriber_client.delete_subscription(request={"subscription": subscription_path})
    except exceptions.NotFound:
        pass

    # Create subscription
    subscription = subscriber_client.create_subscription(
        request={
            "name": subscription_path,
            "topic": topic_path,
            "ack_deadline_seconds": 60,
        }
    )

    assert subscription.name == subscription_path
    assert subscription.topic == topic_path
    assert subscription.ack_deadline_seconds == 60
    print(f"✓ Created subscription: {subscription.name}")

    # Cleanup
    subscriber_client.delete_subscription(request={"subscription": subscription_path})


def test_get_subscription(subscriber_client, subscription_path):
    """Test getting a subscription."""
    subscription = subscriber_client.get_subscription(
        request={"subscription": subscription_path}
    )

    assert subscription.name == subscription_path
    print(f"✓ Got subscription: {subscription.name}")


def test_list_subscriptions(subscriber_client, subscription_path):
    """Test listing subscriptions."""
    project_path = f"projects/{PROJECT_ID}"
    subscriptions = list(
        subscriber_client.list_subscriptions(request={"project": project_path})
    )

    subscription_names = [sub.name for sub in subscriptions]
    assert subscription_path in subscription_names
    print(f"✓ Listed {len(subscriptions)} subscriptions")


def test_delete_subscription(subscriber_client, topic_path):
    """Test deleting a subscription."""
    subscription_path = subscriber_client.subscription_path(PROJECT_ID, "test-delete-sub")

    # Create and delete
    subscriber_client.create_subscription(
        request={
            "name": subscription_path,
            "topic": topic_path,
            "ack_deadline_seconds": 30,
        }
    )
    subscriber_client.delete_subscription(request={"subscription": subscription_path})

    # Verify deletion
    with pytest.raises(exceptions.NotFound):
        subscriber_client.get_subscription(request={"subscription": subscription_path})

    print(f"✓ Deleted subscription: {subscription_path}")


def test_publish_and_pull(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test publishing and pulling messages."""
    # Publish messages
    messages = [
        b"Message 1",
        b"Message 2",
        b"Message 3",
    ]

    message_ids = []
    for msg_data in messages:
        future = publisher_client.publish(topic_path, msg_data)
        message_ids.append(future.result())

    print(f"✓ Published {len(messages)} messages")

    # Pull messages
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 10,
        }
    )

    assert len(response.received_messages) == len(messages)

    # Verify message content
    received_data = [msg.message.data for msg in response.received_messages]
    for msg_data in messages:
        assert msg_data in received_data

    print(f"✓ Pulled {len(response.received_messages)} messages")

    # Acknowledge messages
    ack_ids = [msg.ack_id for msg in response.received_messages]
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": ack_ids,
        }
    )

    print(f"✓ Acknowledged {len(ack_ids)} messages")


def test_message_attributes(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test message attributes in publish and pull."""
    message_data = b"Message with attributes"
    attributes = {
        "attribute1": "value1",
        "attribute2": "value2",
        "number": "42",
    }

    # Publish with attributes
    future = publisher_client.publish(topic_path, message_data, **attributes)
    message_id = future.result()

    print(f"✓ Published message with attributes")

    # Pull and verify attributes
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 1,
        }
    )

    assert len(response.received_messages) == 1
    msg = response.received_messages[0].message

    assert msg.data == message_data
    for key, value in attributes.items():
        assert key in msg.attributes
        assert msg.attributes[key] == value

    print(f"✓ Verified message attributes")

    # Acknowledge
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": [response.received_messages[0].ack_id],
        }
    )


def test_message_ordering():
    """Test message ordering with ordering keys."""
    # Create publisher client with message ordering enabled
    from google.cloud.pubsub_v1.types import PublisherOptions
    publisher_client = pubsub_v1.PublisherClient(
        publisher_options=PublisherOptions(enable_message_ordering=True)
    )
    subscriber_client = pubsub_v1.SubscriberClient()

    topic_path = publisher_client.topic_path(PROJECT_ID, "test-ordering-topic")
    subscription_path = subscriber_client.subscription_path(PROJECT_ID, "test-ordering-sub")

    # Cleanup
    try:
        subscriber_client.delete_subscription(request={"subscription": subscription_path})
    except exceptions.NotFound:
        pass
    try:
        publisher_client.delete_topic(request={"topic": topic_path})
    except exceptions.NotFound:
        pass

    # Create topic and subscription with ordering enabled
    publisher_client.create_topic(request={"name": topic_path})
    subscriber_client.create_subscription(
        request={
            "name": subscription_path,
            "topic": topic_path,
            "ack_deadline_seconds": 30,
            "enable_message_ordering": True,
        }
    )

    # Publish messages with ordering key
    ordering_key = "order-key-1"
    messages = [
        (b"Message 1", ordering_key),
        (b"Message 2", ordering_key),
        (b"Message 3", ordering_key),
    ]

    for msg_data, key in messages:
        future = publisher_client.publish(topic_path, msg_data, ordering_key=key)
        future.result()

    print(f"✓ Published {len(messages)} ordered messages")

    # Pull and verify order
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 10,
        }
    )

    assert len(response.received_messages) == len(messages)

    # Verify ordering keys
    for msg in response.received_messages:
        assert msg.message.ordering_key == ordering_key

    print(f"✓ Verified message ordering")

    # Cleanup
    ack_ids = [msg.ack_id for msg in response.received_messages]
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": ack_ids,
        }
    )
    subscriber_client.delete_subscription(request={"subscription": subscription_path})
    publisher_client.delete_topic(request={"topic": topic_path})


def test_acknowledge(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test acknowledging messages."""
    # Publish a message
    message_data = b"Test acknowledge"
    future = publisher_client.publish(topic_path, message_data)
    message_id = future.result()

    # Pull the message
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 1,
        }
    )

    assert len(response.received_messages) == 1
    ack_id = response.received_messages[0].ack_id

    # Acknowledge
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": [ack_id],
        }
    )

    print(f"✓ Acknowledged message: {message_id}")

    # Pull again - should be empty
    response2 = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 1,
        }
    )

    assert len(response2.received_messages) == 0
    print(f"✓ Verified message was deleted after ack")


def test_modify_ack_deadline(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test modifying ack deadline."""
    # Publish a message
    message_data = b"Test modify ack deadline"
    future = publisher_client.publish(topic_path, message_data)
    future.result()

    # Pull the message
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 1,
        }
    )

    assert len(response.received_messages) == 1
    ack_id = response.received_messages[0].ack_id

    # Modify ack deadline to 60 seconds
    subscriber_client.modify_ack_deadline(
        request={
            "subscription": subscription_path,
            "ack_ids": [ack_id],
            "ack_deadline_seconds": 60,
        }
    )

    print(f"✓ Modified ack deadline to 60 seconds")

    # Acknowledge to clean up
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": [ack_id],
        }
    )


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-s"])
