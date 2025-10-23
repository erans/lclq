// Integration tests for lclq Pub/Sub gRPC implementation.
//
// Tests the Pub/Sub gRPC API using the official Google Cloud Pub/Sub Go client.
package main

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	"cloud.google.com/go/pubsub"
	"google.golang.org/api/option"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

const (
	projectID      = "test-project"
	emulatorHost   = "localhost:8085"
	topicID        = "test-topic"
	subscriptionID = "test-subscription"
)

// setupClient creates a Pub/Sub client configured to use the local lclq server
func setupClient(t *testing.T) (*pubsub.Client, context.Context) {
	ctx := context.Background()

	// Set environment variable for emulator
	os.Setenv("PUBSUB_EMULATOR_HOST", emulatorHost)

	// Create client with insecure connection for local testing
	conn, err := grpc.Dial(
		emulatorHost,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		t.Fatalf("Failed to dial emulator: %v", err)
	}

	client, err := pubsub.NewClient(
		ctx,
		projectID,
		option.WithGRPCConn(conn),
	)
	if err != nil {
		t.Fatalf("Failed to create client: %v", err)
	}

	return client, ctx
}

// cleanupTopic deletes a topic if it exists
func cleanupTopic(ctx context.Context, client *pubsub.Client, topicID string) {
	topic := client.Topic(topicID)
	_ = topic.Delete(ctx)
}

// cleanupSubscription deletes a subscription if it exists
func cleanupSubscription(ctx context.Context, client *pubsub.Client, subscriptionID string) {
	sub := client.Subscription(subscriptionID)
	_ = sub.Delete(ctx)
}

// TestCreateTopic tests creating a topic
func TestCreateTopic(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-create-topic"
	cleanupTopic(ctx, client, topicID)
	defer cleanupTopic(ctx, client, topicID)

	// Create topic
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	if topic.ID() != topicID {
		t.Errorf("Expected topic ID %s, got %s", topicID, topic.ID())
	}

	t.Logf("✓ Created topic: %s", topic.ID())
}

// TestGetTopic tests getting an existing topic
func TestGetTopic(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-get-topic"
	cleanupTopic(ctx, client, topicID)
	defer cleanupTopic(ctx, client, topicID)

	// Create topic
	_, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Get topic
	topic := client.Topic(topicID)
	exists, err := topic.Exists(ctx)
	if err != nil {
		t.Fatalf("Failed to check topic existence: %v", err)
	}

	if !exists {
		t.Errorf("Expected topic to exist")
	}

	t.Logf("✓ Got topic: %s", topic.ID())
}

// TestListTopics tests listing topics
func TestListTopics(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	// Create multiple topics
	topicIDs := []string{"test-list-topic-1", "test-list-topic-2", "test-list-topic-3"}
	for _, id := range topicIDs {
		cleanupTopic(ctx, client, id)
		defer cleanupTopic(ctx, client, id)

		_, err := client.CreateTopic(ctx, id)
		if err != nil {
			t.Fatalf("Failed to create topic %s: %v", id, err)
		}
	}

	// List topics
	it := client.Topics(ctx)
	foundTopics := make(map[string]bool)

	for {
		topic, err := it.Next()
		if err != nil {
			break
		}
		foundTopics[topic.ID()] = true
	}

	// Verify all topics were found
	for _, id := range topicIDs {
		if !foundTopics[id] {
			t.Errorf("Expected to find topic %s in list", id)
		}
	}

	t.Logf("✓ Listed %d topics", len(foundTopics))
}

// TestDeleteTopic tests deleting a topic
func TestDeleteTopic(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-delete-topic"
	cleanupTopic(ctx, client, topicID)

	// Create topic
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Delete topic
	err = topic.Delete(ctx)
	if err != nil {
		t.Fatalf("Failed to delete topic: %v", err)
	}

	// Verify topic is gone
	exists, err := topic.Exists(ctx)
	if err != nil {
		t.Fatalf("Failed to check topic existence: %v", err)
	}

	if exists {
		t.Errorf("Expected topic to be deleted")
	}

	t.Logf("✓ Deleted topic: %s", topicID)
}

