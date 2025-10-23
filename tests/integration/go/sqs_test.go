package main

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/sqs"
	"github.com/aws/aws-sdk-go-v2/service/sqs/types"
)

const (
	sqsEndpoint = "http://localhost:9324"
	region      = "us-east-1"
)

// createSQSClient creates a configured SQS client
func createSQSClient() *sqs.Client {
	customResolver := aws.EndpointResolverWithOptionsFunc(
		func(service, region string, options ...interface{}) (aws.Endpoint, error) {
			return aws.Endpoint{
				URL:           sqsEndpoint,
				SigningRegion: region,
			}, nil
		})

	cfg, err := config.LoadDefaultConfig(context.TODO(),
		config.WithRegion(region),
		config.WithEndpointResolverWithOptions(customResolver),
		config.WithCredentialsProvider(credentials.NewStaticCredentialsProvider("dummy", "dummy", "")),
	)
	if err != nil {
		panic(fmt.Sprintf("Failed to load config: %v", err))
	}

	return sqs.NewFromConfig(cfg)
}

// Test 1: Basic Queue Operations
func TestBasicQueueOperations(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-basic-%d", time.Now().UnixNano())

	// Create queue
	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
	})
	if err != nil {
		t.Fatalf("Failed to create queue: %v", err)
	}
	queueURL := createResult.QueueUrl
	if queueURL == nil || *queueURL == "" {
		t.Fatal("Queue URL should not be empty")
	}
	t.Logf("Created queue: %s", *queueURL)

	// Send message
	messageBody := "Hello from Go!"
	sendResult, err := client.SendMessage(ctx, &sqs.SendMessageInput{
		QueueUrl:    queueURL,
		MessageBody: aws.String(messageBody),
	})
	if err != nil {
		t.Fatalf("Failed to send message: %v", err)
	}
	if sendResult.MessageId == nil {
		t.Fatal("Message ID should not be nil")
	}
	t.Logf("Sent message: %s", *sendResult.MessageId)

	// Receive message
	receiveResult, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:            queueURL,
		MaxNumberOfMessages: 1,
		AttributeNames:      []types.QueueAttributeName{types.QueueAttributeNameAll},
	})
	if err != nil {
		t.Fatalf("Failed to receive message: %v", err)
	}
	if len(receiveResult.Messages) != 1 {
		t.Fatalf("Expected 1 message, got %d", len(receiveResult.Messages))
	}
	if *receiveResult.Messages[0].Body != messageBody {
		t.Fatalf("Expected body '%s', got '%s'", messageBody, *receiveResult.Messages[0].Body)
	}
	if len(receiveResult.Messages[0].Attributes) == 0 {
		t.Fatal("Message should have attributes")
	}
	t.Logf("Received message with attributes: %v", receiveResult.Messages[0].Attributes)

	// Delete message
	_, err = client.DeleteMessage(ctx, &sqs.DeleteMessageInput{
		QueueUrl:      queueURL,
		ReceiptHandle: receiveResult.Messages[0].ReceiptHandle,
	})
	if err != nil {
		t.Fatalf("Failed to delete message: %v", err)
	}
	t.Log("Deleted message")

	// Delete queue
	_, err = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{
		QueueUrl: queueURL,
	})
	if err != nil {
		t.Fatalf("Failed to delete queue: %v", err)
	}
	t.Log("Deleted queue")
}

