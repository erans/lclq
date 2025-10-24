package main

import (
	"context"
	"fmt"
	"time"

	"cloud.google.com/go/pubsub"
	"google.golang.org/api/option"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func main() {
	ctx := context.Background()

	conn, err := grpc.Dial(
		"localhost:8085",
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		panic(err)
	}

	client, err := pubsub.NewClient(ctx, "test-project", option.WithGRPCConn(conn))
	if err != nil {
		panic(err)
	}
	defer client.Close()

	// Create topic
	topic, err := client.CreateTopic(ctx, "test-ordering-debug")
	if err != nil {
		fmt.Printf("CreateTopic error (might already exist): %v\n", err)
		topic = client.Topic("test-ordering-debug")
	}

	// Enable ordering
	topic.EnableMessageOrdering = true
	
	fmt.Println("Publishing 5 messages with ordering...")
	for i := 1; i <= 5; i++ {
		fmt.Printf("Publishing message %d...\n", i)
		result := topic.Publish(ctx, &pubsub.Message{
			Data:        []byte(fmt.Sprintf("Message %d", i)),
			OrderingKey: "order-key-1",
		})

		msgID, err := result.Get(ctx)
		if err != nil {
			fmt.Printf("ERROR publishing message %d: %v\n", i, err)
		} else {
			fmt.Printf("Published message %d with ID: %s\n", i, msgID)
		}
	}

	fmt.Println("Calling topic.Stop()...")
	topic.Stop()
	fmt.Println("topic.Stop() returned")

	time.Sleep(2 * time.Second)
	fmt.Println("Done!")
}
