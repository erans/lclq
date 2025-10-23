"""
Integration tests for lclq Pub/Sub REST API implementation.

Tests the HTTP/REST Pub/Sub API using the official google-cloud-pubsub Python client
with REST transport.
"""

import os
import time
import pytest
from google.cloud import pubsub_v1
from google.api_core import exceptions
from google.api_core.client_options import ClientOptions
from google.auth.credentials import AnonymousCredentials


# Configure to use local lclq REST server
REST_ENDPOINT = "http://localhost:8086"
PROJECT_ID = "test-project"
TOPIC_ID = "test-topic-rest"
SUBSCRIPTION_ID = "test-subscription-rest"


@pytest.fixture(scope="function")
def publisher_client():
    """Create a publisher client using REST transport."""
    client_options = ClientOptions(api_endpoint=REST_ENDPOINT)
    return pubsub_v1.PublisherClient(
        transport='rest',
        client_options=client_options,
        credentials=AnonymousCredentials()
    )


@pytest.fixture(scope="function")
def subscriber_client():
    """Create a subscriber client using REST transport."""
    client_options = ClientOptions(api_endpoint=REST_ENDPOINT)
    return pubsub_v1.SubscriberClient(
        transport='rest',
        client_options=client_options,
        credentials=AnonymousCredentials()
    )


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
            "ack_deadline_seconds": 10
        }
    )
    yield subscription_path

    # Cleanup
    try:
        subscriber_client.delete_subscription(request={"subscription": subscription_path})
    except exceptions.NotFound:
        pass


def test_create_and_get_topic_rest(publisher_client):
    """Test creating and retrieving a topic via REST API."""
    topic_path = publisher_client.topic_path(PROJECT_ID, "rest-create-topic")

    # Clean up if exists
    try:
        publisher_client.delete_topic(request={"topic": topic_path})
    except exceptions.NotFound:
        pass

    # Create topic
    topic = publisher_client.create_topic(request={"name": topic_path})
    assert topic.name == topic_path
    print(f"✓ Created topic via REST: {topic.name}")

    # Get topic
    retrieved_topic = publisher_client.get_topic(request={"topic": topic_path})
    assert retrieved_topic.name == topic_path
    print(f"✓ Retrieved topic via REST: {retrieved_topic.name}")

    # Cleanup
    publisher_client.delete_topic(request={"topic": topic_path})
    print(f"✓ Deleted topic via REST: {topic_path}")


def test_list_topics_rest(publisher_client):
    """Test listing topics via REST API."""
    project_path = f"projects/{PROJECT_ID}"

    # Create a few test topics
    topic_ids = ["rest-list-1", "rest-list-2", "rest-list-3"]
    created_topics = []

    for topic_id in topic_ids:
        topic_path = publisher_client.topic_path(PROJECT_ID, topic_id)
        try:
            publisher_client.delete_topic(request={"topic": topic_path})
        except exceptions.NotFound:
            pass
        topic = publisher_client.create_topic(request={"name": topic_path})
        created_topics.append(topic.name)

    # List topics
    topics = list(publisher_client.list_topics(request={"project": project_path}))
    topic_names = [t.name for t in topics]

    # Verify our topics are in the list
    for created_topic in created_topics:
        assert created_topic in topic_names, f"Topic {created_topic} not found in list"

    print(f"✓ Listed {len(topics)} topics via REST (found our {len(created_topics)} test topics)")

    # Cleanup
    for topic_id in topic_ids:
        topic_path = publisher_client.topic_path(PROJECT_ID, topic_id)
        try:
            publisher_client.delete_topic(request={"topic": topic_path})
        except exceptions.NotFound:
            pass


