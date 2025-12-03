//! SQLite-based storage backend for lclq.
//!
//! This module provides a persistent storage backend using SQLite,
//! implementing the `StorageBackend` trait for queue and message operations.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tracing::{debug, info};

use crate::core::receipt::{generate_receipt_handle, parse_receipt_handle};
use crate::error::{Error, Result};
use crate::storage::{ReceivedMessage, StorageBackend};
use crate::types::{Message, MessageId, QueueConfig, QueueStats, QueueType, ReceiveOptions};

/// SQLite storage backend configuration.
#[derive(Debug, Clone)]
pub struct SqliteConfig {
    /// Database file path (":memory:" for in-memory database).
    pub database_path: String,
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            database_path: "lclq.db".to_string(),
            max_connections: 10,
        }
    }
}

/// SQLite-based storage backend.
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    /// Create a new SQLite backend.
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        info!(
            database_path = %config.database_path,
            max_connections = config.max_connections,
            "Initializing SQLite backend"
        );

        // Configure SQLite connection options
        let options = SqliteConnectOptions::new()
            .filename(&config.database_path)
            .create_if_missing(true)
            // Enable WAL mode for better concurrency
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            // Set busy timeout to 5 seconds
            .busy_timeout(std::time::Duration::from_secs(5));

        // Create connection pool
        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .connect_with(options)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to connect to SQLite: {}", e)))?;

        // Run migrations
        sqlx::migrate!("./migrations/sqlite")
            .run(&pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to run migrations: {}", e)))?;

        info!("SQLite backend initialized successfully");

        Ok(Self { pool })
    }

    /// Process expired visibility timeouts and return messages to available state.
    /// Returns the number of messages that were returned to available state.
    pub async fn process_expired_visibility(&self, queue_id: &str) -> Result<u64> {
        let now = Utc::now().timestamp_millis();

        // Return messages with expired visibility to available state for the specified queue
        let result = sqlx::query(
            r#"
            UPDATE messages
            SET state = 'available',
                visible_at = ?,
                receipt_handle = NULL,
                visibility_expires_at = NULL
            WHERE state = 'in_flight'
              AND queue_id = ?
              AND visibility_expires_at IS NOT NULL
              AND visibility_expires_at <= ?
            "#,
        )
        .bind(now)
        .bind(queue_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to process expired visibility: {}", e)))?;

        let count = result.rows_affected();
        if count > 0 {
            debug!(
                queue_id = %queue_id,
                messages_returned = count,
                "Returned expired messages to queue"
            );
        }

        Ok(count)
    }

    /// Clean up expired deduplication cache entries.
    pub async fn cleanup_deduplication_cache(&self) -> Result<u64> {
        let now = Utc::now().timestamp();

        let result = sqlx::query("DELETE FROM deduplication_cache WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to cleanup deduplication cache: {}", e))
            })?;

        let count = result.rows_affected();
        if count > 0 {
            debug!(entries_deleted = count, "Cleaned up deduplication cache");
        }

        Ok(count)
    }

    /// Delete messages that have exceeded their retention period.
    pub async fn delete_expired_messages(&self) -> Result<u64> {
        let now = Utc::now().timestamp_millis();

        // Get all queues to check their retention periods
        let queues = sqlx::query("SELECT id, message_retention_period FROM queues")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to get queues for retention cleanup: {}", e))
            })?;

        let mut total_deleted = 0u64;

        for row in queues {
            let queue_id: String = row.get("id");
            let retention_period_secs: i64 = row.get("message_retention_period");
            let retention_millis = retention_period_secs * 1000;

            // Delete messages older than retention period
            let cutoff_timestamp = now - retention_millis;

            let result =
                sqlx::query("DELETE FROM messages WHERE queue_id = ? AND sent_timestamp < ?")
                    .bind(&queue_id)
                    .bind(cutoff_timestamp)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| {
                        Error::StorageError(format!("Failed to delete expired messages: {}", e))
                    })?;

            let count = result.rows_affected();
            if count > 0 {
                info!(
                    queue_id = %queue_id,
                    messages_deleted = count,
                    "Deleted expired messages past retention period"
                );
                total_deleted += count;
            }
        }

        if total_deleted > 0 {
            info!(
                total_deleted = total_deleted,
                "Deleted expired messages across all queues"
            );
        }

        Ok(total_deleted)
    }

    /// Helper: Convert QueueType to string for database storage.
    fn queue_type_to_string(queue_type: &QueueType) -> &'static str {
        match queue_type {
            QueueType::SqsStandard => "sqs_standard",
            QueueType::SqsFifo => "sqs_fifo",
            QueueType::PubSubTopic => "pubsub_topic",
        }
    }

    /// Helper: Convert string from database to QueueType.
    fn string_to_queue_type(s: &str) -> Result<QueueType> {
        match s {
            "sqs_standard" => Ok(QueueType::SqsStandard),
            "sqs_fifo" => Ok(QueueType::SqsFifo),
            "pubsub_topic" => Ok(QueueType::PubSubTopic),
            _ => Err(Error::StorageError(format!("Unknown queue type: {}", s))),
        }
    }

    /// Helper: Parse a message from a database row.
    fn parse_message_row(&self, row: &sqlx::sqlite::SqliteRow) -> Result<Message> {
        use crate::types::MessageId;

        let attributes_json: Option<String> = row.get("attributes");
        let attributes = if let Some(json) = attributes_json {
            serde_json::from_str(&json).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        let sent_timestamp_millis: i64 = row.get("sent_timestamp");
        let sent_timestamp =
            DateTime::from_timestamp_millis(sent_timestamp_millis).ok_or_else(|| {
                Error::StorageError(format!("Invalid timestamp: {}", sent_timestamp_millis))
            })?;

        Ok(Message {
            id: MessageId(row.get("id")),
            body: row.get("body"),
            attributes,
            queue_id: row.get("queue_id"),
            sent_timestamp,
            receive_count: row.get::<i64, _>("receive_count") as u32,
            message_group_id: row.get("message_group_id"),
            deduplication_id: row.get("deduplication_id"),
            sequence_number: row
                .get::<Option<i64>, _>("sequence_number")
                .map(|n| n as u64),
            delay_seconds: row.get::<Option<i64>, _>("delay_seconds").map(|d| d as u32),
        })
    }
}