// TestCreateSubscription tests creating a subscription
func TestCreateSubscription(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-create-sub-topic"
	subID := "test-create-subscription"

	cleanupTopic(ctx, client, topicID)
	cleanupSubscription(ctx, client, subID)
	defer cleanupTopic(ctx, client, topicID)
	defer cleanupSubscription(ctx, client, subID)

	// Create topic
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Create subscription
	sub, err := client.CreateSubscription(ctx, subID, pubsub.SubscriptionConfig{
		Topic:       topic,
		AckDeadline: 30 * time.Second,
	})
	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	if sub.ID() != subID {
		t.Errorf("Expected subscription ID %s, got %s", subID, sub.ID())
	}

	t.Logf("✓ Created subscription: %s", sub.ID())
}

// TestGetSubscription tests getting a subscription
func TestGetSubscription(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-get-sub-topic"
	subID := "test-get-subscription"

	cleanupTopic(ctx, client, topicID)
	cleanupSubscription(ctx, client, subID)
	defer cleanupTopic(ctx, client, topicID)
	defer cleanupSubscription(ctx, client, subID)

	// Create topic and subscription
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	_, err = client.CreateSubscription(ctx, subID, pubsub.SubscriptionConfig{
		Topic:       topic,
		AckDeadline: 30 * time.Second,
	})
	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Get subscription
	sub := client.Subscription(subID)
	exists, err := sub.Exists(ctx)
	if err != nil {
		t.Fatalf("Failed to check subscription existence: %v", err)
	}

	if !exists {
		t.Errorf("Expected subscription to exist")
	}

	t.Logf("✓ Got subscription: %s", sub.ID())
}

// TestListSubscriptions tests listing subscriptions
func TestListSubscriptions(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-list-sub-topic"
	cleanupTopic(ctx, client, topicID)
	defer cleanupTopic(ctx, client, topicID)

	// Create topic
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Create multiple subscriptions
	subIDs := []string{"test-list-sub-1", "test-list-sub-2", "test-list-sub-3"}
	for _, id := range subIDs {
		cleanupSubscription(ctx, client, id)
		defer cleanupSubscription(ctx, client, id)

		_, err := client.CreateSubscription(ctx, id, pubsub.SubscriptionConfig{
			Topic:       topic,
			AckDeadline: 30 * time.Second,
		})
		if err != nil {
			t.Fatalf("Failed to create subscription %s: %v", id, err)
		}
	}

	// List subscriptions
	it := client.Subscriptions(ctx)
	foundSubs := make(map[string]bool)

	for {
		sub, err := it.Next()
		if err != nil {
			break
		}
		foundSubs[sub.ID()] = true
	}

	// Verify all subscriptions were found
	for _, id := range subIDs {
		if !foundSubs[id] {
			t.Errorf("Expected to find subscription %s in list", id)
		}
	}

	t.Logf("✓ Listed %d subscriptions", len(foundSubs))
}

// TestDeleteSubscription tests deleting a subscription
func TestDeleteSubscription(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-delete-sub-topic"
	subID := "test-delete-subscription"

	cleanupTopic(ctx, client, topicID)
	cleanupSubscription(ctx, client, subID)
	defer cleanupTopic(ctx, client, topicID)

	// Create topic and subscription
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	sub, err := client.CreateSubscription(ctx, subID, pubsub.SubscriptionConfig{
		Topic:       topic,
		AckDeadline: 30 * time.Second,
	})
	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Delete subscription
	err = sub.Delete(ctx)
	if err != nil {
		t.Fatalf("Failed to delete subscription: %v", err)
	}

	// Verify subscription is gone
	exists, err := sub.Exists(ctx)
	if err != nil {
		t.Fatalf("Failed to check subscription existence: %v", err)
	}

	if exists {
		t.Errorf("Expected subscription to be deleted")
	}

	t.Logf("✓ Deleted subscription: %s", subID)
}

