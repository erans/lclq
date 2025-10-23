#!/usr/bin/env node
/**
 * lclq Advanced Integration Tests with AWS SDK for JavaScript v3
 *
 * Tests the lclq SQS implementation using the official AWS SDK.
 * Runs against a local lclq server on http://localhost:9324
 */

import {
  SQSClient,
  CreateQueueCommand,
  SendMessageCommand,
  ReceiveMessageCommand,
  DeleteMessageCommand,
  DeleteQueueCommand,
  GetQueueAttributesCommand,
  SetQueueAttributesCommand,
  ChangeMessageVisibilityCommand,
  PurgeQueueCommand,
  ListQueuesCommand,
  SendMessageBatchCommand,
  DeleteMessageBatchCommand,
  ChangeMessageVisibilityBatchCommand,
} from '@aws-sdk/client-sqs';

const SQS_ENDPOINT = 'http://localhost:9324';
const REGION = 'us-east-1';

// Create SQS client
const sqs = new SQSClient({
  endpoint: SQS_ENDPOINT,
  region: REGION,
  credentials: {
    accessKeyId: 'dummy',
    secretAccessKey: 'dummy',
  },
});

// Test utilities
let testsPassed = 0;
let testsFailed = 0;
const failures = [];

function assert(condition, message) {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
}

async function runTest(name, testFn) {
  process.stdout.write(`\n🧪 Test: ${name}\n`);
  try {
    await testFn();
    process.stdout.write(`✅ ${name} PASSED\n`);
    testsPassed++;
  } catch (error) {
    process.stdout.write(`❌ ${name} FAILED: ${error.message}\n`);
    if (error.stack) {
      process.stdout.write(`   ${error.stack.split('\n').slice(1, 3).join('\n   ')}\n`);
    }
    testsFailed++;
    failures.push({ name, error: error.message });
  }
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Test 1: Basic Queue Operations
async function testBasicQueueOperations() {
  const queueName = `test-basic-${Date.now()}`;

  // Create queue
  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
  }));
  const queueUrl = createResult.QueueUrl;
  assert(queueUrl, 'Queue URL should be returned');
  process.stdout.write(`   Created queue: ${queueUrl}\n`);

  // Send message
  const sendResult = await sqs.send(new SendMessageCommand({
    QueueUrl: queueUrl,
    MessageBody: 'Hello from JavaScript!',
  }));
  assert(sendResult.MessageId, 'Message ID should be returned');
  process.stdout.write(`   Sent message: ${sendResult.MessageId}\n`);

  // Receive message
  const receiveResult = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 1,
    AttributeNames: ['All'],
  }));
  assert(receiveResult.Messages && receiveResult.Messages.length === 1, 'Should receive 1 message');
  assert(receiveResult.Messages[0].Body === 'Hello from JavaScript!', 'Message body should match');
  assert(receiveResult.Messages[0].Attributes, 'Message should have attributes');
  process.stdout.write(`   Received message with attributes: ${Object.keys(receiveResult.Messages[0].Attributes).join(', ')}\n`);

  // Delete message
  await sqs.send(new DeleteMessageCommand({
    QueueUrl: queueUrl,
    ReceiptHandle: receiveResult.Messages[0].ReceiptHandle,
  }));
  process.stdout.write(`   Deleted message\n`);

  // Delete queue
  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
  process.stdout.write(`   Deleted queue\n`);
}

// Test 2: Message Attributes
async function testMessageAttributes() {
  const queueName = `test-attrs-${Date.now()}`;

  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
  }));
  const queueUrl = createResult.QueueUrl;

  // Send message with attributes
  await sqs.send(new SendMessageCommand({
    QueueUrl: queueUrl,
    MessageBody: 'Test message',
    MessageAttributes: {
      'Author': {
        DataType: 'String',
        StringValue: 'JavaScript SDK',
      },
      'Priority': {
        DataType: 'Number',
        StringValue: '5',
      },
    },
  }));
  process.stdout.write(`   Sent message with 2 attributes\n`);

  // Receive and verify attributes
  const receiveResult = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 1,
    MessageAttributeNames: ['All'],
  }));

  assert(receiveResult.Messages && receiveResult.Messages.length === 1, 'Should receive 1 message');
  const attrs = receiveResult.Messages[0].MessageAttributes;
  assert(attrs, 'Message should have attributes');
  assert(attrs['Author']?.StringValue === 'JavaScript SDK', 'Author attribute should match');
  assert(attrs['Priority']?.StringValue === '5', 'Priority attribute should match');
  process.stdout.write(`   Verified attributes: Author=${attrs['Author'].StringValue}, Priority=${attrs['Priority'].StringValue}\n`);

  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
}