// Test 2: Message Attributes
func TestMessageAttributes(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-attrs-%d", time.Now().UnixNano())

	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
	})
	if err != nil {
		t.Fatalf("Failed to create queue: %v", err)
	}
	queueURL := createResult.QueueUrl

	// Send message with attributes
	_, err = client.SendMessage(ctx, &sqs.SendMessageInput{
		QueueUrl:    queueURL,
		MessageBody: aws.String("Test message"),
		MessageAttributes: map[string]types.MessageAttributeValue{
			"Author": {
				DataType:    aws.String("String"),
				StringValue: aws.String("Go SDK"),
			},
			"Priority": {
				DataType:    aws.String("Number"),
				StringValue: aws.String("5"),
			},
		},
	})
	if err != nil {
		t.Fatalf("Failed to send message: %v", err)
	}
	t.Log("Sent message with 2 attributes")

	// Receive and verify attributes
	receiveResult, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:               queueURL,
		MaxNumberOfMessages:    1,
		MessageAttributeNames:  []string{"All"},
	})
	if err != nil {
		t.Fatalf("Failed to receive message: %v", err)
	}

	if len(receiveResult.Messages) != 1 {
		t.Fatalf("Expected 1 message, got %d", len(receiveResult.Messages))
	}

	attrs := receiveResult.Messages[0].MessageAttributes
	if len(attrs) == 0 {
		t.Fatal("Message should have attributes")
	}

	author, ok := attrs["Author"]
	if !ok || *author.StringValue != "Go SDK" {
		t.Fatalf("Expected Author='Go SDK', got %v", author)
	}

	priority, ok := attrs["Priority"]
	if !ok || *priority.StringValue != "5" {
		t.Fatalf("Expected Priority='5', got %v", priority)
	}

	t.Logf("Verified attributes: Author=%s, Priority=%s", *author.StringValue, *priority.StringValue)

	// Cleanup
	_, _ = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{QueueUrl: queueURL})
}

// Test 3: FIFO Queue
func TestFifoQueue(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-fifo-%d.fifo", time.Now().UnixNano())

	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
		Attributes: map[string]string{
			"FifoQueue":                  "true",
			"ContentBasedDeduplication": "true",
		},
	})
	if err != nil {
		t.Fatalf("Failed to create FIFO queue: %v", err)
	}
	queueURL := createResult.QueueUrl
	t.Logf("Created FIFO queue: %s", *queueURL)

	// Send messages in order
	messages := []string{"First", "Second", "Third"}
	for _, msg := range messages {
		_, err := client.SendMessage(ctx, &sqs.SendMessageInput{
			QueueUrl:       queueURL,
			MessageBody:    aws.String(msg),
			MessageGroupId: aws.String("test-group"),
		})
		if err != nil {
			t.Fatalf("Failed to send message: %v", err)
		}
	}
	t.Log("Sent 3 messages in order")

	// Receive and verify order
	var receivedBodies []string
	for i := 0; i < 3; i++ {
		receiveResult, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
			QueueUrl:            queueURL,
			MaxNumberOfMessages: 1,
		})
		if err != nil {
			t.Fatalf("Failed to receive message: %v", err)
		}
		if len(receiveResult.Messages) > 0 {
			receivedBodies = append(receivedBodies, *receiveResult.Messages[0].Body)
			_, _ = client.DeleteMessage(ctx, &sqs.DeleteMessageInput{
				QueueUrl:      queueURL,
				ReceiptHandle: receiveResult.Messages[0].ReceiptHandle,
			})
		}
	}

	if !reflect.DeepEqual(receivedBodies, messages) {
		t.Fatalf("Expected order %v, got %v", messages, receivedBodies)
	}
	t.Logf("Verified FIFO ordering: %v", receivedBodies)

	// Cleanup
	_, _ = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{QueueUrl: queueURL})
}