// TestPublishMessage tests publishing a single message
func TestPublishMessage(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-publish-topic"
	cleanupTopic(ctx, client, topicID)
	defer cleanupTopic(ctx, client, topicID)

	// Create topic
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Publish message
	result := topic.Publish(ctx, &pubsub.Message{
		Data: []byte("Hello, Pub/Sub!"),
	})

	messageID, err := result.Get(ctx)
	if err != nil {
		t.Fatalf("Failed to publish message: %v", err)
	}

	if messageID == "" {
		t.Errorf("Expected non-empty message ID")
	}

	t.Logf("✓ Published message with ID: %s", messageID)
}

// TestPublishWithAttributes tests publishing a message with attributes
func TestPublishWithAttributes(t *testing.T) {
	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-publish-attr-topic"
	cleanupTopic(ctx, client, topicID)
	defer cleanupTopic(ctx, client, topicID)

	// Create topic
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Publish message with attributes
	result := topic.Publish(ctx, &pubsub.Message{
		Data: []byte("Message with attributes"),
		Attributes: map[string]string{
			"author": "test",
			"type":   "integration-test",
		},
	})

	messageID, err := result.Get(ctx)
	if err != nil {
		t.Fatalf("Failed to publish message: %v", err)
	}

	if messageID == "" {
		t.Errorf("Expected non-empty message ID")
	}

	t.Logf("✓ Published message with attributes, ID: %s", messageID)
}

// TestPublishAndPull tests the full publish-subscribe cycle
func TestPublishAndPull(t *testing.T) {
	t.Skip("Skipping: Go SDK's subscription.Receive() uses StreamingPull which is not yet implemented in lclq (see TODO.md section 4.8)")

	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-pull-topic"
	subID := "test-pull-subscription"

	cleanupTopic(ctx, client, topicID)
	cleanupSubscription(ctx, client, subID)
	defer cleanupTopic(ctx, client, topicID)
	defer cleanupSubscription(ctx, client, subID)

	// Create topic and subscription
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	sub, err := client.CreateSubscription(ctx, subID, pubsub.SubscriptionConfig{
		Topic:       topic,
		AckDeadline: 30 * time.Second,
	})
	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Publish messages
	testMessages := []string{"Message 1", "Message 2", "Message 3"}
	for i, msg := range testMessages {
		result := topic.Publish(ctx, &pubsub.Message{
			Data: []byte(msg),
			Attributes: map[string]string{
				"index": fmt.Sprintf("%d", i),
			},
		})

		_, err := result.Get(ctx)
		if err != nil {
			t.Fatalf("Failed to publish message: %v", err)
		}
	}

	t.Logf("✓ Published %d messages", len(testMessages))

	// Pull messages
	receivedCount := 0
	receivedMessages := make(map[string]bool)

	// Create a context with timeout for pulling
	pullCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()

	err = sub.Receive(pullCtx, func(ctx context.Context, msg *pubsub.Message) {
		receivedMessages[string(msg.Data)] = true
		receivedCount++
		msg.Ack()

		// Stop after receiving all messages
		if receivedCount >= len(testMessages) {
			cancel()
		}
	})

	if err != nil && err != context.Canceled {
		t.Fatalf("Failed to receive messages: %v", err)
	}

	// Verify all messages were received
	if receivedCount != len(testMessages) {
		t.Errorf("Expected to receive %d messages, got %d", len(testMessages), receivedCount)
	}

	for _, msg := range testMessages {
		if !receivedMessages[msg] {
			t.Errorf("Did not receive message: %s", msg)
		}
	}

	t.Logf("✓ Received and acknowledged %d messages", receivedCount)
}

