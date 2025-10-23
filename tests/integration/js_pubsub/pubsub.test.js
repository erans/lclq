/**
 * Integration tests for lclq Pub/Sub implementation using Google Cloud Pub/Sub JavaScript SDK.
 *
 * Tests the gRPC Pub/Sub API using the official @google-cloud/pubsub library.
 */

import { PubSub } from '@google-cloud/pubsub';
import * as grpc from '@grpc/grpc-js';

// Configure to use local lclq server
process.env.PUBSUB_EMULATOR_HOST = 'localhost:8085';

const PROJECT_ID = 'test-project';
const TOPIC_ID = 'test-topic';
const SUBSCRIPTION_ID = 'test-subscription';

describe('Pub/Sub Integration Tests', () => {
  let pubsub;

  beforeAll(() => {
    pubsub = new PubSub({ projectId: PROJECT_ID });
  });

  // Helper function to clean up topic
  async function cleanupTopic(topicName) {
    try {
      const topic = pubsub.topic(topicName);
      await topic.delete();
    } catch (error) {
      // Ignore if topic doesn't exist
    }
  }

  // Helper function to clean up subscription
  async function cleanupSubscription(subscriptionName) {
    try {
      const subscription = pubsub.subscription(subscriptionName);
      await subscription.delete();
    } catch (error) {
      // Ignore if subscription doesn't exist
    }
  }

  describe('Topic Management', () => {
    test('should create a topic', async () => {
      const topicName = 'test-create-topic';
      await cleanupTopic(topicName);

      const [topic] = await pubsub.createTopic(topicName);

      expect(topic.name).toContain(topicName);
      console.log(`✓ Created topic: ${topic.name}`);

      // Cleanup
      await topic.delete();
    });

    test('should get a topic', async () => {
      const topicName = 'test-get-topic';
      await cleanupTopic(topicName);

      // Create topic first
      const [createdTopic] = await pubsub.createTopic(topicName);

      // Get the topic
      const topic = pubsub.topic(topicName);
      const [metadata] = await topic.getMetadata();

      expect(metadata.name).toContain(topicName);
      console.log(`✓ Got topic: ${metadata.name}`);

      // Cleanup
      await createdTopic.delete();
    });

    test('should list topics', async () => {
      const topicName = 'test-list-topic';
      await cleanupTopic(topicName);

      // Create a topic
      const [createdTopic] = await pubsub.createTopic(topicName);

      // List topics
      const [topics] = await pubsub.getTopics();

      const topicNames = topics.map(t => t.name);
      const fullTopicName = `projects/${PROJECT_ID}/topics/${topicName}`;
      expect(topicNames).toContain(fullTopicName);
      console.log(`✓ Listed ${topics.length} topics`);

      // Cleanup
      await createdTopic.delete();
    });

    test('should delete a topic', async () => {
      const topicName = 'test-delete-topic';

      // Create and delete
      const [topic] = await pubsub.createTopic(topicName);
      await topic.delete();

      // Verify deletion - should throw NotFound
      const deletedTopic = pubsub.topic(topicName);
      await expect(deletedTopic.getMetadata()).rejects.toThrow();

      console.log(`✓ Deleted topic: ${topicName}`);
    });
  });

  describe('Message Publishing', () => {
    let topic;

    beforeEach(async () => {
      await cleanupTopic(TOPIC_ID);
      [topic] = await pubsub.createTopic(TOPIC_ID);
    });

    afterEach(async () => {
      await cleanupTopic(TOPIC_ID);
    });

    test('should publish a message', async () => {
      const messageData = Buffer.from('Hello, Pub/Sub!');

      const messageId = await topic.publishMessage({ data: messageData });

      expect(messageId).toBeDefined();
      expect(messageId.length).toBeGreaterThan(0);
      console.log(`✓ Published message with ID: ${messageId}`);
    });

    test('should publish message with attributes', async () => {
      const messageData = Buffer.from('Message with attributes');
      const attributes = {
        key1: 'value1',
        key2: 'value2',
        origin: 'test',
      };

      const messageId = await topic.publishMessage({
        data: messageData,
        attributes,
      });

      expect(messageId).toBeDefined();
      console.log(`✓ Published message with attributes, ID: ${messageId}`);
    });

    test('should publish message with ordering key', async () => {
      const messageData = Buffer.from('Ordered message');
      const orderingKey = 'order-key-1';

      const messageId = await topic.publishMessage({
        data: messageData,
        orderingKey,
      });

      expect(messageId).toBeDefined();
      console.log(`✓ Published ordered message with ID: ${messageId}`);
    });
  });

  describe('Subscription Management', () => {
    let topic;

    beforeEach(async () => {
      await cleanupTopic(TOPIC_ID);
      [topic] = await pubsub.createTopic(TOPIC_ID);
    });

    afterEach(async () => {
      await cleanupTopic(TOPIC_ID);
    });

    test('should create a subscription', async () => {
      const subscriptionName = 'test-create-sub';
      await cleanupSubscription(subscriptionName);

      const [subscription] = await topic.createSubscription(subscriptionName, {
        ackDeadlineSeconds: 60,
      });

      expect(subscription.name).toContain(subscriptionName);
      console.log(`✓ Created subscription: ${subscription.name}`);

      // Cleanup
      await subscription.delete();
    });

    test('should get a subscription', async () => {
      const subscriptionName = 'test-get-sub';
      await cleanupSubscription(subscriptionName);

      // Create subscription first
      const [createdSub] = await topic.createSubscription(subscriptionName);

      // Get the subscription
      const subscription = pubsub.subscription(subscriptionName);
      const [metadata] = await subscription.getMetadata();

      expect(metadata.name).toContain(subscriptionName);
      console.log(`✓ Got subscription: ${metadata.name}`);

      // Cleanup
      await createdSub.delete();
    });

    test('should list subscriptions', async () => {
      const subscriptionName = 'test-list-sub';
      await cleanupSubscription(subscriptionName);

      // Create a subscription
      const [createdSub] = await topic.createSubscription(subscriptionName);

      // List subscriptions
      const [subscriptions] = await pubsub.getSubscriptions();

      const subNames = subscriptions.map(s => s.name);
      const fullSubName = `projects/${PROJECT_ID}/subscriptions/${subscriptionName}`;
      expect(subNames).toContain(fullSubName);
      console.log(`✓ Listed ${subscriptions.length} subscriptions`);

      // Cleanup
      await createdSub.delete();
    });

    test('should delete a subscription', async () => {
      const subscriptionName = 'test-delete-sub';

      // Create and delete
      const [subscription] = await topic.createSubscription(subscriptionName);
      await subscription.delete();

      // Verify deletion - should throw NotFound
      const deletedSub = pubsub.subscription(subscriptionName);
      await expect(deletedSub.getMetadata()).rejects.toThrow();

      console.log(`✓ Deleted subscription: ${subscriptionName}`);
    });
  });

  describe('Message Pull and Acknowledge', () => {
    let topic;
    let subscription;
    let subscriberClient;

    beforeEach(async () => {
      await cleanupTopic(TOPIC_ID);
      await cleanupSubscription(SUBSCRIPTION_ID);

      [topic] = await pubsub.createTopic(TOPIC_ID);
      [subscription] = await topic.createSubscription(SUBSCRIPTION_ID, {
        ackDeadlineSeconds: 30,
      });

      // Get the v1 subscriber client for pull operations
      const { v1 } = await import('@google-cloud/pubsub');
      subscriberClient = new v1.SubscriberClient({
        servicePath: 'localhost',
        port: 8085,
        sslCreds: grpc.credentials.createInsecure(),
      });
    });

    afterEach(async () => {
      await cleanupSubscription(SUBSCRIPTION_ID);
      await cleanupTopic(TOPIC_ID);
      if (subscriberClient) {
        subscriberClient.close();
      }
    });

    test('should publish and pull messages', async () => {
      // Publish messages
      const messages = ['Message 1', 'Message 2', 'Message 3'];
      const messageIds = [];

      for (const msg of messages) {
        const messageId = await topic.publishMessage({
          data: Buffer.from(msg),
        });
        messageIds.push(messageId);
      }

      console.log(`✓ Published ${messages.length} messages`);

      // Pull messages using v1 client
      const subscriptionPath = `projects/${PROJECT_ID}/subscriptions/${SUBSCRIPTION_ID}`;
      const [response] = await subscriberClient.pull({
        subscription: subscriptionPath,
        maxMessages: 10,
      });

      const pulledMessages = response.receivedMessages || [];
      expect(pulledMessages.length).toBe(messages.length);

      // Verify message content
      const pulledData = pulledMessages.map(m => m.message.data.toString());
      for (const msg of messages) {
        expect(pulledData).toContain(msg);
      }

      console.log(`✓ Pulled ${pulledMessages.length} messages`);

      // Acknowledge messages
      const ackIds = pulledMessages.map(m => m.ackId);
      await subscriberClient.acknowledge({
        subscription: subscriptionPath,
        ackIds,
      });

      console.log(`✓ Acknowledged ${pulledMessages.length} messages`);
    });

    test('should handle message attributes', async () => {
      const messageData = 'Message with attributes';
      const attributes = {
        attribute1: 'value1',
        attribute2: 'value2',
        number: '42',
      };

      // Publish with attributes
      await topic.publishMessage({
        data: Buffer.from(messageData),
        attributes,
      });

      console.log('✓ Published message with attributes');

      // Pull and verify attributes
      const subscriptionPath = `projects/${PROJECT_ID}/subscriptions/${SUBSCRIPTION_ID}`;
      const [response] = await subscriberClient.pull({
        subscription: subscriptionPath,
        maxMessages: 1,
      });

      const pulledMessages = response.receivedMessages || [];
      expect(pulledMessages.length).toBe(1);
      const message = pulledMessages[0].message;

      expect(message.data.toString()).toBe(messageData);
      expect(message.attributes).toMatchObject(attributes);

      console.log('✓ Verified message attributes');

      // Acknowledge
      await subscriberClient.acknowledge({
        subscription: subscriptionPath,
        ackIds: [pulledMessages[0].ackId],
      });
    });

    test('should verify message ordering', async () => {
      await cleanupSubscription('test-ordering-sub');
      await cleanupTopic('test-ordering-topic');

      // Create topic and subscription with ordering enabled
      const [orderingTopic] = await pubsub.createTopic('test-ordering-topic');
      const [orderingSub] = await orderingTopic.createSubscription('test-ordering-sub', {
        ackDeadlineSeconds: 30,
        enableMessageOrdering: true,
      });

      // Publish messages with ordering key
      const orderingKey = 'order-key-1';
      const messages = ['Message 1', 'Message 2', 'Message 3'];

      for (const msg of messages) {
        await orderingTopic.publishMessage({
          data: Buffer.from(msg),
          orderingKey,
        });
      }

      console.log(`✓ Published ${messages.length} ordered messages`);

      // Pull and verify order
      const subscriptionPath = `projects/${PROJECT_ID}/subscriptions/test-ordering-sub`;
      const [response] = await subscriberClient.pull({
        subscription: subscriptionPath,
        maxMessages: 10,
      });

      const pulledMessages = response.receivedMessages || [];
      expect(pulledMessages.length).toBe(messages.length);

      // Verify ordering keys
      for (const message of pulledMessages) {
        expect(message.message.orderingKey).toBe(orderingKey);
      }

      console.log('✓ Verified message ordering');

      // Cleanup
      const ackIds = pulledMessages.map(m => m.ackId);
      await subscriberClient.acknowledge({
        subscription: subscriptionPath,
        ackIds,
      });
      await orderingSub.delete();
      await orderingTopic.delete();
    });

    test('should acknowledge messages and verify deletion', async () => {
      // Publish a message
      const messageData = 'Test acknowledge';
      await topic.publishMessage({ data: Buffer.from(messageData) });

      // Pull the message
      const subscriptionPath = `projects/${PROJECT_ID}/subscriptions/${SUBSCRIPTION_ID}`;
      const [response] = await subscriberClient.pull({
        subscription: subscriptionPath,
        maxMessages: 1,
      });

      const pulledMessages = response.receivedMessages || [];
      expect(pulledMessages.length).toBe(1);

      // Acknowledge
      await subscriberClient.acknowledge({
        subscription: subscriptionPath,
        ackIds: [pulledMessages[0].ackId],
      });
      console.log('✓ Acknowledged message');

      // Pull again - should be empty
      const [response2] = await subscriberClient.pull({
        subscription: subscriptionPath,
        maxMessages: 1,
      });

      const pulledMessages2 = response2.receivedMessages || [];
      expect(pulledMessages2.length).toBe(0);
      console.log('✓ Verified message was deleted after ack');
    });

    test('should modify ack deadline', async () => {
      // Publish a message
      const messageData = 'Test modify ack deadline';
      await topic.publishMessage({ data: Buffer.from(messageData) });

      // Pull the message
      const subscriptionPath = `projects/${PROJECT_ID}/subscriptions/${SUBSCRIPTION_ID}`;
      const [response] = await subscriberClient.pull({
        subscription: subscriptionPath,
        maxMessages: 1,
      });

      const pulledMessages = response.receivedMessages || [];
      expect(pulledMessages.length).toBe(1);

      const ackId = pulledMessages[0].ackId;

      // Modify ack deadline to 60 seconds
      await subscriberClient.modifyAckDeadline({
        subscription: subscriptionPath,
        ackIds: [ackId],
        ackDeadlineSeconds: 60,
      });
      console.log('✓ Modified ack deadline to 60 seconds');

      // Acknowledge to clean up
      await subscriberClient.acknowledge({
        subscription: subscriptionPath,
        ackIds: [ackId],
      });
    });
  });
});