impl SqliteBackend {
    /// Helper method to send a message within a transaction or pool.
    async fn send_message_impl<'e, E>(
        &self,
        executor: E,
        queue_id: &str,
        mut message: Message,
    ) -> Result<Message>
    where
        E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
    {
        // Get queue config to check FIFO settings
        let queue = self.get_queue(queue_id).await?;

        // Handle FIFO deduplication
        if queue.queue_type == QueueType::SqsFifo {
            let dedup_id = if let Some(ref id) = message.deduplication_id {
                id.clone()
            } else if queue.content_based_deduplication {
                // Generate content-based dedup ID (SHA256 of body)
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(message.body.as_bytes());
                format!("{:x}", hasher.finalize())
            } else {
                return Err(Error::Validation(
                    crate::error::ValidationError::InvalidParameter {
                        name: "MessageDeduplicationId".to_string(),
                        reason: "Required for FIFO queues without content-based deduplication"
                            .to_string(),
                    },
                ));
            };

            // Check deduplication cache (5 minute window)
            let cutoff = Utc::now().timestamp() - 300; // 5 minutes ago
            let cached = sqlx::query(
                "SELECT expires_at FROM deduplication_cache WHERE queue_id = ? AND deduplication_id = ? AND expires_at > ?"
            )
            .bind(queue_id)
            .bind(&dedup_id)
            .bind(cutoff)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to check deduplication: {}", e)))?;

            if cached.is_some() {
                debug!(
                    queue_id = %queue_id,
                    dedup_id = %dedup_id,
                    "Message deduplicated (already sent within 5 minutes)"
                );
                return Ok(message); // Return the message but don't add it
            }

            // Add to deduplication cache
            let expires_at = Utc::now().timestamp() + 300; // Expires in 5 minutes
            sqlx::query(
                "INSERT OR REPLACE INTO deduplication_cache (queue_id, deduplication_id, expires_at) VALUES (?, ?, ?)"
            )
            .bind(queue_id)
            .bind(&dedup_id)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update deduplication cache: {}", e)))?;

            // Get next sequence number
            let sequence_number: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM messages WHERE queue_id = ?",
            )
            .bind(queue_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get sequence number: {}", e)))?;

            message.sequence_number = Some(sequence_number as u64);
        }

        // Calculate when message becomes visible
        let delay = message.delay_seconds.unwrap_or(queue.delay_seconds);
        let visible_at = (Utc::now() + Duration::seconds(delay as i64)).timestamp_millis();

        // Serialize message attributes to JSON
        let attributes_json = if message.attributes.is_empty() {
            None
        } else {
            serde_json::to_string(&message.attributes).ok()
        };

        // Insert message
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, queue_id, body, attributes, sent_timestamp, visible_at,
                receive_count, message_group_id, deduplication_id, sequence_number,
                delay_seconds, state
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'available')
            "#,
        )
        .bind(&message.id.0)
        .bind(queue_id)
        .bind(&message.body)
        .bind(attributes_json)
        .bind(message.sent_timestamp.timestamp_millis())
        .bind(visible_at)
        .bind(message.receive_count as i64)
        .bind(&message.message_group_id)
        .bind(&message.deduplication_id)
        .bind(message.sequence_number.map(|n| n as i64))
        .bind(delay as i64)
        .execute(executor)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to insert message: {}", e)))?;

        info!(queue_id = %queue_id, message_id = %message.id, "Message sent to SQLite");
        Ok(message)
    }
}