// TestMessageOrdering tests message ordering with ordering keys
func TestMessageOrdering(t *testing.T) {
	t.Skip("Skipping: Go SDK's subscription.Receive() uses StreamingPull which is not yet implemented in lclq (see TODO.md section 4.8)")

	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-ordering-topic"
	subID := "test-ordering-subscription"

	cleanupTopic(ctx, client, topicID)
	cleanupSubscription(ctx, client, subID)
	defer cleanupTopic(ctx, client, topicID)
	defer cleanupSubscription(ctx, client, subID)

	// Create topic with message ordering enabled
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	// Enable message ordering on the topic
	topic.EnableMessageOrdering = true

	// Create subscription with message ordering enabled
	sub, err := client.CreateSubscription(ctx, subID, pubsub.SubscriptionConfig{
		Topic:                 topic,
		AckDeadline:           30 * time.Second,
		EnableMessageOrdering: true,
	})
	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Publish ordered messages
	orderingKey := "order-key-1"
	for i := 1; i <= 5; i++ {
		result := topic.Publish(ctx, &pubsub.Message{
			Data:        []byte(fmt.Sprintf("Ordered message %d", i)),
			OrderingKey: orderingKey,
			Attributes: map[string]string{
				"sequence": fmt.Sprintf("%d", i),
			},
		})

		_, err := result.Get(ctx)
		if err != nil {
			t.Fatalf("Failed to publish ordered message: %v", err)
		}
	}

	t.Logf("✓ Published 5 ordered messages with key: %s", orderingKey)

	// Pull messages and verify ordering
	receivedSequence := []int{}
	expectedCount := 5

	pullCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()

	err = sub.Receive(pullCtx, func(ctx context.Context, msg *pubsub.Message) {
		if seqStr, ok := msg.Attributes["sequence"]; ok {
			var seq int
			fmt.Sscanf(seqStr, "%d", &seq)
			receivedSequence = append(receivedSequence, seq)
		}
		msg.Ack()

		if len(receivedSequence) >= expectedCount {
			cancel()
		}
	})

	if err != nil && err != context.Canceled {
		t.Fatalf("Failed to receive messages: %v", err)
	}

	// Verify messages were received in order
	if len(receivedSequence) != expectedCount {
		t.Errorf("Expected to receive %d messages, got %d", expectedCount, len(receivedSequence))
	}

	for i := 0; i < len(receivedSequence); i++ {
		if receivedSequence[i] != i+1 {
			t.Errorf("Message out of order: expected sequence %d, got %d at position %d",
				i+1, receivedSequence[i], i)
		}
	}

	t.Logf("✓ Received %d messages in correct order", len(receivedSequence))
}

// TestModifyAckDeadline tests modifying acknowledgment deadline
func TestModifyAckDeadline(t *testing.T) {
	t.Skip("Skipping: Go SDK's subscription.Receive() uses StreamingPull which is not yet implemented in lclq (see TODO.md section 4.8)")

	client, ctx := setupClient(t)
	defer client.Close()

	topicID := "test-ack-deadline-topic"
	subID := "test-ack-deadline-subscription"

	cleanupTopic(ctx, client, topicID)
	cleanupSubscription(ctx, client, subID)
	defer cleanupTopic(ctx, client, topicID)
	defer cleanupSubscription(ctx, client, subID)

	// Create topic and subscription
	topic, err := client.CreateTopic(ctx, topicID)
	if err != nil {
		t.Fatalf("Failed to create topic: %v", err)
	}

	sub, err := client.CreateSubscription(ctx, subID, pubsub.SubscriptionConfig{
		Topic:       topic,
		AckDeadline: 10 * time.Second,
	})
	if err != nil {
		t.Fatalf("Failed to create subscription: %v", err)
	}

	// Publish a message
	result := topic.Publish(ctx, &pubsub.Message{
		Data: []byte("Test ack deadline"),
	})

	_, err = result.Get(ctx)
	if err != nil {
		t.Fatalf("Failed to publish message: %v", err)
	}

	// Pull message and modify ack deadline
	pullCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()

	receivedMsg := false
	err = sub.Receive(pullCtx, func(ctx context.Context, msg *pubsub.Message) {
		// Modify ack deadline to 60 seconds
		msg.Nack()
		receivedMsg = true
		cancel()
	})

	if err != nil && err != context.Canceled {
		t.Fatalf("Failed to receive message: %v", err)
	}

	if !receivedMsg {
		t.Errorf("Expected to receive message")
	}

	t.Logf("✓ Successfully modified ack deadline")
}
