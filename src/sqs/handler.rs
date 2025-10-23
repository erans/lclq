//! SQS action handler - processes SQS actions using the storage backend.

use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use tracing::{debug, info};

use crate::config::LclqConfig;
use crate::sqs::{
    build_change_visibility_batch_response, build_create_queue_response,
    build_delete_message_batch_response, build_delete_message_response, build_error_response,
    build_get_queue_attributes_response, build_get_queue_url_response,
    build_list_queues_response, build_receive_message_response,
    build_send_message_batch_response, build_send_message_response,
    calculate_md5_of_attributes, calculate_md5_of_body, extract_queue_name_from_url,
    BatchErrorEntry, BatchResultEntry, MessageAttributeInfo, ReceivedMessageInfo, SqsAction,
    SqsErrorCode, SqsRequest,
};
use crate::storage::{QueueFilter, StorageBackend};
use crate::types::validation::{validate_sqs_queue_name, SQS_MAX_MESSAGE_SIZE};
use crate::types::{
    DlqConfig, Message, MessageAttributes, MessageId, QueueConfig, QueueType, ReceiveOptions,
};

/// SQS actions handler.
pub struct SqsHandler {
    backend: Arc<dyn StorageBackend>,
    _config: LclqConfig,
    base_url: String,
}

impl SqsHandler {
    /// Create a new SQS handler.
    pub fn new(backend: Arc<dyn StorageBackend>, config: LclqConfig) -> Self {
        let base_url = format!("http://{}:{}", config.server.bind_address, config.server.sqs_port);
        info!(base_url = %base_url, "SQS handler initialized");
        Self {
            backend,
            _config: config,
            base_url,
        }
    }

    /// Handle an SQS request and return response body and content type.
    pub async fn handle_request(&self, request: SqsRequest) -> (String, String) {
        debug!(action = ?request.action, "Handling SQS request");

        let is_json = request.is_json;
        let response_xml = match request.action {
            SqsAction::CreateQueue => self.handle_create_queue(request).await,
            SqsAction::GetQueueUrl => self.handle_get_queue_url(request).await,
            SqsAction::DeleteQueue => self.handle_delete_queue(request).await,
            SqsAction::ListQueues => self.handle_list_queues(request).await,
            SqsAction::SendMessage => self.handle_send_message(request).await,
            SqsAction::SendMessageBatch => self.handle_send_message_batch(request).await,
            SqsAction::ReceiveMessage => self.handle_receive_message(request).await,
            SqsAction::DeleteMessage => self.handle_delete_message(request).await,
            SqsAction::DeleteMessageBatch => self.handle_delete_message_batch(request).await,
            SqsAction::ChangeMessageVisibility => self.handle_change_message_visibility(request).await,
            SqsAction::ChangeMessageVisibilityBatch => self.handle_change_message_visibility_batch(request).await,
            SqsAction::PurgeQueue => self.handle_purge_queue(request).await,
            SqsAction::GetQueueAttributes => self.handle_get_queue_attributes(request).await,
            SqsAction::SetQueueAttributes => self.handle_set_queue_attributes(request).await,
            _ => build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "Action not yet implemented",
            ),
        };