// Test 3: FIFO Queue
async function testFifoQueue() {
  const queueName = `test-fifo-${Date.now()}.fifo`;

  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
    Attributes: {
      'FifoQueue': 'true',
      'ContentBasedDeduplication': 'true',
    },
  }));
  const queueUrl = createResult.QueueUrl;
  process.stdout.write(`   Created FIFO queue: ${queueUrl}\n`);

  // Send messages in order
  const messages = ['First', 'Second', 'Third'];
  for (const body of messages) {
    await sqs.send(new SendMessageCommand({
      QueueUrl: queueUrl,
      MessageBody: body,
      MessageGroupId: 'test-group',
    }));
  }
  process.stdout.write(`   Sent 3 messages in order\n`);

  // Receive and verify order
  const receivedBodies = [];
  for (let i = 0; i < 3; i++) {
    const result = await sqs.send(new ReceiveMessageCommand({
      QueueUrl: queueUrl,
      MaxNumberOfMessages: 1,
    }));
    if (result.Messages && result.Messages.length > 0) {
      receivedBodies.push(result.Messages[0].Body);
      await sqs.send(new DeleteMessageCommand({
        QueueUrl: queueUrl,
        ReceiptHandle: result.Messages[0].ReceiptHandle,
      }));
    }
  }

  assert(JSON.stringify(receivedBodies) === JSON.stringify(messages),
         `Messages should be in order: expected ${JSON.stringify(messages)}, got ${JSON.stringify(receivedBodies)}`);
  process.stdout.write(`   Verified FIFO ordering: ${receivedBodies.join(' → ')}\n`);

  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
}

// Test 4: Batch Operations
async function testBatchOperations() {
  const queueName = `test-batch-${Date.now()}`;

  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
  }));
  const queueUrl = createResult.QueueUrl;

  // Send batch
  const batchResult = await sqs.send(new SendMessageBatchCommand({
    QueueUrl: queueUrl,
    Entries: [
      { Id: '1', MessageBody: 'Message 1' },
      { Id: '2', MessageBody: 'Message 2' },
      { Id: '3', MessageBody: 'Message 3' },
    ],
  }));
  assert(batchResult.Successful && batchResult.Successful.length === 3, 'All 3 messages should succeed');
  process.stdout.write(`   Sent batch of 3 messages\n`);

  // Receive messages
  const receiveResult = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 10,
  }));
  assert(receiveResult.Messages && receiveResult.Messages.length === 3, 'Should receive 3 messages');
  process.stdout.write(`   Received 3 messages\n`);

  // Delete batch
  const deleteResult = await sqs.send(new DeleteMessageBatchCommand({
    QueueUrl: queueUrl,
    Entries: receiveResult.Messages.map((msg, i) => ({
      Id: `${i + 1}`,
      ReceiptHandle: msg.ReceiptHandle,
    })),
  }));
  assert(deleteResult.Successful && deleteResult.Successful.length === 3, 'All 3 deletes should succeed');
  process.stdout.write(`   Deleted batch of 3 messages\n`);

  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
}

// Test 5: Queue Attributes
async function testQueueAttributes() {
  const queueName = `test-qattrs-${Date.now()}`;

  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
    Attributes: {
      'VisibilityTimeout': '60',
      'MessageRetentionPeriod': '86400',
    },
  }));
  const queueUrl = createResult.QueueUrl;

  // Get attributes
  const getResult = await sqs.send(new GetQueueAttributesCommand({
    QueueUrl: queueUrl,
    AttributeNames: ['All'],
  }));
  assert(getResult.Attributes, 'Should have attributes');
  assert(getResult.Attributes['VisibilityTimeout'] === '60', 'VisibilityTimeout should be 60');
  assert(getResult.Attributes['MessageRetentionPeriod'] === '86400', 'MessageRetentionPeriod should be 86400');
  process.stdout.write(`   Retrieved attributes: VisibilityTimeout=${getResult.Attributes['VisibilityTimeout']}\n`);

  // Set attributes
  await sqs.send(new SetQueueAttributesCommand({
    QueueUrl: queueUrl,
    Attributes: {
      'VisibilityTimeout': '120',
    },
  }));
  process.stdout.write(`   Updated VisibilityTimeout to 120\n`);

  // Verify update
  const verifyResult = await sqs.send(new GetQueueAttributesCommand({
    QueueUrl: queueUrl,
    AttributeNames: ['VisibilityTimeout'],
  }));
  assert(verifyResult.Attributes['VisibilityTimeout'] === '120', 'VisibilityTimeout should be updated to 120');
  process.stdout.write(`   Verified update: VisibilityTimeout=${verifyResult.Attributes['VisibilityTimeout']}\n`);

  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
}

