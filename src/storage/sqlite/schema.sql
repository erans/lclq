-- SQLite schema for lclq
-- This schema supports both AWS SQS and GCP Pub/Sub queue models

-- Queues table: stores queue metadata and configuration
CREATE TABLE IF NOT EXISTS queues (
    -- Primary key: queue identifier
    id TEXT PRIMARY KEY NOT NULL,

    -- Queue name (same as id for most cases)
    name TEXT NOT NULL,

    -- Queue type: 'sqs_standard', 'sqs_fifo', 'pubsub_topic'
    queue_type TEXT NOT NULL,

    -- Configuration
    visibility_timeout INTEGER NOT NULL DEFAULT 30,
    message_retention_period INTEGER NOT NULL DEFAULT 345600,  -- 4 days in seconds
    max_message_size INTEGER NOT NULL DEFAULT 262144,  -- 256 KB
    delay_seconds INTEGER NOT NULL DEFAULT 0,

    -- FIFO-specific
    content_based_deduplication BOOLEAN NOT NULL DEFAULT FALSE,

    -- Dead Letter Queue configuration (JSON)
    dlq_config TEXT,  -- JSON: {"target_queue_id": "...", "max_receive_count": 5}

    -- Tags (JSON)
    tags TEXT,  -- JSON object of key-value pairs

    -- Timestamps
    created_at INTEGER NOT NULL,  -- Unix timestamp
    updated_at INTEGER NOT NULL,  -- Unix timestamp

    -- Indexes for common queries
    UNIQUE(name)
);

-- Messages table: stores all messages in all queues
CREATE TABLE IF NOT EXISTS messages (
    -- Primary key: message identifier
    id TEXT PRIMARY KEY NOT NULL,

    -- Foreign key to queue
    queue_id TEXT NOT NULL,

    -- Message content
    body TEXT NOT NULL,

    -- Message attributes (JSON)
    attributes TEXT,  -- JSON object of message attributes

    -- Timing
    sent_timestamp INTEGER NOT NULL,  -- Unix timestamp in milliseconds
    visible_at INTEGER NOT NULL,  -- Unix timestamp when message becomes visible

    -- Receive tracking
    receive_count INTEGER NOT NULL DEFAULT 0,

    -- FIFO-specific fields
    message_group_id TEXT,
    deduplication_id TEXT,
    sequence_number INTEGER,

    -- Delay
    delay_seconds INTEGER,

    -- State: 'available', 'in_flight', 'delayed'
    state TEXT NOT NULL DEFAULT 'available',

    -- In-flight tracking
    receipt_handle TEXT,  -- Current receipt handle if in-flight
    visibility_expires_at INTEGER,  -- Unix timestamp when visibility expires

    -- Foreign key constraint
    FOREIGN KEY (queue_id) REFERENCES queues(id) ON DELETE CASCADE
);

-- Indexes for message queries
CREATE INDEX IF NOT EXISTS idx_messages_queue_id ON messages(queue_id);
CREATE INDEX IF NOT EXISTS idx_messages_state ON messages(state);
CREATE INDEX IF NOT EXISTS idx_messages_visible_at ON messages(visible_at);
CREATE INDEX IF NOT EXISTS idx_messages_visibility_expires ON messages(visibility_expires_at);
CREATE INDEX IF NOT EXISTS idx_messages_message_group ON messages(queue_id, message_group_id, sequence_number);
CREATE INDEX IF NOT EXISTS idx_messages_dedup ON messages(queue_id, deduplication_id);

-- Deduplication cache: tracks recent message deduplication IDs for FIFO queues
CREATE TABLE IF NOT EXISTS deduplication_cache (
    -- Queue + deduplication ID combination
    queue_id TEXT NOT NULL,
    deduplication_id TEXT NOT NULL,

    -- When this entry expires (5 minutes from creation)
    expires_at INTEGER NOT NULL,  -- Unix timestamp

    -- Composite primary key
    PRIMARY KEY (queue_id, deduplication_id),

    -- Foreign key
    FOREIGN KEY (queue_id) REFERENCES queues(id) ON DELETE CASCADE
);

-- Index for expiration cleanup
CREATE INDEX IF NOT EXISTS idx_dedup_expires ON deduplication_cache(expires_at);

-- Subscriptions table: for GCP Pub/Sub subscriptions (future use)
CREATE TABLE IF NOT EXISTS subscriptions (
    -- Primary key: subscription identifier
    id TEXT PRIMARY KEY NOT NULL,

    -- Subscription name
    name TEXT NOT NULL,

    -- Topic (queue) this subscription is attached to
    topic_id TEXT NOT NULL,

    -- Configuration
    ack_deadline_seconds INTEGER NOT NULL DEFAULT 10,

    -- Push configuration (JSON) - optional
    push_config TEXT,  -- JSON: {"push_endpoint": "...", "attributes": {...}}

    -- Message ordering
    enable_message_ordering BOOLEAN NOT NULL DEFAULT FALSE,

    -- Filter (CEL expression)
    filter TEXT,

    -- Dead letter topic
    dead_letter_topic_id TEXT,
    max_delivery_attempts INTEGER,

    -- Retention
    retain_acked_messages BOOLEAN NOT NULL DEFAULT FALSE,
    message_retention_duration INTEGER,  -- Seconds

    -- Timestamps
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    -- Foreign keys
    FOREIGN KEY (topic_id) REFERENCES queues(id) ON DELETE CASCADE,

    -- Unique constraint
    UNIQUE(name)
);

-- Subscription messages: tracks which messages have been delivered to which subscriptions
CREATE TABLE IF NOT EXISTS subscription_messages (
    -- Subscription
    subscription_id TEXT NOT NULL,

    -- Message
    message_id TEXT NOT NULL,

    -- State: 'pending', 'acked', 'nacked'
    state TEXT NOT NULL DEFAULT 'pending',

    -- Delivery tracking
    delivery_attempts INTEGER NOT NULL DEFAULT 0,
    ack_deadline_at INTEGER,  -- Unix timestamp

    -- Composite primary key
    PRIMARY KEY (subscription_id, message_id),

    -- Foreign keys
    FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- Indexes for subscription message queries
CREATE INDEX IF NOT EXISTS idx_sub_msgs_state ON subscription_messages(subscription_id, state);
CREATE INDEX IF NOT EXISTS idx_sub_msgs_ack_deadline ON subscription_messages(ack_deadline_at);