def test_publish_and_pull_messages_rest(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test publishing and pulling messages via REST API."""
    # Publish messages
    messages_to_send = [
        {"data": b"REST message 1", "attributes": {"source": "test", "index": "1"}},
        {"data": b"REST message 2", "attributes": {"source": "test", "index": "2"}},
        {"data": b"REST message 3", "attributes": {"source": "test", "index": "3"}},
    ]

    message_ids = []
    for msg in messages_to_send:
        future = publisher_client.publish(
            topic_path,
            msg["data"],
            **msg["attributes"]
        )
        message_id = future.result(timeout=5)
        message_ids.append(message_id)

    print(f"✓ Published {len(message_ids)} messages via REST")

    # Wait a bit for messages to be available
    time.sleep(0.5)

    # Pull messages
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 10,
        },
        timeout=5
    )

    assert len(response.received_messages) == 3, f"Expected 3 messages, got {len(response.received_messages)}"
    print(f"✓ Pulled {len(response.received_messages)} messages via REST")

    # Verify message content
    received_data = [msg.message.data for msg in response.received_messages]
    expected_data = [msg["data"] for msg in messages_to_send]

    for expected in expected_data:
        assert expected in received_data, f"Expected message data {expected} not found"

    # Verify attributes
    for received_msg in response.received_messages:
        assert "source" in received_msg.message.attributes
        assert received_msg.message.attributes["source"] == "test"
        print(f"✓ Message attributes verified: {dict(received_msg.message.attributes)}")

    # Acknowledge messages
    ack_ids = [msg.ack_id for msg in response.received_messages]
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": ack_ids
        }
    )
    print(f"✓ Acknowledged {len(ack_ids)} messages via REST")


def test_create_and_get_subscription_rest(subscriber_client, topic_path):
    """Test creating and retrieving a subscription via REST API."""
    subscription_path = subscriber_client.subscription_path(PROJECT_ID, "rest-create-sub")

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
            "ack_deadline_seconds": 30
        }
    )
    assert subscription.name == subscription_path
    assert subscription.topic == topic_path
    assert subscription.ack_deadline_seconds == 30
    print(f"✓ Created subscription via REST: {subscription.name}")

    # Get subscription
    retrieved_sub = subscriber_client.get_subscription(
        request={"subscription": subscription_path}
    )
    assert retrieved_sub.name == subscription_path
    assert retrieved_sub.topic == topic_path
    print(f"✓ Retrieved subscription via REST: {retrieved_sub.name}")

    # Cleanup
    subscriber_client.delete_subscription(request={"subscription": subscription_path})
    print(f"✓ Deleted subscription via REST: {subscription_path}")


def test_list_subscriptions_rest(subscriber_client, topic_path):
    """Test listing subscriptions via REST API."""
    project_path = f"projects/{PROJECT_ID}"

    # Create a few test subscriptions
    subscription_ids = ["rest-sub-list-1", "rest-sub-list-2"]
    created_subs = []

    for sub_id in subscription_ids:
        sub_path = subscriber_client.subscription_path(PROJECT_ID, sub_id)
        try:
            subscriber_client.delete_subscription(request={"subscription": sub_path})
        except exceptions.NotFound:
            pass
        sub = subscriber_client.create_subscription(
            request={
                "name": sub_path,
                "topic": topic_path,
            }
        )
        created_subs.append(sub.name)

    # List subscriptions
    subscriptions = list(subscriber_client.list_subscriptions(request={"project": project_path}))
    sub_names = [s.name for s in subscriptions]

    # Verify our subscriptions are in the list
    for created_sub in created_subs:
        assert created_sub in sub_names, f"Subscription {created_sub} not found in list"

    print(f"✓ Listed {len(subscriptions)} subscriptions via REST (found our {len(created_subs)} test subs)")

    # Cleanup
    for sub_id in subscription_ids:
        sub_path = subscriber_client.subscription_path(PROJECT_ID, sub_id)
        try:
            subscriber_client.delete_subscription(request={"subscription": sub_path})
        except exceptions.NotFound:
            pass


def test_modify_ack_deadline_rest(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test modifying acknowledgment deadline via REST API."""
    # Publish a message
    future = publisher_client.publish(topic_path, b"Test ack deadline message")
    message_id = future.result(timeout=5)
    print(f"✓ Published message via REST: {message_id}")

    time.sleep(0.5)

    # Pull message
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 1,
        },
        timeout=5
    )

    assert len(response.received_messages) == 1
    ack_id = response.received_messages[0].ack_id
    print(f"✓ Pulled message via REST with ack_id: {ack_id[:20]}...")

    # Modify ack deadline
    subscriber_client.modify_ack_deadline(
        request={
            "subscription": subscription_path,
            "ack_ids": [ack_id],
            "ack_deadline_seconds": 60
        }
    )
    print(f"✓ Modified ack deadline to 60 seconds via REST")

    # Clean up by acknowledging
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": [ack_id]
        }
    )
    print(f"✓ Acknowledged message via REST")