// Test 4: Batch Operations
func TestBatchOperations(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-batch-%d", time.Now().UnixNano())

	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
	})
	if err != nil {
		t.Fatalf("Failed to create queue: %v", err)
	}
	queueURL := createResult.QueueUrl

	// Send batch
	batchResult, err := client.SendMessageBatch(ctx, &sqs.SendMessageBatchInput{
		QueueUrl: queueURL,
		Entries: []types.SendMessageBatchRequestEntry{
			{Id: aws.String("1"), MessageBody: aws.String("Message 1")},
			{Id: aws.String("2"), MessageBody: aws.String("Message 2")},
			{Id: aws.String("3"), MessageBody: aws.String("Message 3")},
		},
	})
	if err != nil {
		t.Fatalf("Failed to send batch: %v", err)
	}
	if len(batchResult.Successful) != 3 {
		t.Fatalf("Expected 3 successful, got %d", len(batchResult.Successful))
	}
	t.Log("Sent batch of 3 messages")

	// Receive messages
	receiveResult, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:            queueURL,
		MaxNumberOfMessages: 10,
	})
	if err != nil {
		t.Fatalf("Failed to receive messages: %v", err)
	}
	if len(receiveResult.Messages) != 3 {
		t.Fatalf("Expected 3 messages, got %d", len(receiveResult.Messages))
	}
	t.Log("Received 3 messages")

	// Delete batch
	var deleteEntries []types.DeleteMessageBatchRequestEntry
	for i, msg := range receiveResult.Messages {
		deleteEntries = append(deleteEntries, types.DeleteMessageBatchRequestEntry{
			Id:            aws.String(fmt.Sprintf("%d", i+1)),
			ReceiptHandle: msg.ReceiptHandle,
		})
	}

	deleteResult, err := client.DeleteMessageBatch(ctx, &sqs.DeleteMessageBatchInput{
		QueueUrl: queueURL,
		Entries:  deleteEntries,
	})
	if err != nil {
		t.Fatalf("Failed to delete batch: %v", err)
	}
	if len(deleteResult.Successful) != 3 {
		t.Fatalf("Expected 3 deletes successful, got %d", len(deleteResult.Successful))
	}
	t.Log("Deleted batch of 3 messages")

	// Cleanup
	_, _ = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{QueueUrl: queueURL})
}

// Test 5: Queue Attributes
func TestQueueAttributes(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-qattrs-%d", time.Now().UnixNano())

	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
		Attributes: map[string]string{
			"VisibilityTimeout":      "60",
			"MessageRetentionPeriod": "86400",
		},
	})
	if err != nil {
		t.Fatalf("Failed to create queue: %v", err)
	}
	queueURL := createResult.QueueUrl

	// Get attributes
	getResult, err := client.GetQueueAttributes(ctx, &sqs.GetQueueAttributesInput{
		QueueUrl:       queueURL,
		AttributeNames: []types.QueueAttributeName{types.QueueAttributeNameAll},
	})
	if err != nil {
		t.Fatalf("Failed to get attributes: %v", err)
	}

	if len(getResult.Attributes) == 0 {
		t.Fatal("Should have attributes")
	}

	if getResult.Attributes["VisibilityTimeout"] != "60" {
		t.Fatalf("Expected VisibilityTimeout=60, got %s", getResult.Attributes["VisibilityTimeout"])
	}

	if getResult.Attributes["MessageRetentionPeriod"] != "86400" {
		t.Fatalf("Expected MessageRetentionPeriod=86400, got %s", getResult.Attributes["MessageRetentionPeriod"])
	}

	t.Logf("Retrieved attributes: VisibilityTimeout=%s", getResult.Attributes["VisibilityTimeout"])

	// Set attributes
	_, err = client.SetQueueAttributes(ctx, &sqs.SetQueueAttributesInput{
		QueueUrl: queueURL,
		Attributes: map[string]string{
			"VisibilityTimeout": "120",
		},
	})
	if err != nil {
		t.Fatalf("Failed to set attributes: %v", err)
	}
	t.Log("Updated VisibilityTimeout to 120")

	// Verify update
	verifyResult, err := client.GetQueueAttributes(ctx, &sqs.GetQueueAttributesInput{
		QueueUrl:       queueURL,
		AttributeNames: []types.QueueAttributeName{types.QueueAttributeNameVisibilityTimeout},
	})
	if err != nil {
		t.Fatalf("Failed to verify attributes: %v", err)
	}

	if verifyResult.Attributes["VisibilityTimeout"] != "120" {
		t.Fatalf("Expected VisibilityTimeout=120, got %s", verifyResult.Attributes["VisibilityTimeout"])
	}
	t.Logf("Verified update: VisibilityTimeout=%s", verifyResult.Attributes["VisibilityTimeout"])

	// Cleanup
	_, _ = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{QueueUrl: queueURL})
}