        // Convert to JSON if request was JSON
        if is_json {
            tracing::info!("Converting XML to JSON. XML: {}", response_xml);
            let json_response = xml_to_json_response(&response_xml);
            tracing::info!("JSON response: {}", json_response);
            (json_response, "application/x-amz-json-1.0".to_string())
        } else {
            (response_xml, "application/xml".to_string())
        }
    }

    /// Handle CreateQueue action.
    async fn handle_create_queue(&self, request: SqsRequest) -> String {
        let queue_name = match request.get_required_param("QueueName") {
            Ok(name) => name,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        // Validate queue name
        if let Err(e) = validate_sqs_queue_name(queue_name) {
            return build_error_response(SqsErrorCode::InvalidParameterValue, &e.to_string());
        }

        // Parse queue attributes
        let attributes = request.parse_queue_attributes();

        // Determine queue type
        let is_fifo = queue_name.ends_with(".fifo")
            || attributes.get("FifoQueue").map(|v| v == "true").unwrap_or(false);

        let queue_type = if is_fifo {
            QueueType::SqsFifo
        } else {
            QueueType::SqsStandard
        };

        // Parse queue configuration
        let visibility_timeout = attributes
            .get("VisibilityTimeout")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let message_retention_period = attributes
            .get("MessageRetentionPeriod")
            .and_then(|v| v.parse().ok())
            .unwrap_or(345600);

        let delay_seconds = attributes
            .get("DelaySeconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let content_based_deduplication = attributes
            .get("ContentBasedDeduplication")
            .map(|v| v == "true")
            .unwrap_or(false);

        // Parse redrive policy for DLQ
        let dlq_config = if let Some(redrive_policy) = attributes.get("RedrivePolicy") {
            // RedrivePolicy is JSON: {"maxReceiveCount":"5","deadLetterTargetArn":"arn:..."}
            if let Ok(policy) = serde_json::from_str::<serde_json::Value>(redrive_policy) {
                let max_receive_count = policy["maxReceiveCount"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5);

                let target_arn = policy["deadLetterTargetArn"].as_str().unwrap_or("");
                let target_queue_id = target_arn.split(':').next_back().unwrap_or(target_arn);

                Some(DlqConfig {
                    target_queue_id: target_queue_id.to_string(),
                    max_receive_count,
                })
            } else {
                None
            }
        } else {
            None
        };

        let queue_config = QueueConfig {
            id: queue_name.to_string(),
            name: queue_name.to_string(),
            queue_type,
            visibility_timeout,
            message_retention_period,
            max_message_size: SQS_MAX_MESSAGE_SIZE,
            delay_seconds,
            dlq_config,
            content_based_deduplication,
            tags: std::collections::HashMap::new(),
        };

        // Create queue in backend
        match self.backend.create_queue(queue_config).await {
            Ok(config) => {
                let queue_url = self.get_queue_url(&config.name);
                info!(queue_name = %config.name, queue_url = %queue_url, "Queue created");
                build_create_queue_response(&queue_url)
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    /// Handle GetQueueUrl action.
    async fn handle_get_queue_url(&self, request: SqsRequest) -> String {
        let queue_name = match request.get_required_param("QueueName") {
            Ok(name) => name,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        // Try to get the queue
        match self.backend.get_queue(queue_name).await {
            Ok(_) => {
                let queue_url = self.get_queue_url(queue_name);
                build_get_queue_url_response(&queue_url)
            }
            Err(_) => build_error_response(
                SqsErrorCode::QueueDoesNotExist,
                &format!("Queue '{}' does not exist", queue_name),
            ),
        }
    }

    /// Handle DeleteQueue action.
    async fn handle_delete_queue(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        match self.backend.delete_queue(&queue_name).await {
            Ok(_) => {
                info!(queue_name = %queue_name, "Queue deleted");
                build_delete_message_response() // DeleteQueue has empty response
            }
            Err(_) => build_error_response(
                SqsErrorCode::QueueDoesNotExist,
                &format!("Queue '{}' does not exist", queue_name),
            ),
        }
    }

    /// Handle ListQueues action.
    async fn handle_list_queues(&self, request: SqsRequest) -> String {
        let name_prefix = request.get_param("QueueNamePrefix").map(String::from);

        let filter = name_prefix.map(|prefix| QueueFilter {
            name_prefix: Some(prefix),
        });

        match self.backend.list_queues(filter).await {
            Ok(queues) => {
                let queue_urls: Vec<String> =
                    queues.iter().map(|q| self.get_queue_url(&q.name)).collect();
                build_list_queues_response(&queue_urls)
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    /// Handle SendMessage action.
    async fn handle_send_message(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let message_body = match request.get_required_param("MessageBody") {
            Ok(body) => body,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        // Parse message attributes
        let sqs_attributes = request.parse_message_attributes();
        let mut attributes = MessageAttributes::new();

        for (key, value) in sqs_attributes.iter() {
            attributes.insert(
                key.clone(),
                crate::types::MessageAttributeValue {
                    data_type: value.data_type.clone(),
                    string_value: value.string_value.clone(),
                    binary_value: value.binary_value.as_ref().and_then(|b64| {
                        base64::engine::general_purpose::STANDARD
                            .decode(b64)
                            .ok()
                    }),
                },
            );
        }

        // Parse optional parameters
        let delay_seconds = request
            .get_param("DelaySeconds")
            .and_then(|v| v.parse().ok());

        let message_group_id = request.get_param("MessageGroupId").map(String::from);
        let deduplication_id = request.get_param("MessageDeduplicationId").map(String::from);

        let message = Message {
            id: MessageId::new(),
            body: message_body.to_string(),
            attributes,
            queue_id: queue_name.clone(),
            sent_timestamp: Utc::now(),
            receive_count: 0,
            message_group_id,
            deduplication_id,
            sequence_number: None,
            delay_seconds,
        };

        match self.backend.send_message(&queue_name, message.clone()).await {
            Ok(sent_message) => {
                let md5_of_body = calculate_md5_of_body(&sent_message.body);
                let md5_of_attrs = if !sqs_attributes.is_empty() {
                    Some(calculate_md5_of_attributes(&sqs_attributes))
                } else {
                    None
                };

                info!(
                    queue_name = %queue_name,
                    message_id = %sent_message.id,
                    "Message sent"
                );

                build_send_message_response(
                    &sent_message.id.0,
                    &md5_of_body,
                    md5_of_attrs.as_deref(),
                )
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    /// Handle SendMessageBatch action.
    async fn handle_send_message_batch(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        // Parse batch entries
        let entries = request.parse_send_message_batch_entries();

        if entries.is_empty() {
            return build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "No batch entries provided",
            );
        }

        if entries.len() > 10 {
            return build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "Maximum 10 batch entries allowed",
            );
        }

        let mut successful_entries = Vec::new();
        let mut failed_entries = Vec::new();

        // Process each entry
        for entry in entries {
            // Validate message body size
            if entry.message_body.len() > SQS_MAX_MESSAGE_SIZE {
                failed_entries.push(BatchErrorEntry {
                    id: entry.id.clone(),
                    code: "MessageTooLong".to_string(),
                    message: format!(
                        "Message body size {} exceeds maximum {}",
                        entry.message_body.len(),
                        SQS_MAX_MESSAGE_SIZE
                    ),
                    sender_fault: true,
                });
                continue;
            }

            // Create message
            let message = Message {
                id: MessageId::new(),
                body: entry.message_body.clone(),
                attributes: MessageAttributes::new(), // TODO: Support batch message attributes
                queue_id: queue_name.clone(),
                sent_timestamp: Utc::now(),
                receive_count: 0,
                message_group_id: entry.message_group_id.clone(),
                deduplication_id: entry.message_deduplication_id.clone(),
                sequence_number: None,
                delay_seconds: entry.delay_seconds,
            };

            // Send message
            match self.backend.send_message(&queue_name, message.clone()).await {
                Ok(sent_message) => {
                    let md5_of_body = calculate_md5_of_body(&sent_message.body);

                    successful_entries.push(BatchResultEntry {
                        id: entry.id.clone(),
                        message_id: sent_message.id.0.clone(),
                        md5_of_body,
                        md5_of_attrs: None, // TODO: Support batch message attributes
                    });

                    debug!(
                        queue_name = %queue_name,
                        entry_id = %entry.id,
                        message_id = %sent_message.id,
                        "Batch message sent"
                    );
                }
                Err(e) => {
                    failed_entries.push(BatchErrorEntry {
                        id: entry.id.clone(),
                        code: "InternalError".to_string(),
                        message: e.to_string(),
                        sender_fault: false,
                    });
                }
            }
        }

        info!(
            queue_name = %queue_name,
            successful = successful_entries.len(),
            failed = failed_entries.len(),
            "Batch messages processed"
        );

        build_send_message_batch_response(&successful_entries, &failed_entries)
    }

    /// Handle ReceiveMessage action.
    async fn handle_receive_message(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        let max_messages = request
            .get_param("MaxNumberOfMessages")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
            .min(10);

        let visibility_timeout = request
            .get_param("VisibilityTimeout")
            .and_then(|v| v.parse().ok());

        let wait_time_seconds = request
            .get_param("WaitTimeSeconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let options = ReceiveOptions {
            max_messages,
            visibility_timeout,
            wait_time_seconds,
            attribute_names: vec![],
            message_attribute_names: vec![],
        };

        match self.backend.receive_messages(&queue_name, options).await {
            Ok(messages) => {
                let mut message_infos = Vec::new();

                for received in messages {
                    let md5_of_body = calculate_md5_of_body(&received.message.body);

                    // Convert attributes
                    let attributes = vec![
                        (
                            "SenderId".to_string(),
                            "AIDAIT2UOQQY3AUEKVGXU".to_string(), // Dummy sender ID
                        ),
                        (
                            "SentTimestamp".to_string(),
                            received.message.sent_timestamp.timestamp_millis().to_string(),
                        ),
                        (
                            "ApproximateReceiveCount".to_string(),
                            received.message.receive_count.to_string(),
                        ),
                        (
                            "ApproximateFirstReceiveTimestamp".to_string(),
                            Utc::now().timestamp_millis().to_string(),
                        ),
                    ];

                    // Convert message attributes
                    let mut msg_attributes = Vec::new();
                    for (key, value) in &received.message.attributes {
                        msg_attributes.push((
                            key.clone(),
                            MessageAttributeInfo {
                                data_type: value.data_type.clone(),
                                string_value: value.string_value.clone(),
                            },
                        ));
                    }

                    message_infos.push(ReceivedMessageInfo {
                        message_id: received.message.id.0.clone(),
                        receipt_handle: received.receipt_handle,
                        md5_of_body,
                        body: received.message.body,
                        attributes,
                        message_attributes: msg_attributes,
                    });
                }

                debug!(
                    queue_name = %queue_name,
                    count = message_infos.len(),
                    "Messages received"
                );

                build_receive_message_response(&message_infos)
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    /// Handle DeleteMessage action.
    async fn handle_delete_message(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let receipt_handle = match request.get_required_param("ReceiptHandle") {
            Ok(handle) => handle,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        match self.backend.delete_message(&queue_name, receipt_handle).await {
            Ok(_) => {
                debug!(queue_name = %queue_name, "Message deleted");
                build_delete_message_response()
            }
            Err(_) => build_error_response(
                SqsErrorCode::ReceiptHandleIsInvalid,
                "Receipt handle is invalid",
            ),
        }
    }

    /// Handle DeleteMessageBatch action.
    async fn handle_delete_message_batch(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        // Parse batch entries
        let entries = request.parse_delete_message_batch_entries();

        if entries.is_empty() {
            return build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "No batch entries provided",
            );
        }

        if entries.len() > 10 {
            return build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "Maximum 10 batch entries allowed",
            );
        }

        let mut successful_ids = Vec::new();
        let mut failed_entries = Vec::new();

        // Process each entry
        for entry in entries {
            match self.backend.delete_message(&queue_name, &entry.receipt_handle).await {
                Ok(_) => {
                    successful_ids.push(entry.id.clone());
                    debug!(
                        queue_name = %queue_name,
                        entry_id = %entry.id,
                        "Batch message deleted"
                    );
                }
                Err(e) => {
                    failed_entries.push(BatchErrorEntry {
                        id: entry.id.clone(),
                        code: "ReceiptHandleIsInvalid".to_string(),
                        message: e.to_string(),
                        sender_fault: true,
                    });
                }
            }
        }

        info!(
            queue_name = %queue_name,
            successful = successful_ids.len(),
            failed = failed_entries.len(),
            "Batch messages deleted"
        );

        build_delete_message_batch_response(&successful_ids, &failed_entries)
    }

    /// Handle ChangeMessageVisibility action.
    async fn handle_change_message_visibility(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let receipt_handle = match request.get_required_param("ReceiptHandle") {
            Ok(handle) => handle,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let visibility_timeout = match request.get_required_param("VisibilityTimeout") {
            Ok(timeout_str) => match timeout_str.parse::<u32>() {
                Ok(timeout) if timeout <= 43200 => timeout,
                _ => {
                    return build_error_response(
                        SqsErrorCode::InvalidParameterValue,
                        "VisibilityTimeout must be between 0 and 43200 seconds",
                    )
                }
            },
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        match self
            .backend
            .change_visibility(&queue_name, receipt_handle, visibility_timeout)
            .await
        {
            Ok(_) => {
                debug!(
                    queue_name = %queue_name,
                    visibility_timeout = visibility_timeout,
                    "Message visibility changed"
                );
                build_delete_message_response() // Empty response
            }
            Err(_) => build_error_response(
                SqsErrorCode::ReceiptHandleIsInvalid,
                "Receipt handle is invalid",
            ),
        }
    }

    /// Handle ChangeMessageVisibilityBatch action.
    async fn handle_change_message_visibility_batch(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        // Parse batch entries
        let entries = request.parse_change_visibility_batch_entries();

        if entries.is_empty() {
            return build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "No batch entries provided",
            );
        }

        if entries.len() > 10 {
            return build_error_response(
                SqsErrorCode::InvalidParameterValue,
                "Maximum 10 batch entries allowed",
            );
        }

        let mut successful_ids = Vec::new();
        let mut failed_entries = Vec::new();

        // Process each entry
        for entry in entries {
            // Validate visibility timeout
            if entry.visibility_timeout > 43200 {
                failed_entries.push(BatchErrorEntry {
                    id: entry.id.clone(),
                    code: "InvalidParameterValue".to_string(),
                    message: "VisibilityTimeout must be between 0 and 43200 seconds".to_string(),
                    sender_fault: true,
                });
                continue;
            }

            match self
                .backend
                .change_visibility(&queue_name, &entry.receipt_handle, entry.visibility_timeout)
                .await
            {
                Ok(_) => {
                    successful_ids.push(entry.id.clone());
                    debug!(
                        queue_name = %queue_name,
                        entry_id = %entry.id,
                        visibility_timeout = entry.visibility_timeout,
                        "Batch message visibility changed"
                    );
                }
                Err(e) => {
                    failed_entries.push(BatchErrorEntry {
                        id: entry.id.clone(),
                        code: "ReceiptHandleIsInvalid".to_string(),
                        message: e.to_string(),
                        sender_fault: true,
                    });
                }
            }
        }

        info!(
            queue_name = %queue_name,
            successful = successful_ids.len(),
            failed = failed_entries.len(),
            "Batch message visibility changes processed"
        );

        build_change_visibility_batch_response(&successful_ids, &failed_entries)
    }

    /// Handle PurgeQueue action.
    async fn handle_purge_queue(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        match self.backend.purge_queue(&queue_name).await {
            Ok(_) => {
                info!(queue_name = %queue_name, "Queue purged");
                build_delete_message_response() // PurgeQueue has empty response
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    /// Handle GetQueueAttributes action.
    async fn handle_get_queue_attributes(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        // Parse requested attribute names
        let mut requested_attrs = request.parse_attribute_names();

        // If no attributes specified, default to "All"
        if requested_attrs.is_empty() {
            requested_attrs.push("All".to_string());
        }

        // Check if "All" is requested
        let request_all = requested_attrs.iter().any(|a| a == "All");

        // Get queue config and stats
        let queue_config = match self.backend.get_queue(&queue_name).await {
            Ok(config) => config,
            Err(_) => {
                return build_error_response(
                    SqsErrorCode::QueueDoesNotExist,
                    "The specified queue does not exist",
                )
            }
        };

        let stats = match self.backend.get_stats(&queue_name).await {
            Ok(s) => s,
            Err(e) => {
                return build_error_response(SqsErrorCode::InternalError, &e.to_string())
            }
        };

        // Build attribute list
        let mut attributes: Vec<(String, String)> = Vec::new();

        for attr_name in &requested_attrs {
            match attr_name.parse::<crate::sqs::QueueAttribute>() {
                Ok(attr) => {
                    Self::add_queue_attribute(&mut attributes, &attr, &queue_config, &stats, request_all);
                }
                Err(_) => {
                    // Unknown attribute, ignore (AWS behavior)
                    debug!(attribute = %attr_name, "Unknown attribute requested");
                }
            }
        }

        build_get_queue_attributes_response(&attributes)
    }

    /// Handle SetQueueAttributes action.
    async fn handle_set_queue_attributes(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(SqsErrorCode::InvalidParameterValue, "Invalid QueueUrl")
            }
        };

        // Get current queue configuration
        let mut queue_config = match self.backend.get_queue(&queue_name).await {
            Ok(config) => config,
            Err(_) => {
                return build_error_response(
                    SqsErrorCode::QueueDoesNotExist,
                    "The specified queue does not exist",
                )
            }
        };

        // Parse attributes to update
        let attributes = request.parse_queue_attributes();

        // Update modifiable attributes
        for (key, value) in attributes {
            match key.as_str() {
                "VisibilityTimeout" => {
                    match value.parse::<u32>() {
                        Ok(timeout) if timeout <= 43200 => {
                            queue_config.visibility_timeout = timeout;
                        }
                        _ => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "VisibilityTimeout must be between 0 and 43200 seconds",
                            )
                        }
                    }
                }
                "MessageRetentionPeriod" => {
                    match value.parse::<u32>() {
                        Ok(period) if (60..=1209600).contains(&period) => {
                            queue_config.message_retention_period = period;
                        }
                        _ => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "MessageRetentionPeriod must be between 60 and 1209600 seconds",
                            )
                        }
                    }
                }
                "MaximumMessageSize" => {
                    match value.parse::<usize>() {
                        Ok(size) if (1024..=262144).contains(&size) => {
                            queue_config.max_message_size = size;
                        }
                        _ => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "MaximumMessageSize must be between 1024 and 262144 bytes",
                            )
                        }
                    }
                }
                "DelaySeconds" => {
                    match value.parse::<u32>() {
                        Ok(delay) if delay <= 900 => {
                            queue_config.delay_seconds = delay;
                        }
                        _ => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "DelaySeconds must be between 0 and 900 seconds",
                            )
                        }
                    }
                }
                "ContentBasedDeduplication" => {
                    if queue_config.queue_type != QueueType::SqsFifo {
                        return build_error_response(
                            SqsErrorCode::InvalidParameterValue,
                            "ContentBasedDeduplication is only valid for FIFO queues",
                        );
                    }
                    match value.to_lowercase().as_str() {
                        "true" => queue_config.content_based_deduplication = true,
                        "false" => queue_config.content_based_deduplication = false,
                        _ => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "ContentBasedDeduplication must be 'true' or 'false'",
                            )
                        }
                    }
                }
                "RedrivePolicy" => {
                    // Parse the redrive policy JSON
                    match serde_json::from_str::<serde_json::Value>(&value) {
                        Ok(policy) => {
                            let target_arn = policy["deadLetterTargetArn"].as_str();
                            let max_receive_count = policy["maxReceiveCount"].as_u64();

                            if let (Some(arn), Some(max_count)) = (target_arn, max_receive_count) {
                                // Extract queue name from ARN (format: arn:aws:sqs:region:account:queue-name)
                                let target_queue_id = arn.split(':').next_back().unwrap_or(arn).to_string();

                                queue_config.dlq_config = Some(DlqConfig {
                                    target_queue_id,
                                    max_receive_count: max_count as u32,
                                });
                            } else {
                                return build_error_response(
                                    SqsErrorCode::InvalidParameterValue,
                                    "Invalid RedrivePolicy format",
                                );
                            }
                        }
                        Err(_) => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "Invalid RedrivePolicy JSON",
                            )
                        }
                    }
                }
                "FifoQueue" => {
                    // FifoQueue cannot be changed after queue creation
                    return build_error_response(
                        SqsErrorCode::InvalidParameterValue,
                        "FifoQueue attribute cannot be modified",
                    );
                }
                _ => {
                    // Unknown attribute, ignore (AWS behavior)
                    debug!(attribute = %key, "Unknown attribute in SetQueueAttributes");
                }
            }
        }

        // Update the queue in the backend
        match self.backend.update_queue(queue_config).await {
            Ok(_) => {
                info!(queue_name = %queue_name, "Queue attributes updated");
                build_delete_message_response() // SetQueueAttributes has empty response
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    /// Add a queue attribute to the attributes list.
    fn add_queue_attribute(
        attributes: &mut Vec<(String, String)>,
        attr: &crate::sqs::QueueAttribute,
        queue_config: &QueueConfig,
        stats: &crate::types::QueueStats,
        _include_all: bool,
    ) {
        use crate::sqs::QueueAttribute;

        match attr {
            QueueAttribute::All => {
                // Add all attributes
                Self::add_queue_attribute(attributes, &QueueAttribute::VisibilityTimeout, queue_config, stats, false);
                Self::add_queue_attribute(attributes, &QueueAttribute::DelaySeconds, queue_config, stats, false);
                Self::add_queue_attribute(attributes, &QueueAttribute::MaximumMessageSize, queue_config, stats, false);
                Self::add_queue_attribute(attributes, &QueueAttribute::MessageRetentionPeriod, queue_config, stats, false);
                Self::add_queue_attribute(attributes, &QueueAttribute::ApproximateNumberOfMessages, queue_config, stats, false);
                Self::add_queue_attribute(attributes, &QueueAttribute::ApproximateNumberOfMessagesNotVisible, queue_config, stats, false);
                Self::add_queue_attribute(attributes, &QueueAttribute::QueueArn, queue_config, stats, false);

                if queue_config.queue_type == crate::types::QueueType::SqsFifo {
                    Self::add_queue_attribute(attributes, &QueueAttribute::FifoQueue, queue_config, stats, false);
                    Self::add_queue_attribute(attributes, &QueueAttribute::ContentBasedDeduplication, queue_config, stats, false);
                }

                if queue_config.dlq_config.is_some() {
                    Self::add_queue_attribute(attributes, &QueueAttribute::RedrivePolicy, queue_config, stats, false);
                }
            }
            QueueAttribute::VisibilityTimeout => {
                attributes.push(("VisibilityTimeout".to_string(), queue_config.visibility_timeout.to_string()));
            }
            QueueAttribute::DelaySeconds => {
                attributes.push(("DelaySeconds".to_string(), queue_config.delay_seconds.to_string()));
            }
            QueueAttribute::MaximumMessageSize => {
                attributes.push(("MaximumMessageSize".to_string(), queue_config.max_message_size.to_string()));
            }
            QueueAttribute::MessageRetentionPeriod => {
                attributes.push(("MessageRetentionPeriod".to_string(), queue_config.message_retention_period.to_string()));
            }
            QueueAttribute::ApproximateNumberOfMessages => {
                attributes.push(("ApproximateNumberOfMessages".to_string(), stats.available_messages.to_string()));
            }
            QueueAttribute::ApproximateNumberOfMessagesNotVisible => {
                attributes.push(("ApproximateNumberOfMessagesNotVisible".to_string(), stats.in_flight_messages.to_string()));
            }
            QueueAttribute::ApproximateNumberOfMessagesDelayed => {
                // TODO: Track delayed messages separately
                attributes.push(("ApproximateNumberOfMessagesDelayed".to_string(), "0".to_string()));
            }
            QueueAttribute::QueueArn => {
                // Generate a fake ARN (local development)
                let arn = format!("arn:aws:sqs:us-east-1:000000000000:{}", queue_config.name);
                attributes.push(("QueueArn".to_string(), arn));
            }
            QueueAttribute::FifoQueue => {
                let is_fifo = queue_config.queue_type == crate::types::QueueType::SqsFifo;
                attributes.push(("FifoQueue".to_string(), is_fifo.to_string()));
            }
            QueueAttribute::ContentBasedDeduplication => {
                attributes.push(("ContentBasedDeduplication".to_string(), queue_config.content_based_deduplication.to_string()));
            }
            QueueAttribute::RedrivePolicy => {
                if let Some(dlq) = &queue_config.dlq_config {
                    // Format as JSON
                    let policy = format!(
                        r#"{{"deadLetterTargetArn":"arn:aws:sqs:us-east-1:000000000000:{}","maxReceiveCount":{}}}"#,
                        dlq.target_queue_id, dlq.max_receive_count
                    );
                    attributes.push(("RedrivePolicy".to_string(), policy));
                }
            }
            QueueAttribute::CreatedTimestamp | QueueAttribute::LastModifiedTimestamp => {
                // TODO: Track these timestamps in QueueConfig
                let now = chrono::Utc::now().timestamp();
                attributes.push((attr.as_str().to_string(), now.to_string()));
            }
        }
    }

    /// Generate queue URL for a queue name.
    fn get_queue_url(&self, queue_name: &str) -> String {
        format!("{}/queue/{}", self.base_url, queue_name)
    }
}

/// Convert XML response to JSON for AWS JSON 1.0 protocol.
/// This is a simple implementation that extracts common fields.
fn xml_to_json_response(xml: &str) -> String {
    use serde_json::json;

    // For empty responses (like DeleteMessage), return empty JSON
    if xml.contains("DeleteMessageResponse") || xml.contains("PurgeQueueResponse") {
        return json!({}).to_string();
    }

    // For Messages in ReceiveMessage - must check BEFORE MessageId
    if xml.contains("<Message>") && xml.contains("ReceiveMessageResponse") {
        let mut messages = Vec::new();
        let mut pos = 0;

        while let Some(msg_start) = xml[pos..].find("<Message>") {
            let abs_start = pos + msg_start;
            if let Some(msg_end_rel) = xml[abs_start..].find("</Message>") {
                let msg_xml = &xml[abs_start..abs_start + msg_end_rel + 10];

                let mut message = serde_json::Map::new();

                // Extract MessageId
                if let Some(id_start) = msg_xml.find("<MessageId>") {
                    if let Some(id_end) = msg_xml.find("</MessageId>") {
                        let id = &msg_xml[id_start + 11..id_end];
                        message.insert("MessageId".to_string(), json!(id));
                    }
                }

                // Extract ReceiptHandle
                if let Some(rh_start) = msg_xml.find("<ReceiptHandle>") {
                    if let Some(rh_end) = msg_xml.find("</ReceiptHandle>") {
                        let rh = &msg_xml[rh_start + 15..rh_end];
                        message.insert("ReceiptHandle".to_string(), json!(rh));
                    }
                }

                // Extract Body
                if let Some(body_start) = msg_xml.find("<Body>") {
                    if let Some(body_end) = msg_xml.find("</Body>") {
                        let body = &msg_xml[body_start + 6..body_end];
                        message.insert("Body".to_string(), json!(body));
                    }
                }

                // Extract MD5
                if let Some(md5_start) = msg_xml.find("<MD5OfBody>") {
                    if let Some(md5_end) = msg_xml.find("</MD5OfBody>") {
                        let md5 = &msg_xml[md5_start + 11..md5_end];
                        message.insert("MD5OfBody".to_string(), json!(md5));
                    }
                }

                messages.push(message);
                pos = abs_start + msg_end_rel + 10;
            } else {
                break;
            }
        }

        return json!({"Messages": messages}).to_string();
    }

    // For ListQueues - collect all QueueUrl elements into an array
    if xml.contains("ListQueuesResponse") || xml.contains("ListQueuesResult") {
        let mut queue_urls = Vec::new();
        let mut pos = 0;

        while let Some(url_start) = xml[pos..].find("<QueueUrl>") {
            let abs_start = pos + url_start;
            if let Some(url_end_rel) = xml[abs_start..].find("</QueueUrl>") {
                let url = &xml[abs_start + 10..abs_start + url_end_rel];
                queue_urls.push(url);
                pos = abs_start + url_end_rel + 11;
            } else {
                break;
            }
        }

        return json!({"QueueUrls": queue_urls}).to_string();
    }

    // For GetQueueAttributes - collect all Attribute elements into a map
    if xml.contains("GetQueueAttributesResponse") || xml.contains("GetQueueAttributesResult") {
        let mut attributes = serde_json::Map::new();
        let mut pos = 0;

        while let Some(attr_start) = xml[pos..].find("<Attribute>") {
            let abs_start = pos + attr_start;
            if let Some(attr_end_rel) = xml[abs_start..].find("</Attribute>") {
                let attr_xml = &xml[abs_start..abs_start + attr_end_rel + 12];

                // Extract Name
                if let Some(name_start) = attr_xml.find("<Name>") {
                    if let Some(name_end) = attr_xml.find("</Name>") {
                        let name = &attr_xml[name_start + 6..name_end];

                        // Extract Value
                        if let Some(value_start) = attr_xml.find("<Value>") {
                            if let Some(value_end) = attr_xml.find("</Value>") {
                                let value = &attr_xml[value_start + 7..value_end];
                                attributes.insert(name.to_string(), json!(value));
                            }
                        }
                    }
                }

                pos = abs_start + attr_end_rel + 12;
            } else {
                break;
            }
        }

        return json!({"Attributes": attributes}).to_string();
    }

    // For SendMessageBatch - collect successful and failed entries
    if xml.contains("SendMessageBatchResponse") || xml.contains("SendMessageBatchResult") {
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        let mut pos = 0;

        // Parse successful entries
        while let Some(entry_start) = xml[pos..].find("<SendMessageBatchResultEntry>") {
            let abs_start = pos + entry_start;
            if let Some(entry_end_rel) = xml[abs_start..].find("</SendMessageBatchResultEntry>") {
                let entry_xml = &xml[abs_start..abs_start + entry_end_rel + 30];
                let mut entry = serde_json::Map::new();

                if let Some(id_start) = entry_xml.find("<Id>") {
                    if let Some(id_end) = entry_xml.find("</Id>") {
                        entry.insert("Id".to_string(), json!(&entry_xml[id_start + 4..id_end]));
                    }
                }

                if let Some(mid_start) = entry_xml.find("<MessageId>") {
                    if let Some(mid_end) = entry_xml.find("</MessageId>") {
                        entry.insert("MessageId".to_string(), json!(&entry_xml[mid_start + 11..mid_end]));
                    }
                }

                if let Some(md5_start) = entry_xml.find("<MD5OfMessageBody>") {
                    if let Some(md5_end) = entry_xml.find("</MD5OfMessageBody>") {
                        entry.insert("MD5OfMessageBody".to_string(), json!(&entry_xml[md5_start + 18..md5_end]));
                    }
                }

                successful.push(entry);
                pos = abs_start + entry_end_rel + 30;
            } else {
                break;
            }
        }

        // Parse failed entries
        pos = 0;
        while let Some(entry_start) = xml[pos..].find("<BatchResultErrorEntry>") {
            let abs_start = pos + entry_start;
            if let Some(entry_end_rel) = xml[abs_start..].find("</BatchResultErrorEntry>") {
                let entry_xml = &xml[abs_start..abs_start + entry_end_rel + 24];
                let mut entry = serde_json::Map::new();

                if let Some(id_start) = entry_xml.find("<Id>") {
                    if let Some(id_end) = entry_xml.find("</Id>") {
                        entry.insert("Id".to_string(), json!(&entry_xml[id_start + 4..id_end]));
                    }
                }

                if let Some(code_start) = entry_xml.find("<Code>") {
                    if let Some(code_end) = entry_xml.find("</Code>") {
                        entry.insert("Code".to_string(), json!(&entry_xml[code_start + 6..code_end]));
                    }
                }

                if let Some(msg_start) = entry_xml.find("<Message>") {
                    if let Some(msg_end) = entry_xml.find("</Message>") {
                        entry.insert("Message".to_string(), json!(&entry_xml[msg_start + 9..msg_end]));
                    }
                }

                if let Some(sf_start) = entry_xml.find("<SenderFault>") {
                    if let Some(sf_end) = entry_xml.find("</SenderFault>") {
                        let sf_val = &entry_xml[sf_start + 13..sf_end] == "true";
                        entry.insert("SenderFault".to_string(), json!(sf_val));
                    }
                }

                failed.push(entry);
                pos = abs_start + entry_end_rel + 24;
            } else {
                break;
            }
        }

        let mut result = serde_json::Map::new();
        result.insert("Successful".to_string(), json!(successful));
        result.insert("Failed".to_string(), json!(failed));
        return json!(result).to_string();
    }

    // For DeleteMessageBatch and ChangeMessageVisibilityBatch - collect successful and failed entries
    if xml.contains("DeleteMessageBatchResponse") || xml.contains("DeleteMessageBatchResult")
        || xml.contains("ChangeMessageVisibilityBatchResponse") || xml.contains("ChangeMessageVisibilityBatchResult") {
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        let mut pos = 0;

        // Determine the result entry tag name
        let success_tag = if xml.contains("DeleteMessageBatch") {
            "DeleteMessageBatchResultEntry"
        } else {
            "ChangeMessageVisibilityBatchResultEntry"
        };

        // Parse successful entries
        while let Some(entry_start) = xml[pos..].find(&format!("<{}>", success_tag)) {
            let abs_start = pos + entry_start;
            let end_tag = format!("</{}>", success_tag);
            if let Some(entry_end_rel) = xml[abs_start..].find(&end_tag) {
                let entry_xml = &xml[abs_start..abs_start + entry_end_rel + end_tag.len()];
                let mut entry = serde_json::Map::new();

                // Extract Id
                if let Some(id_start) = entry_xml.find("<Id>") {
                    if let Some(id_end) = entry_xml.find("</Id>") {
                        entry.insert("Id".to_string(), json!(&entry_xml[id_start + 4..id_end]));
                    }
                }

                successful.push(entry);
                pos = abs_start + entry_end_rel + end_tag.len();
            } else {
                break;
            }
        }

        // Parse failed entries (shared structure)
        pos = 0;
        while let Some(entry_start) = xml[pos..].find("<BatchResultErrorEntry>") {
            let abs_start = pos + entry_start;
            if let Some(entry_end_rel) = xml[abs_start..].find("</BatchResultErrorEntry>") {
                let entry_xml = &xml[abs_start..abs_start + entry_end_rel + 24];
                let mut entry = serde_json::Map::new();

                if let Some(id_start) = entry_xml.find("<Id>") {
                    if let Some(id_end) = entry_xml.find("</Id>") {
                        entry.insert("Id".to_string(), json!(&entry_xml[id_start + 4..id_end]));
                    }
                }

                if let Some(code_start) = entry_xml.find("<Code>") {
                    if let Some(code_end) = entry_xml.find("</Code>") {
                        entry.insert("Code".to_string(), json!(&entry_xml[code_start + 6..code_end]));
                    }
                }

                if let Some(msg_start) = entry_xml.find("<Message>") {
                    if let Some(msg_end) = entry_xml.find("</Message>") {
                        entry.insert("Message".to_string(), json!(&entry_xml[msg_start + 9..msg_end]));
                    }
                }

                if let Some(sf_start) = entry_xml.find("<SenderFault>") {
                    if let Some(sf_end) = entry_xml.find("</SenderFault>") {
                        let sf_val = &entry_xml[sf_start + 13..sf_end] == "true";
                        entry.insert("SenderFault".to_string(), json!(sf_val));
                    }
                }

                failed.push(entry);
                pos = abs_start + entry_end_rel + 24;
            } else {
                break;
            }
        }

        let mut result = serde_json::Map::new();
        result.insert("Successful".to_string(), json!(successful));
        result.insert("Failed".to_string(), json!(failed));
        return json!(result).to_string();
    }

    // Simple parsing - extract QueueUrl if present (for CreateQueue, GetQueueUrl)
    if let Some(start) = xml.find("<QueueUrl>") {
        if let Some(end) = xml.find("</QueueUrl>") {
            let queue_url = &xml[start + 10..end];
            return json!({"QueueUrl": queue_url}).to_string();
        }
    }

    // Extract MessageId if present (SendMessage response)
    if let Some(start) = xml.find("<MessageId>") {
        if let Some(end) = xml.find("</MessageId>") {
            let message_id = &xml[start + 11..end];
            let mut response = json!({"MessageId": message_id});

            // Also extract MD5 if present
            if let Some(md5_start) = xml.find("<MD5OfMessageBody>") {
                if let Some(md5_end) = xml.find("</MD5OfMessageBody>") {
                    let md5 = &xml[md5_start + 18..md5_end];
                    response["MD5OfMessageBody"] = json!(md5);
                }
            }

            return response.to_string();
        }
    }

    // Default: return empty JSON
    json!({}).to_string()
}