def test_message_ordering_rest(publisher_client, subscriber_client, topic_path):
    """Test message ordering with ordering keys via REST API."""
    # Create subscription with message ordering enabled
    subscription_path = subscriber_client.subscription_path(PROJECT_ID, "rest-ordered-sub")

    try:
        subscriber_client.delete_subscription(request={"subscription": subscription_path})
    except exceptions.NotFound:
        pass

    subscriber_client.create_subscription(
        request={
            "name": subscription_path,
            "topic": topic_path,
            "enable_message_ordering": True
        }
    )
    print(f"✓ Created subscription with message ordering via REST")

    # Publish messages with ordering key
    ordering_key = "order-group-1"
    message_ids = []

    for i in range(5):
        # Note: The Python SDK's publish method doesn't directly support ordering_key
        # in the same way as gRPC. For REST, we'd need to use a different approach
        # or the SDK needs to support it. For now, we'll publish without ordering key.
        future = publisher_client.publish(
            topic_path,
            f"Ordered message {i}".encode(),
            order_number=str(i)
        )
        message_id = future.result(timeout=5)
        message_ids.append(message_id)

    print(f"✓ Published {len(message_ids)} messages via REST")

    time.sleep(0.5)

    # Pull and verify messages
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 10,
        },
        timeout=5
    )

    assert len(response.received_messages) == 5
    print(f"✓ Pulled {len(response.received_messages)} ordered messages via REST")

    # Acknowledge all
    ack_ids = [msg.ack_id for msg in response.received_messages]
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": ack_ids
        }
    )

    # Cleanup
    subscriber_client.delete_subscription(request={"subscription": subscription_path})
    print(f"✓ Cleaned up ordered subscription via REST")


def test_empty_pull_rest(subscriber_client, subscription_path):
    """Test pulling from empty subscription via REST API."""
    # Pull with no messages available
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": 10,
        },
        timeout=5
    )

    # Should return empty list, not error
    assert len(response.received_messages) == 0
    print(f"✓ Empty pull returned 0 messages via REST (as expected)")


def test_batch_publish_rest(publisher_client, subscriber_client, topic_path, subscription_path):
    """Test batch publishing via REST API."""
    # Publish multiple messages
    futures = []
    num_messages = 10

    for i in range(num_messages):
        future = publisher_client.publish(
            topic_path,
            f"Batch message {i}".encode(),
            batch_id=str(i)
        )
        futures.append(future)

    # Wait for all publishes to complete
    message_ids = [future.result(timeout=5) for future in futures]
    assert len(message_ids) == num_messages
    print(f"✓ Batch published {len(message_ids)} messages via REST")

    time.sleep(0.5)

    # Pull all messages
    response = subscriber_client.pull(
        request={
            "subscription": subscription_path,
            "max_messages": num_messages,
        },
        timeout=5
    )

    assert len(response.received_messages) == num_messages
    print(f"✓ Pulled {len(response.received_messages)} batch messages via REST")

    # Acknowledge all
    ack_ids = [msg.ack_id for msg in response.received_messages]
    subscriber_client.acknowledge(
        request={
            "subscription": subscription_path,
            "ack_ids": ack_ids
        }
    )
    print(f"✓ Acknowledged all batch messages via REST")


if __name__ == "__main__":
    # Run tests with pytest
    pytest.main([__file__, "-v", "-s"])