// Test 6: Change Message Visibility
func TestChangeMessageVisibility(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-visibility-%d", time.Now().UnixNano())

	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
	})
	if err != nil {
		t.Fatalf("Failed to create queue: %v", err)
	}
	queueURL := createResult.QueueUrl

	// Send message
	_, err = client.SendMessage(ctx, &sqs.SendMessageInput{
		QueueUrl:    queueURL,
		MessageBody: aws.String("Test visibility"),
	})
	if err != nil {
		t.Fatalf("Failed to send message: %v", err)
	}

	// Receive message
	receiveResult, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:            queueURL,
		MaxNumberOfMessages: 1,
	})
	if err != nil {
		t.Fatalf("Failed to receive message: %v", err)
	}
	if len(receiveResult.Messages) != 1 {
		t.Fatalf("Expected 1 message, got %d", len(receiveResult.Messages))
	}
	receiptHandle := receiveResult.Messages[0].ReceiptHandle
	t.Log("Received message")

	// Change visibility to 0 (return to queue immediately)
	_, err = client.ChangeMessageVisibility(ctx, &sqs.ChangeMessageVisibilityInput{
		QueueUrl:          queueURL,
		ReceiptHandle:     receiptHandle,
		VisibilityTimeout: 0,
	})
	if err != nil {
		t.Fatalf("Failed to change visibility: %v", err)
	}
	t.Log("Changed visibility timeout to 0")

	// Should be able to receive again immediately
	receiveAgain, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:            queueURL,
		MaxNumberOfMessages: 1,
	})
	if err != nil {
		t.Fatalf("Failed to receive message again: %v", err)
	}
	if len(receiveAgain.Messages) != 1 {
		t.Fatalf("Expected to receive message again, got %d", len(receiveAgain.Messages))
	}
	t.Log("Received message again immediately")

	// Cleanup
	_, _ = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{QueueUrl: queueURL})
}

// Test 7: Delay Queue
func TestDelayQueue(t *testing.T) {
	client := createSQSClient()
	ctx := context.Background()
	queueName := fmt.Sprintf("test-delay-%d", time.Now().UnixNano())

	createResult, err := client.CreateQueue(ctx, &sqs.CreateQueueInput{
		QueueName: aws.String(queueName),
		Attributes: map[string]string{
			"DelaySeconds": "2",
		},
	})
	if err != nil {
		t.Fatalf("Failed to create delay queue: %v", err)
	}
	queueURL := createResult.QueueUrl
	t.Log("Created delay queue (2 second delay)")

	// Send message
	sendTime := time.Now()
	_, err = client.SendMessage(ctx, &sqs.SendMessageInput{
		QueueUrl:    queueURL,
		MessageBody: aws.String("Delayed message"),
	})
	if err != nil {
		t.Fatalf("Failed to send message: %v", err)
	}
	t.Logf("Sent message at %s", sendTime.Format("15:04:05"))

	// Try immediate receive (should be empty)
	immediate, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:            queueURL,
		MaxNumberOfMessages: 1,
	})
	if err != nil {
		t.Fatalf("Failed immediate receive: %v", err)
	}
	if len(immediate.Messages) != 0 {
		t.Fatal("Should not receive message immediately")
	}
	t.Log("No message received immediately (correct)")

	// Wait for delay
	t.Log("Waiting 3 seconds...")
	time.Sleep(3 * time.Second)

	// Receive should work now
	delayed, err := client.ReceiveMessage(ctx, &sqs.ReceiveMessageInput{
		QueueUrl:            queueURL,
		MaxNumberOfMessages: 1,
	})
	if err != nil {
		t.Fatalf("Failed delayed receive: %v", err)
	}
	receiveTime := time.Now()
	if len(delayed.Messages) != 1 {
		t.Fatalf("Expected 1 message after delay, got %d", len(delayed.Messages))
	}

	elapsed := receiveTime.Sub(sendTime).Seconds()
	t.Logf("Received message after %.2f seconds", elapsed)

	// Cleanup
	_, _ = client.DeleteQueue(ctx, &sqs.DeleteQueueInput{QueueUrl: queueURL})
}