// Test 6: Change Message Visibility
async function testChangeMessageVisibility() {
  const queueName = `test-visibility-${Date.now()}`;

  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
  }));
  const queueUrl = createResult.QueueUrl;

  // Send message
  await sqs.send(new SendMessageCommand({
    QueueUrl: queueUrl,
    MessageBody: 'Test visibility',
  }));

  // Receive message
  const receiveResult = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 1,
  }));
  assert(receiveResult.Messages && receiveResult.Messages.length === 1, 'Should receive 1 message');
  const receiptHandle = receiveResult.Messages[0].ReceiptHandle;
  process.stdout.write(`   Received message\n`);

  // Change visibility to 0 (return to queue immediately)
  await sqs.send(new ChangeMessageVisibilityCommand({
    QueueUrl: queueUrl,
    ReceiptHandle: receiptHandle,
    VisibilityTimeout: 0,
  }));
  process.stdout.write(`   Changed visibility timeout to 0\n`);

  // Should be able to receive again immediately
  const receiveAgain = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 1,
  }));
  assert(receiveAgain.Messages && receiveAgain.Messages.length === 1, 'Should receive message again');
  process.stdout.write(`   Received message again immediately\n`);

  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
}

// Test 7: Delay Queue
async function testDelayQueue() {
  const queueName = `test-delay-${Date.now()}`;

  const createResult = await sqs.send(new CreateQueueCommand({
    QueueName: queueName,
    Attributes: {
      'DelaySeconds': '2',
    },
  }));
  const queueUrl = createResult.QueueUrl;
  process.stdout.write(`   Created delay queue (2 second delay)\n`);

  // Send message
  const sendTime = Date.now();
  await sqs.send(new SendMessageCommand({
    QueueUrl: queueUrl,
    MessageBody: 'Delayed message',
  }));
  process.stdout.write(`   Sent message at ${new Date(sendTime).toTimeString().split(' ')[0]}\n`);

  // Try immediate receive (should be empty)
  const immediate = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 1,
  }));
  assert(!immediate.Messages || immediate.Messages.length === 0, 'Should not receive message immediately');
  process.stdout.write(`   No message received immediately (correct)\n`);

  // Wait for delay
  process.stdout.write(`   Waiting 3 seconds...\n`);
  await sleep(3000);

  // Receive should work now
  const delayed = await sqs.send(new ReceiveMessageCommand({
    QueueUrl: queueUrl,
    MaxNumberOfMessages: 1,
  }));
  const receiveTime = Date.now();
  assert(delayed.Messages && delayed.Messages.length === 1, 'Should receive message after delay');
  const elapsed = (receiveTime - sendTime) / 1000;
  process.stdout.write(`   Received message after ${elapsed.toFixed(2)} seconds\n`);

  await sqs.send(new DeleteQueueCommand({ QueueUrl: queueUrl }));
}

// Main test runner
async function main() {
  console.log('============================================================');
  console.log('🚀 lclq Advanced Integration Tests with AWS SDK v3');
  console.log('============================================================');
  console.log(`Endpoint: ${SQS_ENDPOINT}`);
  console.log(`Region: ${REGION}`);

  await runTest('Basic Queue Operations', testBasicQueueOperations);
  await runTest('Message Attributes', testMessageAttributes);
  await runTest('FIFO Queue', testFifoQueue);
  await runTest('Batch Operations', testBatchOperations);
  await runTest('Queue Attributes', testQueueAttributes);
  await runTest('Change Message Visibility', testChangeMessageVisibility);
  await runTest('Delay Queue', testDelayQueue);

  console.log('\n============================================================');
  console.log(`📊 Test Results: ${testsPassed} passed, ${testsFailed} failed`);

  if (testsFailed > 0) {
    console.log('❌ FAILED TESTS:');
    failures.forEach(({ name, error }) => {
      console.log(`   - ${name}: ${error}`);
    });
    console.log('============================================================');
    process.exit(1);
  } else {
    console.log('✅ ALL TESTS PASSED!');
    console.log('============================================================');
    process.exit(0);
  }
}

// Run tests
main().catch(error => {
  console.error('\n💥 Fatal error:', error);
  process.exit(1);
});