#[async_trait]
impl StorageBackend for SqliteBackend {
    async fn create_queue(&self, config: QueueConfig) -> Result<QueueConfig> {
        debug!(queue_id = %config.id, "Creating queue in SQLite");

        let now = Utc::now().timestamp();
        let queue_type_str = Self::queue_type_to_string(&config.queue_type);

        // Serialize DLQ config, tags, and redrive allow policy to JSON
        let dlq_config_json = config
            .dlq_config
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok());

        let tags_json = if config.tags.is_empty() {
            None
        } else {
            serde_json::to_string(&config.tags).ok()
        };

        let redrive_allow_policy_json = config
            .redrive_allow_policy
            .as_ref()
            .and_then(|p| serde_json::to_string(p).ok());

        sqlx::query(
            r#"
            INSERT INTO queues (
                id, name, queue_type, visibility_timeout, message_retention_period,
                max_message_size, delay_seconds, content_based_deduplication,
                dlq_config, tags, redrive_allow_policy, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(queue_type_str)
        .bind(config.visibility_timeout as i64)
        .bind(config.message_retention_period as i64)
        .bind(config.max_message_size as i64)
        .bind(config.delay_seconds as i64)
        .bind(config.content_based_deduplication)
        .bind(dlq_config_json)
        .bind(tags_json)
        .bind(redrive_allow_policy_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                Error::QueueAlreadyExists(config.id.clone())
            } else {
                Error::StorageError(format!("Failed to create queue: {}", e))
            }
        })?;

        info!(queue_id = %config.id, "Queue created in SQLite");
        Ok(config)
    }

    async fn get_queue(&self, queue_id: &str) -> Result<QueueConfig> {
        debug!(queue_id = %queue_id, "Getting queue from SQLite");

        let row = sqlx::query(
            r#"
            SELECT id, name, queue_type, visibility_timeout, message_retention_period,
                   max_message_size, delay_seconds, content_based_deduplication,
                   dlq_config, tags, redrive_allow_policy
            FROM queues
            WHERE id = ?
            "#,
        )
        .bind(queue_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to get queue: {}", e)))?
        .ok_or_else(|| Error::QueueNotFound(queue_id.to_string()))?;

        // Parse row into QueueConfig
        let queue_type = Self::string_to_queue_type(row.get("queue_type"))?;

        let dlq_config: Option<crate::types::DlqConfig> = row
            .get::<Option<String>, _>("dlq_config")
            .and_then(|json| serde_json::from_str(&json).ok());

        let tags: std::collections::HashMap<String, String> = row
            .get::<Option<String>, _>("tags")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();

        let redrive_allow_policy: Option<crate::types::RedriveAllowPolicy> = row
            .get::<Option<String>, _>("redrive_allow_policy")
            .and_then(|json| serde_json::from_str(&json).ok());

        Ok(QueueConfig {
            id: row.get("id"),
            name: row.get("name"),
            queue_type,
            visibility_timeout: row.get::<i64, _>("visibility_timeout") as u32,
            message_retention_period: row.get::<i64, _>("message_retention_period") as u32,
            max_message_size: row.get::<i64, _>("max_message_size") as usize,
            delay_seconds: row.get::<i64, _>("delay_seconds") as u32,
            dlq_config,
            content_based_deduplication: row.get("content_based_deduplication"),
            tags,
            redrive_allow_policy,
        })
    }

    async fn delete_queue(&self, queue_id: &str) -> Result<()> {
        debug!(queue_id = %queue_id, "Deleting queue from SQLite");

        let result = sqlx::query("DELETE FROM queues WHERE id = ?")
            .bind(queue_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to delete queue: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::QueueNotFound(queue_id.to_string()));
        }

        info!(queue_id = %queue_id, "Queue deleted from SQLite");
        Ok(())
    }

    async fn list_queues(
        &self,
        filter: Option<crate::storage::QueueFilter>,
    ) -> Result<Vec<QueueConfig>> {
        debug!("Listing queues from SQLite");

        let rows = if let Some(ref f) = filter {
            if let Some(ref prefix) = f.name_prefix {
                // Use parameterized query to prevent SQL injection
                sqlx::query(
                    r#"
                    SELECT id, name, queue_type, visibility_timeout, message_retention_period,
                           max_message_size, delay_seconds, content_based_deduplication,
                           dlq_config, tags, redrive_allow_policy
                    FROM queues
                    WHERE name LIKE ? || '%'
                    ORDER BY name
                    "#,
                )
                .bind(prefix)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::StorageError(format!("Failed to list queues: {}", e)))?
            } else {
                sqlx::query(
                    r#"
                    SELECT id, name, queue_type, visibility_timeout, message_retention_period,
                           max_message_size, delay_seconds, content_based_deduplication,
                           dlq_config, tags, redrive_allow_policy
                    FROM queues
                    ORDER BY name
                    "#,
                )
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::StorageError(format!("Failed to list queues: {}", e)))?
            }
        } else {
            sqlx::query(
                r#"
                SELECT id, name, queue_type, visibility_timeout, message_retention_period,
                       max_message_size, delay_seconds, content_based_deduplication,
                       dlq_config, tags, redrive_allow_policy
                FROM queues
                ORDER BY name
                "#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to list queues: {}", e)))?
        };

        let mut queues = Vec::new();
        for row in rows {
            let queue_type = Self::string_to_queue_type(row.get("queue_type"))?;

            let dlq_config: Option<crate::types::DlqConfig> = row
                .get::<Option<String>, _>("dlq_config")
                .and_then(|json| serde_json::from_str(&json).ok());

            let tags: std::collections::HashMap<String, String> = row
                .get::<Option<String>, _>("tags")
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            let redrive_allow_policy: Option<crate::types::RedriveAllowPolicy> = row
                .get::<Option<String>, _>("redrive_allow_policy")
                .and_then(|json| serde_json::from_str(&json).ok());

            queues.push(QueueConfig {
                id: row.get("id"),
                name: row.get("name"),
                queue_type,
                visibility_timeout: row.get::<i64, _>("visibility_timeout") as u32,
                message_retention_period: row.get::<i64, _>("message_retention_period") as u32,
                max_message_size: row.get::<i64, _>("max_message_size") as usize,
                delay_seconds: row.get::<i64, _>("delay_seconds") as u32,
                dlq_config,
                content_based_deduplication: row.get("content_based_deduplication"),
                tags,
                redrive_allow_policy,
            });
        }

        debug!(count = queues.len(), "Listed queues from SQLite");
        Ok(queues)
    }

    async fn update_queue(&self, config: QueueConfig) -> Result<QueueConfig> {
        debug!(queue_id = %config.id, "Updating queue in SQLite");

        let now = Utc::now().timestamp();

        // Serialize DLQ config, tags, and redrive allow policy to JSON
        let dlq_config_json = config
            .dlq_config
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok());

        let tags_json = if config.tags.is_empty() {
            None
        } else {
            serde_json::to_string(&config.tags).ok()
        };

        let redrive_allow_policy_json = config
            .redrive_allow_policy
            .as_ref()
            .and_then(|p| serde_json::to_string(p).ok());

        let result = sqlx::query(
            r#"
            UPDATE queues
            SET visibility_timeout = ?,
                message_retention_period = ?,
                max_message_size = ?,
                delay_seconds = ?,
                content_based_deduplication = ?,
                dlq_config = ?,
                tags = ?,
                redrive_allow_policy = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(config.visibility_timeout as i64)
        .bind(config.message_retention_period as i64)
        .bind(config.max_message_size as i64)
        .bind(config.delay_seconds as i64)
        .bind(config.content_based_deduplication)
        .bind(dlq_config_json)
        .bind(tags_json)
        .bind(redrive_allow_policy_json)
        .bind(now)
        .bind(&config.id)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to update queue: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::QueueNotFound(config.id.clone()));
        }

        info!(queue_id = %config.id, "Queue updated in SQLite");
        Ok(config)
    }

    // TODO: Implement remaining methods (send_message, receive_messages, etc.)

    async fn send_message(&self, queue_id: &str, message: Message) -> Result<Message> {
        debug!(queue_id = %queue_id, message_id = %message.id, "Sending message to SQLite");
        self.send_message_impl(&self.pool, queue_id, message).await
    }

    async fn send_messages(&self, queue_id: &str, messages: Vec<Message>) -> Result<Vec<Message>> {
        debug!(queue_id = %queue_id, message_count = messages.len(), "Sending batch of messages to SQLite");

        // Start a transaction for atomicity
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to begin transaction: {}", e)))?;

        let mut results = Vec::new();
        for message in messages {
            // Send each message within the transaction
            let result = self.send_message_impl(&mut *tx, queue_id, message).await?;
            results.push(result);
        }

        // Commit transaction - all messages succeed or all fail
        tx.commit()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to commit transaction: {}", e)))?;

        info!(queue_id = %queue_id, message_count = results.len(), "Batch messages sent to SQLite");
        Ok(results)
    }

    async fn receive_messages(
        &self,
        queue_id: &str,
        options: ReceiveOptions,
    ) -> Result<Vec<ReceivedMessage>> {
        debug!(queue_id = %queue_id, "Receiving messages from SQLite");

        // Get queue config
        let queue = self.get_queue(queue_id).await?;

        let now = Utc::now();
        let now_millis = now.timestamp_millis();
        let visibility_timeout = options
            .visibility_timeout
            .unwrap_or(queue.visibility_timeout);
        let max_messages = options.max_messages.min(10) as i64;

        let mut received = Vec::new();

        // Start a transaction
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to begin transaction: {}", e)))?;

        // For FIFO queues, respect message group ordering
        if queue.queue_type == QueueType::SqsFifo {
            // Get message groups that are currently in flight
            let in_flight_groups: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT message_group_id FROM messages WHERE queue_id = ? AND state = 'in_flight' AND message_group_id IS NOT NULL"
            )
            .bind(queue_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to query in-flight groups: {}", e)))?;

            // Build query to exclude in-flight message groups
            let query_str = if in_flight_groups.is_empty() {
                format!(
                    "SELECT * FROM messages WHERE queue_id = ? AND state = 'available' AND visible_at <= ? ORDER BY sequence_number LIMIT {}",
                    max_messages
                )
            } else {
                let placeholders: Vec<_> = (0..in_flight_groups.len())
                    .map(|i| format!("?{}", i + 3))
                    .collect();
                format!(
                    "SELECT * FROM messages WHERE queue_id = ? AND state = 'available' AND visible_at <= ? AND (message_group_id IS NULL OR message_group_id NOT IN ({})) ORDER BY sequence_number LIMIT {}",
                    placeholders.join(", "),
                    max_messages
                )
            };

            let mut query = sqlx::query(&query_str).bind(queue_id).bind(now_millis);

            for group in &in_flight_groups {
                query = query.bind(group);
            }

            let rows = query
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| Error::StorageError(format!("Failed to fetch messages: {}", e)))?;

            for row in rows {
                let message_id: String = row.get("id");
                let mut message = self.parse_message_row(&row)?;
                message.receive_count += 1;

                // Check if message should move to DLQ
                let should_dlq = if let Some(dlq_config) = &queue.dlq_config {
                    message.receive_count > dlq_config.max_receive_count
                } else {
                    false
                };

                if should_dlq {
                    // Move message to DLQ
                    if let Some(dlq_config) = &queue.dlq_config {
                        let dlq_queue_id = &dlq_config.target_queue_id;

                        // Create fresh message for DLQ
                        let mut dlq_message = message.clone();
                        dlq_message.sent_timestamp = Utc::now();
                        dlq_message.receive_count = 0;
                        dlq_message.queue_id = dlq_queue_id.clone();

                        // Insert into DLQ
                        self.send_message_impl(&mut *tx, dlq_queue_id, dlq_message)
                            .await?;

                        // Delete from source queue
                        sqlx::query("DELETE FROM messages WHERE id = ?")
                            .bind(&message_id)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| {
                                Error::StorageError(format!("Failed to delete message: {}", e))
                            })?;

                        info!(
                            source_queue_id = %queue_id,
                            dlq_queue_id = %dlq_queue_id,
                            message_id = %message.id,
                            original_receive_count = message.receive_count,
                            "Message moved to DLQ"
                        );
                    }
                    continue; // Don't add to received list
                }

                // Generate receipt handle
                let receipt_handle =
                    generate_receipt_handle(queue_id, &MessageId(message_id.clone()));

                // Update message to in_flight state
                let visibility_expires_at =
                    (now + Duration::seconds(visibility_timeout as i64)).timestamp_millis();
                sqlx::query(
                    "UPDATE messages SET state = 'in_flight', receive_count = ?, receipt_handle = ?, visibility_expires_at = ? WHERE id = ?"
                )
                .bind(message.receive_count as i64)
                .bind(&receipt_handle)
                .bind(visibility_expires_at)
                .bind(&message_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| Error::StorageError(format!("Failed to update message state: {}", e)))?;

                received.push(ReceivedMessage {
                    message,
                    receipt_handle,
                });
            }
        } else {
            // Standard queue - simpler logic
            let rows = sqlx::query(
                "SELECT * FROM messages WHERE queue_id = ? AND state = 'available' AND visible_at <= ? ORDER BY sent_timestamp LIMIT ?"
            )
            .bind(queue_id)
            .bind(now_millis)
            .bind(max_messages)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to fetch messages: {}", e)))?;

            for row in rows {
                let message_id: String = row.get("id");
                let mut message = self.parse_message_row(&row)?;
                message.receive_count += 1;

                // Check if message should move to DLQ
                let should_dlq = if let Some(dlq_config) = &queue.dlq_config {
                    message.receive_count > dlq_config.max_receive_count
                } else {
                    false
                };

                if should_dlq {
                    // Move message to DLQ
                    if let Some(dlq_config) = &queue.dlq_config {
                        let dlq_queue_id = &dlq_config.target_queue_id;

                        // Create fresh message for DLQ
                        let mut dlq_message = message.clone();
                        dlq_message.sent_timestamp = Utc::now();
                        dlq_message.receive_count = 0;
                        dlq_message.queue_id = dlq_queue_id.clone();

                        // Insert into DLQ
                        self.send_message_impl(&mut *tx, dlq_queue_id, dlq_message)
                            .await?;

                        // Delete from source queue
                        sqlx::query("DELETE FROM messages WHERE id = ?")
                            .bind(&message_id)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| {
                                Error::StorageError(format!("Failed to delete message: {}", e))
                            })?;

                        info!(
                            source_queue_id = %queue_id,
                            dlq_queue_id = %dlq_queue_id,
                            message_id = %message.id,
                            original_receive_count = message.receive_count,
                            "Message moved to DLQ"
                        );
                    }
                    continue; // Don't add to received list
                }

                let receipt_handle =
                    generate_receipt_handle(queue_id, &MessageId(message_id.clone()));
                let visibility_expires_at =
                    (now + Duration::seconds(visibility_timeout as i64)).timestamp_millis();

                sqlx::query(
                    "UPDATE messages SET state = 'in_flight', receive_count = ?, receipt_handle = ?, visibility_expires_at = ? WHERE id = ?"
                )
                .bind(message.receive_count as i64)
                .bind(&receipt_handle)
                .bind(visibility_expires_at)
                .bind(&message_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| Error::StorageError(format!("Failed to update message state: {}", e)))?;

                received.push(ReceivedMessage {
                    message,
                    receipt_handle,
                });
            }
        }

        tx.commit()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to commit transaction: {}", e)))?;

        debug!(
            queue_id = %queue_id,
            received_count = received.len(),
            "Messages received from SQLite"
        );

        Ok(received)
    }

    async fn delete_message(&self, queue_id: &str, receipt_handle: &str) -> Result<()> {
        debug!(queue_id = %queue_id, "Deleting message from SQLite");

        let handle_data = parse_receipt_handle(receipt_handle)?;

        if handle_data.queue_id != queue_id {
            return Err(Error::InvalidReceiptHandle);
        }

        let result = sqlx::query(
            "DELETE FROM messages WHERE queue_id = ? AND id = ? AND receipt_handle = ?",
        )
        .bind(queue_id)
        .bind(&handle_data.message_id.0)
        .bind(receipt_handle)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to delete message: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::MessageNotFound(handle_data.message_id.0.clone()));
        }

        info!(queue_id = %queue_id, message_id = %handle_data.message_id, "Message deleted from SQLite");
        Ok(())
    }

    async fn change_visibility(
        &self,
        queue_id: &str,
        receipt_handle: &str,
        visibility_timeout: u32,
    ) -> Result<()> {
        debug!(queue_id = %queue_id, visibility_timeout = visibility_timeout, "Changing message visibility");

        let handle_data = parse_receipt_handle(receipt_handle)?;

        if handle_data.queue_id != queue_id {
            return Err(Error::InvalidReceiptHandle);
        }

        // If visibility timeout is 0, return message to available state
        if visibility_timeout == 0 {
            let result = sqlx::query(
                "UPDATE messages SET state = 'available', visible_at = ?, receipt_handle = NULL, visibility_expires_at = NULL WHERE queue_id = ? AND id = ? AND receipt_handle = ?"
            )
            .bind(Utc::now().timestamp_millis())
            .bind(queue_id)
            .bind(&handle_data.message_id.0)
            .bind(receipt_handle)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to change visibility: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(Error::MessageNotFound(handle_data.message_id.0.clone()));
            }

            debug!(
                queue_id = %queue_id,
                message_id = %handle_data.message_id,
                "Message returned to queue (visibility timeout set to 0)"
            );
        } else {
            // Update visibility timeout
            let visibility_expires_at =
                (Utc::now() + Duration::seconds(visibility_timeout as i64)).timestamp_millis();

            let result = sqlx::query(
                "UPDATE messages SET visibility_expires_at = ? WHERE queue_id = ? AND id = ? AND receipt_handle = ?"
            )
            .bind(visibility_expires_at)
            .bind(queue_id)
            .bind(&handle_data.message_id.0)
            .bind(receipt_handle)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to change visibility: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(Error::MessageNotFound(handle_data.message_id.0.clone()));
            }

            debug!(
                queue_id = %queue_id,
                message_id = %handle_data.message_id,
                visibility_timeout = visibility_timeout,
                "Visibility timeout changed"
            );
        }

        Ok(())
    }

    async fn purge_queue(&self, queue_id: &str) -> Result<()> {
        debug!(queue_id = %queue_id, "Purging queue");

        // Verify queue exists
        self.get_queue(queue_id).await?;

        let result = sqlx::query("DELETE FROM messages WHERE queue_id = ?")
            .bind(queue_id)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to purge queue: {}", e)))?;

        info!(queue_id = %queue_id, messages_purged = result.rows_affected(), "Queue purged");
        Ok(())
    }

    async fn get_stats(&self, queue_id: &str) -> Result<QueueStats> {
        debug!(queue_id = %queue_id, "Getting queue stats");

        // Verify queue exists
        self.get_queue(queue_id).await?;

        let available: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE queue_id = ? AND state = 'available'",
        )
        .bind(queue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to get stats: {}", e)))?;

        let in_flight: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE queue_id = ? AND state = 'in_flight'",
        )
        .bind(queue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to get stats: {}", e)))?;

        let oldest_timestamp: Option<i64> = sqlx::query_scalar(
            "SELECT MIN(sent_timestamp) FROM messages WHERE queue_id = ? AND state = 'available'",
        )
        .bind(queue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to get stats: {}", e)))?;

        Ok(QueueStats {
            available_messages: available as u64,
            in_flight_messages: in_flight as u64,
            dlq_messages: 0, // TODO: Track DLQ messages
            oldest_message_timestamp: oldest_timestamp.and_then(DateTime::from_timestamp_millis),
        })
    }

    async fn health_check(&self) -> Result<crate::storage::HealthStatus> {
        // Simple health check - try to query the database
        match sqlx::query("SELECT 1").fetch_one(&self.pool).await {
            Ok(_) => Ok(crate::storage::HealthStatus::Healthy),
            Err(e) => {
                tracing::error!(error = %e, "SQLite health check failed");
                Ok(crate::storage::HealthStatus::Unhealthy(format!(
                    "Database error: {}",
                    e
                )))
            }
        }
    }

    // Pub/Sub subscription methods
    async fn create_subscription(
        &self,
        config: crate::types::SubscriptionConfig,
    ) -> Result<crate::types::SubscriptionConfig> {
        debug!(subscription_id = %config.id, topic_id = %config.topic_id, "Creating subscription in SQLite");

        let now = Utc::now().timestamp();

        // Extract push config fields
        let (push_endpoint, retry_min_backoff, retry_max_backoff, retry_max_attempts, push_timeout) =
            if let Some(ref push_config) = config.push_config {
                let retry = push_config.retry_policy.as_ref();
                (
                    Some(push_config.endpoint.clone()),
                    retry.map(|r| r.min_backoff_seconds as i64),
                    retry.map(|r| r.max_backoff_seconds as i64),
                    retry.map(|r| r.max_attempts as i64),
                    push_config.timeout_seconds.map(|t| t as i64),
                )
            } else {
                (None, None, None, None, None)
            };

        sqlx::query(
            r#"
            INSERT INTO subscriptions (
                id, name, topic_id, ack_deadline_seconds, enable_message_ordering,
                filter, dead_letter_topic_id, max_delivery_attempts,
                retain_acked_messages, message_retention_duration,
                push_endpoint, retry_min_backoff_seconds, retry_max_backoff_seconds,
                retry_max_attempts, push_timeout_seconds,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&config.id)
        .bind(&config.name)
        .bind(&config.topic_id)
        .bind(config.ack_deadline_seconds as i64)
        .bind(config.enable_message_ordering)
        .bind(&config.filter)
        .bind(config.dead_letter_policy.as_ref().map(|dlp| dlp.dead_letter_topic.clone()))
        .bind(config.dead_letter_policy.as_ref().map(|dlp| dlp.max_delivery_attempts as i64))
        .bind(false) // retain_acked_messages - not in SubscriptionConfig yet
        .bind(config.message_retention_duration as i64)
        .bind(push_endpoint)
        .bind(retry_min_backoff)
        .bind(retry_max_backoff)
        .bind(retry_max_attempts)
        .bind(push_timeout)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                Error::StorageError(format!("Subscription {} already exists", config.id))
            } else {
                Error::StorageError(format!("Failed to create subscription: {}", e))
            }
        })?;

        info!(subscription_id = %config.id, "Subscription created in SQLite");
        Ok(config)
    }

    async fn get_subscription(&self, id: &str) -> Result<crate::types::SubscriptionConfig> {
        debug!(subscription_id = %id, "Getting subscription from SQLite");

        let row = sqlx::query(
            r#"
            SELECT id, name, topic_id, ack_deadline_seconds, enable_message_ordering,
                   filter, dead_letter_topic_id, max_delivery_attempts,
                   message_retention_duration,
                   push_endpoint, retry_min_backoff_seconds, retry_max_backoff_seconds,
                   retry_max_attempts, push_timeout_seconds
            FROM subscriptions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to get subscription: {}", e)))?
        .ok_or_else(|| Error::SubscriptionNotFound(id.to_string()))?;

        // Parse dead letter policy
        let dead_letter_policy = if let Some(topic_id) = row.get::<Option<String>, _>("dead_letter_topic_id") {
            Some(crate::types::DeadLetterPolicy {
                dead_letter_topic: topic_id,
                max_delivery_attempts: row.get::<Option<i64>, _>("max_delivery_attempts")
                    .map(|a| a as u32)
                    .unwrap_or(5),
            })
        } else {
            None
        };

        // Parse push config
        let push_config = if let Some(endpoint) = row.get::<Option<String>, _>("push_endpoint") {
            let retry_policy = if row.get::<Option<i64>, _>("retry_min_backoff_seconds").is_some() {
                Some(crate::types::RetryPolicy {
                    min_backoff_seconds: row.get::<Option<i64>, _>("retry_min_backoff_seconds")
                        .map(|v| v as u32)
                        .unwrap_or(10),
                    max_backoff_seconds: row.get::<Option<i64>, _>("retry_max_backoff_seconds")
                        .map(|v| v as u32)
                        .unwrap_or(600),
                    max_attempts: row.get::<Option<i64>, _>("retry_max_attempts")
                        .map(|v| v as u32)
                        .unwrap_or(5),
                })
            } else {
                None
            };

            Some(crate::types::PushConfig {
                endpoint,
                retry_policy,
                timeout_seconds: row.get::<Option<i64>, _>("push_timeout_seconds")
                    .map(|v| v as u32),
            })
        } else {
            None
        };

        Ok(crate::types::SubscriptionConfig {
            id: row.get("id"),
            name: row.get("name"),
            topic_id: row.get("topic_id"),
            ack_deadline_seconds: row.get::<i64, _>("ack_deadline_seconds") as u32,
            message_retention_duration: row.get::<i64, _>("message_retention_duration") as u32,
            enable_message_ordering: row.get("enable_message_ordering"),
            filter: row.get("filter"),
            dead_letter_policy,
            push_config,
        })
    }

    async fn delete_subscription(&self, id: &str) -> Result<()> {
        debug!(subscription_id = %id, "Deleting subscription from SQLite");

        let result = sqlx::query("DELETE FROM subscriptions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to delete subscription: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::SubscriptionNotFound(id.to_string()));
        }

        info!(subscription_id = %id, "Subscription deleted from SQLite");
        Ok(())
    }

    async fn list_subscriptions(&self) -> Result<Vec<crate::types::SubscriptionConfig>> {
        debug!("Listing subscriptions from SQLite");

        let rows = sqlx::query(
            r#"
            SELECT id, name, topic_id, ack_deadline_seconds, enable_message_ordering,
                   filter, dead_letter_topic_id, max_delivery_attempts,
                   message_retention_duration,
                   push_endpoint, retry_min_backoff_seconds, retry_max_backoff_seconds,
                   retry_max_attempts, push_timeout_seconds
            FROM subscriptions
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::StorageError(format!("Failed to list subscriptions: {}", e)))?;

        let mut subscriptions = Vec::new();
        for row in rows {
            // Parse dead letter policy
            let dead_letter_policy = if let Some(topic_id) = row.get::<Option<String>, _>("dead_letter_topic_id") {
                Some(crate::types::DeadLetterPolicy {
                    dead_letter_topic: topic_id,
                    max_delivery_attempts: row.get::<Option<i64>, _>("max_delivery_attempts")
                        .map(|a| a as u32)
                        .unwrap_or(5),
                })
            } else {
                None
            };

            // Parse push config
            let push_config = if let Some(endpoint) = row.get::<Option<String>, _>("push_endpoint") {
                let retry_policy = if row.get::<Option<i64>, _>("retry_min_backoff_seconds").is_some() {
                    Some(crate::types::RetryPolicy {
                        min_backoff_seconds: row.get::<Option<i64>, _>("retry_min_backoff_seconds")
                            .map(|v| v as u32)
                            .unwrap_or(10),
                        max_backoff_seconds: row.get::<Option<i64>, _>("retry_max_backoff_seconds")
                            .map(|v| v as u32)
                            .unwrap_or(600),
                        max_attempts: row.get::<Option<i64>, _>("retry_max_attempts")
                            .map(|v| v as u32)
                            .unwrap_or(5),
                    })
                } else {
                    None
                };

                Some(crate::types::PushConfig {
                    endpoint,
                    retry_policy,
                    timeout_seconds: row.get::<Option<i64>, _>("push_timeout_seconds")
                        .map(|v| v as u32),
                })
            } else {
                None
            };

            subscriptions.push(crate::types::SubscriptionConfig {
                id: row.get("id"),
                name: row.get("name"),
                topic_id: row.get("topic_id"),
                ack_deadline_seconds: row.get::<i64, _>("ack_deadline_seconds") as u32,
                message_retention_duration: row.get::<i64, _>("message_retention_duration") as u32,
                enable_message_ordering: row.get("enable_message_ordering"),
                filter: row.get("filter"),
                dead_letter_policy,
                push_config,
            });
        }

        debug!(count = subscriptions.len(), "Listed subscriptions from SQLite");
        Ok(subscriptions)
    }

    async fn process_expired_visibility(&self, queue_id: &str) -> Result<u64> {
        // Delegate to the existing method
        self.process_expired_visibility(queue_id).await
    }
}
