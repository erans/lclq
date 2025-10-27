//! SQS action handler - processes SQS actions using the storage backend.

use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use tracing::{debug, info};

use crate::config::LclqConfig;
use crate::sqs::{
    BatchErrorEntry, BatchResultEntry, MessageAttributeInfo, ReceivedMessageInfo, SqsAction,
    SqsErrorCode, SqsRequest, build_change_message_visibility_response,
    build_change_visibility_batch_response, build_create_queue_response,
    build_delete_message_batch_response, build_delete_message_response, build_error_response,
    build_get_queue_attributes_response, build_get_queue_url_response, build_list_queues_response,
    build_receive_message_response, build_send_message_batch_response, build_send_message_response,
    build_tag_queue_response, build_untag_queue_response, calculate_md5_of_attributes,
    calculate_md5_of_body, escape_xml, extract_queue_name_from_url,
};
use crate::storage::{QueueFilter, StorageBackend};
use crate::types::validation::{SQS_MAX_MESSAGE_SIZE, validate_sqs_queue_name};
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
        let base_url = format!(
            "http://{}:{}",
            config.server.bind_address, config.server.sqs_port
        );
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
            SqsAction::ChangeMessageVisibility => {
                self.handle_change_message_visibility(request).await
            }
            SqsAction::ChangeMessageVisibilityBatch => {
                self.handle_change_message_visibility_batch(request).await
            }
            SqsAction::PurgeQueue => self.handle_purge_queue(request).await,
            SqsAction::GetQueueAttributes => self.handle_get_queue_attributes(request).await,
            SqsAction::SetQueueAttributes => self.handle_set_queue_attributes(request).await,
            SqsAction::TagQueue => self.handle_tag_queue(request).await,
            SqsAction::UntagQueue => self.handle_untag_queue(request).await,
            SqsAction::ListQueueTags => self.handle_list_queue_tags(request).await,
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
            || attributes
                .get("FifoQueue")
                .map(|v| v == "true")
                .unwrap_or(false);

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

        // Parse redrive allow policy
        let redrive_allow_policy = if let Some(rap_json) = attributes.get("RedriveAllowPolicy") {
            match serde_json::from_str::<serde_json::Value>(rap_json) {
                Ok(policy) => {
                    use crate::types::{RedriveAllowPolicy, RedrivePermission};

                    let permission_str = policy["redrivePermission"].as_str();

                    permission_str.and_then(|perm| {
                        let permission = match perm {
                            "allowAll" => Some(RedrivePermission::AllowAll),
                            "denyAll" => Some(RedrivePermission::DenyAll),
                            "byQueue" => {
                                let source_arns = policy["sourceQueueArns"]
                                    .as_array()
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect::<Vec<String>>()
                                    })
                                    .unwrap_or_default();

                                Some(RedrivePermission::ByQueue {
                                    source_queue_arns: source_arns,
                                })
                            }
                            _ => None,
                        };

                        permission.map(|p| RedriveAllowPolicy { permission: p })
                    })
                }
                Err(_) => None,
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
            redrive_allow_policy,
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                    binary_value: value
                        .binary_value
                        .as_ref()
                        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok()),
                },
            );
        }

        // Parse optional parameters
        let delay_seconds = request
            .get_param("DelaySeconds")
            .and_then(|v| v.parse().ok());

        let message_group_id = request.get_param("MessageGroupId").map(String::from);
        let deduplication_id = request
            .get_param("MessageDeduplicationId")
            .map(String::from);

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

        match self
            .backend
            .send_message(&queue_name, message.clone())
            .await
        {
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
            match self
                .backend
                .send_message(&queue_name, message.clone())
                .await
            {
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                            received
                                .message
                                .sent_timestamp
                                .timestamp_millis()
                                .to_string(),
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
            }
        };

        match self
            .backend
            .delete_message(&queue_name, receipt_handle)
            .await
        {
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
            match self
                .backend
                .delete_message(&queue_name, &entry.receipt_handle)
                .await
            {
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
                    );
                }
            },
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                build_change_message_visibility_response()
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
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
                );
            }
        };

        let stats = match self.backend.get_stats(&queue_name).await {
            Ok(s) => s,
            Err(e) => return build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        };

        // Build attribute list
        let mut attributes: Vec<(String, String)> = Vec::new();

        for attr_name in &requested_attrs {
            match attr_name.parse::<crate::sqs::QueueAttribute>() {
                Ok(attr) => {
                    Self::add_queue_attribute(
                        &mut attributes,
                        &attr,
                        &queue_config,
                        &stats,
                        request_all,
                    );
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
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
            }
        };

        // Get current queue configuration
        let mut queue_config = match self.backend.get_queue(&queue_name).await {
            Ok(config) => config,
            Err(_) => {
                return build_error_response(
                    SqsErrorCode::QueueDoesNotExist,
                    "The specified queue does not exist",
                );
            }
        };

        // Parse attributes to update
        let attributes = request.parse_queue_attributes();

        // Update modifiable attributes
        for (key, value) in attributes {
            match key.as_str() {
                "VisibilityTimeout" => match value.parse::<u32>() {
                    Ok(timeout) if timeout <= 43200 => {
                        queue_config.visibility_timeout = timeout;
                    }
                    _ => {
                        return build_error_response(
                            SqsErrorCode::InvalidParameterValue,
                            "VisibilityTimeout must be between 0 and 43200 seconds",
                        );
                    }
                },
                "MessageRetentionPeriod" => match value.parse::<u32>() {
                    Ok(period) if (60..=1209600).contains(&period) => {
                        queue_config.message_retention_period = period;
                    }
                    _ => {
                        return build_error_response(
                            SqsErrorCode::InvalidParameterValue,
                            "MessageRetentionPeriod must be between 60 and 1209600 seconds",
                        );
                    }
                },
                "MaximumMessageSize" => match value.parse::<usize>() {
                    Ok(size) if (1024..=262144).contains(&size) => {
                        queue_config.max_message_size = size;
                    }
                    _ => {
                        return build_error_response(
                            SqsErrorCode::InvalidParameterValue,
                            "MaximumMessageSize must be between 1024 and 262144 bytes",
                        );
                    }
                },
                "DelaySeconds" => match value.parse::<u32>() {
                    Ok(delay) if delay <= 900 => {
                        queue_config.delay_seconds = delay;
                    }
                    _ => {
                        return build_error_response(
                            SqsErrorCode::InvalidParameterValue,
                            "DelaySeconds must be between 0 and 900 seconds",
                        );
                    }
                },
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
                            );
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
                                let target_queue_id =
                                    arn.split(':').next_back().unwrap_or(arn).to_string();

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
                            );
                        }
                    }
                }
                "RedriveAllowPolicy" => {
                    // Parse the redrive allow policy JSON
                    // Format: {"redrivePermission": "allowAll"} or "denyAll" or
                    //         {"redrivePermission": "byQueue", "sourceQueueArns": ["arn:..."]}
                    match serde_json::from_str::<serde_json::Value>(&value) {
                        Ok(policy) => {
                            use crate::types::{RedriveAllowPolicy, RedrivePermission};

                            let permission_str = policy["redrivePermission"].as_str();

                            if let Some(perm) = permission_str {
                                let permission = match perm {
                                    "allowAll" => RedrivePermission::AllowAll,
                                    "denyAll" => RedrivePermission::DenyAll,
                                    "byQueue" => {
                                        // Extract source queue ARNs
                                        let source_arns = policy["sourceQueueArns"]
                                            .as_array()
                                            .map(|arr| {
                                                arr.iter()
                                                    .filter_map(|v| {
                                                        v.as_str().map(|s| s.to_string())
                                                    })
                                                    .collect::<Vec<String>>()
                                            })
                                            .unwrap_or_default();

                                        RedrivePermission::ByQueue {
                                            source_queue_arns: source_arns,
                                        }
                                    }
                                    _ => {
                                        return build_error_response(
                                            SqsErrorCode::InvalidParameterValue,
                                            "Invalid redrivePermission value (must be allowAll, denyAll, or byQueue)",
                                        );
                                    }
                                };

                                queue_config.redrive_allow_policy =
                                    Some(RedriveAllowPolicy { permission });
                            } else {
                                return build_error_response(
                                    SqsErrorCode::InvalidParameterValue,
                                    "Missing redrivePermission in RedriveAllowPolicy",
                                );
                            }
                        }
                        Err(_) => {
                            return build_error_response(
                                SqsErrorCode::InvalidParameterValue,
                                "Invalid RedriveAllowPolicy JSON",
                            );
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

    async fn handle_tag_queue(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
            }
        };

        // Get current queue configuration
        let mut queue_config = match self.backend.get_queue(&queue_name).await {
            Ok(config) => config,
            Err(_) => {
                return build_error_response(
                    SqsErrorCode::QueueDoesNotExist,
                    "The specified queue does not exist",
                );
            }
        };

        // Parse tags from request (Tag.N.Key and Tag.N.Value)
        let mut i = 1;
        loop {
            let key_param = format!("Tag.{}.Key", i);
            let value_param = format!("Tag.{}.Value", i);

            match (
                request.get_param(&key_param),
                request.get_param(&value_param),
            ) {
                (Some(key), Some(value)) => {
                    queue_config.tags.insert(key.to_string(), value.to_string());
                    i += 1;
                }
                _ => break,
            }
        }

        // Update the queue
        match self.backend.update_queue(queue_config).await {
            Ok(_) => {
                info!(queue_name = %queue_name, "Queue tags added");
                build_tag_queue_response()
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    async fn handle_untag_queue(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
            }
        };

        // Get current queue configuration
        let mut queue_config = match self.backend.get_queue(&queue_name).await {
            Ok(config) => config,
            Err(_) => {
                return build_error_response(
                    SqsErrorCode::QueueDoesNotExist,
                    "The specified queue does not exist",
                );
            }
        };

        // Parse tag keys to remove (TagKey.N)
        let mut i = 1;
        loop {
            let key_param = format!("TagKey.{}", i);

            match request.get_param(&key_param) {
                Some(key) => {
                    queue_config.tags.remove(key);
                    i += 1;
                }
                None => break,
            }
        }

        // Update the queue
        match self.backend.update_queue(queue_config).await {
            Ok(_) => {
                info!(queue_name = %queue_name, "Queue tags removed");
                build_untag_queue_response()
            }
            Err(e) => build_error_response(SqsErrorCode::InternalError, &e.to_string()),
        }
    }

    async fn handle_list_queue_tags(&self, request: SqsRequest) -> String {
        let queue_url = match request.get_required_param("QueueUrl") {
            Ok(url) => url,
            Err(e) => return build_error_response(SqsErrorCode::MissingParameter, &e),
        };

        let queue_name = match extract_queue_name_from_url(queue_url) {
            Some(name) => name,
            None => {
                return build_error_response(
                    SqsErrorCode::InvalidParameterValue,
                    "Invalid QueueUrl",
                );
            }
        };

        // Get queue configuration
        let queue_config = match self.backend.get_queue(&queue_name).await {
            Ok(config) => config,
            Err(_) => {
                return build_error_response(
                    SqsErrorCode::QueueDoesNotExist,
                    "The specified queue does not exist",
                );
            }
        };

        // Build response with tags
        let mut response = String::from(
            r#"<?xml version="1.0"?><ListQueueTagsResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/"><ListQueueTagsResult>"#,
        );

        if !queue_config.tags.is_empty() {
            for (key, value) in &queue_config.tags {
                response.push_str(&format!(
                    "<Tag><Key>{}</Key><Value>{}</Value></Tag>",
                    escape_xml(key),
                    escape_xml(value)
                ));
            }
        }

        response.push_str(&format!(
            r#"</ListQueueTagsResult><ResponseMetadata><RequestId>{}</RequestId></ResponseMetadata></ListQueueTagsResponse>"#,
            uuid::Uuid::new_v4()
        ));

        info!(queue_name = %queue_name, tag_count = queue_config.tags.len(), "Queue tags listed");
        response
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
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::VisibilityTimeout,
                    queue_config,
                    stats,
                    false,
                );
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::DelaySeconds,
                    queue_config,
                    stats,
                    false,
                );
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::MaximumMessageSize,
                    queue_config,
                    stats,
                    false,
                );
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::MessageRetentionPeriod,
                    queue_config,
                    stats,
                    false,
                );
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::ApproximateNumberOfMessages,
                    queue_config,
                    stats,
                    false,
                );
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::ApproximateNumberOfMessagesNotVisible,
                    queue_config,
                    stats,
                    false,
                );
                Self::add_queue_attribute(
                    attributes,
                    &QueueAttribute::QueueArn,
                    queue_config,
                    stats,
                    false,
                );

                if queue_config.queue_type == crate::types::QueueType::SqsFifo {
                    Self::add_queue_attribute(
                        attributes,
                        &QueueAttribute::FifoQueue,
                        queue_config,
                        stats,
                        false,
                    );
                    Self::add_queue_attribute(
                        attributes,
                        &QueueAttribute::ContentBasedDeduplication,
                        queue_config,
                        stats,
                        false,
                    );
                }

                if queue_config.dlq_config.is_some() {
                    Self::add_queue_attribute(
                        attributes,
                        &QueueAttribute::RedrivePolicy,
                        queue_config,
                        stats,
                        false,
                    );
                }

                if queue_config.redrive_allow_policy.is_some() {
                    Self::add_queue_attribute(
                        attributes,
                        &QueueAttribute::RedriveAllowPolicy,
                        queue_config,
                        stats,
                        false,
                    );
                }
            }
            QueueAttribute::VisibilityTimeout => {
                attributes.push((
                    "VisibilityTimeout".to_string(),
                    queue_config.visibility_timeout.to_string(),
                ));
            }
            QueueAttribute::DelaySeconds => {
                attributes.push((
                    "DelaySeconds".to_string(),
                    queue_config.delay_seconds.to_string(),
                ));
            }
            QueueAttribute::MaximumMessageSize => {
                attributes.push((
                    "MaximumMessageSize".to_string(),
                    queue_config.max_message_size.to_string(),
                ));
            }
            QueueAttribute::MessageRetentionPeriod => {
                attributes.push((
                    "MessageRetentionPeriod".to_string(),
                    queue_config.message_retention_period.to_string(),
                ));
            }
            QueueAttribute::ApproximateNumberOfMessages => {
                attributes.push((
                    "ApproximateNumberOfMessages".to_string(),
                    stats.available_messages.to_string(),
                ));
            }
            QueueAttribute::ApproximateNumberOfMessagesNotVisible => {
                attributes.push((
                    "ApproximateNumberOfMessagesNotVisible".to_string(),
                    stats.in_flight_messages.to_string(),
                ));
            }
            QueueAttribute::ApproximateNumberOfMessagesDelayed => {
                // TODO: Track delayed messages separately
                attributes.push((
                    "ApproximateNumberOfMessagesDelayed".to_string(),
                    "0".to_string(),
                ));
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
                attributes.push((
                    "ContentBasedDeduplication".to_string(),
                    queue_config.content_based_deduplication.to_string(),
                ));
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
            QueueAttribute::RedriveAllowPolicy => {
                if let Some(policy) = &queue_config.redrive_allow_policy {
                    use crate::types::RedrivePermission;

                    // Format as JSON based on permission type
                    let json = match &policy.permission {
                        RedrivePermission::AllowAll => {
                            r#"{"redrivePermission":"allowAll"}"#.to_string()
                        }
                        RedrivePermission::DenyAll => {
                            r#"{"redrivePermission":"denyAll"}"#.to_string()
                        }
                        RedrivePermission::ByQueue { source_queue_arns } => {
                            // Build sourceQueueArns array
                            let arns_json: Vec<String> = source_queue_arns
                                .iter()
                                .map(|arn| format!(r#""{}""#, arn))
                                .collect();

                            format!(
                                r#"{{"redrivePermission":"byQueue","sourceQueueArns":[{}]}}"#,
                                arns_json.join(",")
                            )
                        }
                    };

                    attributes.push(("RedriveAllowPolicy".to_string(), json));
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

                // Extract Attributes
                let mut attributes = serde_json::Map::new();
                let mut attr_pos = 0;
                while let Some(attr_start) = msg_xml[attr_pos..].find("<Attribute>") {
                    let abs_attr_start = attr_pos + attr_start;
                    if let Some(attr_end_rel) = msg_xml[abs_attr_start..].find("</Attribute>") {
                        let attr_xml = &msg_xml[abs_attr_start..abs_attr_start + attr_end_rel + 12];

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

                        attr_pos = abs_attr_start + attr_end_rel + 12;
                    } else {
                        break;
                    }
                }

                if !attributes.is_empty() {
                    message.insert("Attributes".to_string(), json!(attributes));
                }

                // Extract MessageAttributes
                let mut message_attributes = serde_json::Map::new();
                let mut msg_attr_pos = 0;
                while let Some(msg_attr_start) = msg_xml[msg_attr_pos..].find("<MessageAttribute>")
                {
                    let abs_msg_attr_start = msg_attr_pos + msg_attr_start;
                    if let Some(msg_attr_end_rel) =
                        msg_xml[abs_msg_attr_start..].find("</MessageAttribute>")
                    {
                        let msg_attr_xml = &msg_xml
                            [abs_msg_attr_start..abs_msg_attr_start + msg_attr_end_rel + 19];

                        // Extract Name
                        if let Some(name_start) = msg_attr_xml.find("<Name>") {
                            if let Some(name_end) = msg_attr_xml.find("</Name>") {
                                let name = &msg_attr_xml[name_start + 6..name_end];

                                // Extract Value object
                                let mut value_obj = serde_json::Map::new();

                                // Extract DataType
                                if let Some(dt_start) = msg_attr_xml.find("<DataType>") {
                                    if let Some(dt_end) = msg_attr_xml.find("</DataType>") {
                                        let data_type = &msg_attr_xml[dt_start + 10..dt_end];
                                        value_obj.insert("DataType".to_string(), json!(data_type));
                                    }
                                }

                                // Extract StringValue
                                if let Some(sv_start) = msg_attr_xml.find("<StringValue>") {
                                    if let Some(sv_end) = msg_attr_xml.find("</StringValue>") {
                                        let string_value = &msg_attr_xml[sv_start + 13..sv_end];
                                        value_obj
                                            .insert("StringValue".to_string(), json!(string_value));
                                    }
                                }

                                // Extract BinaryValue
                                if let Some(bv_start) = msg_attr_xml.find("<BinaryValue>") {
                                    if let Some(bv_end) = msg_attr_xml.find("</BinaryValue>") {
                                        let binary_value = &msg_attr_xml[bv_start + 13..bv_end];
                                        value_obj
                                            .insert("BinaryValue".to_string(), json!(binary_value));
                                    }
                                }

                                message_attributes.insert(name.to_string(), json!(value_obj));
                            }
                        }

                        msg_attr_pos = abs_msg_attr_start + msg_attr_end_rel + 19;
                    } else {
                        break;
                    }
                }

                if !message_attributes.is_empty() {
                    message.insert("MessageAttributes".to_string(), json!(message_attributes));
                }

                messages.push(message);
                pos = abs_start + msg_end_rel + 10;
            } else {
                break;
            }
        }

        return json!({"Messages": messages}).to_string();
    }

    // For SendMessageBatch - collect successful and failed entries
    if xml.contains("SendMessageBatchResponse") || xml.contains("SendMessageBatchResult") {
        let mut successful = Vec::new();
        let mut failed = Vec::new();
        let mut pos = 0;

        // Extract successful entries
        while let Some(entry_start) = xml[pos..].find("<SendMessageBatchResultEntry>") {
            let abs_start = pos + entry_start;
            if let Some(entry_end_rel) = xml[abs_start..].find("</SendMessageBatchResultEntry>") {
                let entry_xml = &xml[abs_start..abs_start + entry_end_rel + 30];
                let mut entry = serde_json::Map::new();

                // Extract Id
                if let Some(id_start) = entry_xml.find("<Id>") {
                    if let Some(id_end) = entry_xml.find("</Id>") {
                        let id = &entry_xml[id_start + 4..id_end];
                        entry.insert("Id".to_string(), json!(id));
                    }
                }

                // Extract MessageId
                if let Some(mid_start) = entry_xml.find("<MessageId>") {
                    if let Some(mid_end) = entry_xml.find("</MessageId>") {
                        let message_id = &entry_xml[mid_start + 11..mid_end];
                        entry.insert("MessageId".to_string(), json!(message_id));
                    }
                }

                // Extract MD5OfMessageBody
                if let Some(md5_start) = entry_xml.find("<MD5OfMessageBody>") {
                    if let Some(md5_end) = entry_xml.find("</MD5OfMessageBody>") {
                        let md5 = &entry_xml[md5_start + 18..md5_end];
                        entry.insert("MD5OfMessageBody".to_string(), json!(md5));
                    }
                }

                successful.push(json!(entry));
                pos = abs_start + entry_end_rel + 30;
            } else {
                break;
            }
        }

        // Extract failed entries
        pos = 0;
        while let Some(entry_start) = xml[pos..].find("<BatchResultErrorEntry>") {
            let abs_start = pos + entry_start;
            if let Some(entry_end_rel) = xml[abs_start..].find("</BatchResultErrorEntry>") {
                let entry_xml = &xml[abs_start..abs_start + entry_end_rel + 24];
                let mut entry = serde_json::Map::new();

                // Extract Id
                if let Some(id_start) = entry_xml.find("<Id>") {
                    if let Some(id_end) = entry_xml.find("</Id>") {
                        let id = &entry_xml[id_start + 4..id_end];
                        entry.insert("Id".to_string(), json!(id));
                    }
                }

                // Extract Code
                if let Some(code_start) = entry_xml.find("<Code>") {
                    if let Some(code_end) = entry_xml.find("</Code>") {
                        let code = &entry_xml[code_start + 6..code_end];
                        entry.insert("Code".to_string(), json!(code));
                    }
                }

                // Extract Message
                if let Some(msg_start) = entry_xml.find("<Message>") {
                    if let Some(msg_end) = entry_xml.find("</Message>") {
                        let message = &entry_xml[msg_start + 9..msg_end];
                        entry.insert("Message".to_string(), json!(message));
                    }
                }

                // Extract SenderFault
                if let Some(sf_start) = entry_xml.find("<SenderFault>") {
                    if let Some(sf_end) = entry_xml.find("</SenderFault>") {
                        let sender_fault = &entry_xml[sf_start + 13..sf_end];
                        entry.insert("SenderFault".to_string(), json!(sender_fault == "true"));
                    }
                }

                failed.push(json!(entry));
                pos = abs_start + entry_end_rel + 24;
            } else {
                break;
            }
        }

        let mut result = serde_json::Map::new();
        if !successful.is_empty() {
            result.insert("Successful".to_string(), json!(successful));
        }
        if !failed.is_empty() {
            result.insert("Failed".to_string(), json!(failed));
        }

        return serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
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
                                // Unescape XML entities in the value
                                let unescaped_value = crate::sqs::unescape_xml(value);
                                attributes.insert(name.to_string(), json!(unescaped_value));
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
                        entry.insert(
                            "MessageId".to_string(),
                            json!(&entry_xml[mid_start + 11..mid_end]),
                        );
                    }
                }

                if let Some(md5_start) = entry_xml.find("<MD5OfMessageBody>") {
                    if let Some(md5_end) = entry_xml.find("</MD5OfMessageBody>") {
                        entry.insert(
                            "MD5OfMessageBody".to_string(),
                            json!(&entry_xml[md5_start + 18..md5_end]),
                        );
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
                        entry.insert(
                            "Code".to_string(),
                            json!(&entry_xml[code_start + 6..code_end]),
                        );
                    }
                }

                if let Some(msg_start) = entry_xml.find("<Message>") {
                    if let Some(msg_end) = entry_xml.find("</Message>") {
                        entry.insert(
                            "Message".to_string(),
                            json!(&entry_xml[msg_start + 9..msg_end]),
                        );
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
    if xml.contains("DeleteMessageBatchResponse")
        || xml.contains("DeleteMessageBatchResult")
        || xml.contains("ChangeMessageVisibilityBatchResponse")
        || xml.contains("ChangeMessageVisibilityBatchResult")
    {
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
                        entry.insert(
                            "Code".to_string(),
                            json!(&entry_xml[code_start + 6..code_end]),
                        );
                    }
                }

                if let Some(msg_start) = entry_xml.find("<Message>") {
                    if let Some(msg_end) = entry_xml.find("</Message>") {
                        entry.insert(
                            "Message".to_string(),
                            json!(&entry_xml[msg_start + 9..msg_end]),
                        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LclqConfig;
    use crate::sqs::SqsRequest;
    use crate::storage::memory::InMemoryBackend;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Create a test SQS handler with in-memory backend.
    fn create_test_handler() -> SqsHandler {
        let backend = Arc::new(InMemoryBackend::new()) as Arc<dyn StorageBackend>;
        let config = LclqConfig::default();
        SqsHandler::new(backend, config)
    }

    /// Helper to create a queue for testing.
    async fn create_test_queue(handler: &SqsHandler, queue_name: &str) {
        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), queue_name.to_string());

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        handler.handle_request(request).await;
    }

    // ========================================================================
    // TagQueue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_tag_queue_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-queue").await;

        // Tag the queue
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue".to_string(),
        );
        params.insert("Tag.1.Key".to_string(), "Environment".to_string());
        params.insert("Tag.1.Value".to_string(), "Test".to_string());
        params.insert("Tag.2.Key".to_string(), "Owner".to_string());
        params.insert("Tag.2.Value".to_string(), "Alice".to_string());

        let request = SqsRequest {
            action: SqsAction::TagQueue,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("TagQueueResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_tag_queue_missing_queue_url() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("Tag.1.Key".to_string(), "Environment".to_string());
        params.insert("Tag.1.Value".to_string(), "Test".to_string());

        let request = SqsRequest {
            action: SqsAction::TagQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    #[tokio::test]
    async fn test_tag_queue_nonexistent_queue() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/nonexistent".to_string(),
        );
        params.insert("Tag.1.Key".to_string(), "Environment".to_string());
        params.insert("Tag.1.Value".to_string(), "Test".to_string());

        let request = SqsRequest {
            action: SqsAction::TagQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("<Error>"));
    }

    // ========================================================================
    // UntagQueue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_untag_queue_success() {
        let handler = create_test_handler();

        // Create and tag a queue
        create_test_queue(&handler, "test-queue-untag").await;

        let mut tag_params = HashMap::new();
        tag_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-untag".to_string(),
        );
        tag_params.insert("Tag.1.Key".to_string(), "Environment".to_string());
        tag_params.insert("Tag.1.Value".to_string(), "Test".to_string());

        let tag_request = SqsRequest {
            action: SqsAction::TagQueue,
            params: tag_params,
            is_json: false,
        };

        handler.handle_request(tag_request).await;

        // Now untag
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-untag".to_string(),
        );
        params.insert("TagKey.1".to_string(), "Environment".to_string());

        let request = SqsRequest {
            action: SqsAction::UntagQueue,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("UntagQueueResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_untag_queue_missing_queue_url() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("TagKey.1".to_string(), "Environment".to_string());

        let request = SqsRequest {
            action: SqsAction::UntagQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    // ========================================================================
    // ListQueueTags Tests
    // ========================================================================

    #[tokio::test]
    async fn test_list_queue_tags_success() {
        let handler = create_test_handler();

        // Create and tag a queue
        create_test_queue(&handler, "test-queue-list-tags").await;

        let mut tag_params = HashMap::new();
        tag_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-list-tags".to_string(),
        );
        tag_params.insert("Tag.1.Key".to_string(), "Environment".to_string());
        tag_params.insert("Tag.1.Value".to_string(), "Production".to_string());
        tag_params.insert("Tag.2.Key".to_string(), "Team".to_string());
        tag_params.insert("Tag.2.Value".to_string(), "Backend".to_string());

        let tag_request = SqsRequest {
            action: SqsAction::TagQueue,
            params: tag_params,
            is_json: false,
        };

        handler.handle_request(tag_request).await;

        // List tags
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-list-tags".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::ListQueueTags,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("ListQueueTagsResponse"));
        assert!(response.contains("Environment"));
        assert!(response.contains("Production"));
        assert!(response.contains("Team"));
        assert!(response.contains("Backend"));
    }

    #[tokio::test]
    async fn test_list_queue_tags_empty() {
        let handler = create_test_handler();

        // Create a queue without tags
        create_test_queue(&handler, "test-queue-no-tags").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-no-tags".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::ListQueueTags,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("ListQueueTagsResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_list_queue_tags_missing_queue_url() {
        let handler = create_test_handler();

        let params = HashMap::new();

        let request = SqsRequest {
            action: SqsAction::ListQueueTags,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    // ========================================================================
    // ChangeMessageVisibility Tests
    // ========================================================================

    #[tokio::test]
    async fn test_change_message_visibility_success() {
        let handler = create_test_handler();

        // Create queue and send a message
        create_test_queue(&handler, "test-queue-visibility").await;

        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-visibility".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "Test message".to_string());

        let send_request = SqsRequest {
            action: SqsAction::SendMessage,
            params: send_params,
            is_json: false,
        };

        handler.handle_request(send_request).await;

        // Receive the message to get a receipt handle
        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-visibility".to_string(),
        );
        receive_params.insert("MaxNumberOfMessages".to_string(), "1".to_string());

        let receive_request = SqsRequest {
            action: SqsAction::ReceiveMessage,
            params: receive_params,
            is_json: false,
        };

        let (receive_response, _) = handler.handle_request(receive_request).await;

        // Extract receipt handle from response
        if let Some(start) = receive_response.find("<ReceiptHandle>") {
            if let Some(end) = receive_response.find("</ReceiptHandle>") {
                let receipt_handle = &receive_response[start + 15..end];

                // Change visibility timeout
                let mut params = HashMap::new();
                params.insert(
                    "QueueUrl".to_string(),
                    "http://127.0.0.1:9324/queue/test-queue-visibility".to_string(),
                );
                params.insert("ReceiptHandle".to_string(), receipt_handle.to_string());
                params.insert("VisibilityTimeout".to_string(), "60".to_string());

                let request = SqsRequest {
                    action: SqsAction::ChangeMessageVisibility,
                    params,
                    is_json: false,
                };

                let (response, content_type) = handler.handle_request(request).await;

                assert_eq!(content_type, "application/xml");
                assert!(response.contains("ChangeMessageVisibilityResponse"));
                assert!(!response.contains("<Error>"));
            }
        }
    }

    #[tokio::test]
    async fn test_change_message_visibility_missing_receipt_handle() {
        let handler = create_test_handler();

        create_test_queue(&handler, "test-queue-vis-error").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-vis-error".to_string(),
        );
        params.insert("VisibilityTimeout".to_string(), "60".to_string());

        let request = SqsRequest {
            action: SqsAction::ChangeMessageVisibility,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    // ========================================================================
    // ChangeMessageVisibilityBatch Tests
    // ========================================================================

    #[tokio::test]
    async fn test_change_message_visibility_batch_success() {
        let handler = create_test_handler();

        // Create queue and send messages
        create_test_queue(&handler, "test-queue-batch-vis").await;

        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-batch-vis".to_string(),
        );
        send_params.insert(
            "SendMessageBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        send_params.insert(
            "SendMessageBatchRequestEntry.1.MessageBody".to_string(),
            "Message 1".to_string(),
        );
        send_params.insert(
            "SendMessageBatchRequestEntry.2.Id".to_string(),
            "msg2".to_string(),
        );
        send_params.insert(
            "SendMessageBatchRequestEntry.2.MessageBody".to_string(),
            "Message 2".to_string(),
        );

        let send_request = SqsRequest {
            action: SqsAction::SendMessageBatch,
            params: send_params,
            is_json: false,
        };

        handler.handle_request(send_request).await;

        // Receive messages
        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue-batch-vis".to_string(),
        );
        receive_params.insert("MaxNumberOfMessages".to_string(), "10".to_string());

        let receive_request = SqsRequest {
            action: SqsAction::ReceiveMessage,
            params: receive_params,
            is_json: false,
        };

        let (receive_response, _) = handler.handle_request(receive_request).await;

        // Extract receipt handles
        let mut receipt_handles = Vec::new();
        let mut pos = 0;
        while let Some(start) = receive_response[pos..].find("<ReceiptHandle>") {
            let abs_start = pos + start;
            if let Some(end_rel) = receive_response[abs_start..].find("</ReceiptHandle>") {
                let receipt_handle = &receive_response[abs_start + 15..abs_start + end_rel];
                receipt_handles.push(receipt_handle.to_string());
                pos = abs_start + end_rel + 16;
            } else {
                break;
            }
        }

        if receipt_handles.len() >= 2 {
            // Change visibility batch
            let mut params = HashMap::new();
            params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-queue-batch-vis".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.1.Id".to_string(),
                "entry1".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.1.ReceiptHandle".to_string(),
                receipt_handles[0].clone(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.1.VisibilityTimeout".to_string(),
                "30".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.2.Id".to_string(),
                "entry2".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.2.ReceiptHandle".to_string(),
                receipt_handles[1].clone(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.2.VisibilityTimeout".to_string(),
                "60".to_string(),
            );

            let request = SqsRequest {
                action: SqsAction::ChangeMessageVisibilityBatch,
                params,
                is_json: false,
            };

            let (response, content_type) = handler.handle_request(request).await;

            assert_eq!(content_type, "application/xml");
            assert!(response.contains("ChangeMessageVisibilityBatchResponse"));
            assert!(!response.contains("<Error>"));
        }
    }

    #[tokio::test]
    async fn test_change_message_visibility_batch_missing_queue_url() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.Id".to_string(),
            "entry1".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.ReceiptHandle".to_string(),
            "dummy-handle".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.VisibilityTimeout".to_string(),
            "30".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::ChangeMessageVisibilityBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;

        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    // ========================================================================
    // Core SQS Operation Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_queue_standard() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "test-standard-queue".to_string());

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("CreateQueueResponse"));
        assert!(response.contains("QueueUrl"));
        assert!(response.contains("test-standard-queue"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_create_queue_fifo() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "test-fifo-queue.fifo".to_string());
        params.insert("Attribute.FifoQueue".to_string(), "true".to_string());

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("CreateQueueResponse"));
        assert!(response.contains("test-fifo-queue.fifo"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_get_queue_url_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-geturl-queue").await;

        // Get the queue URL
        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "test-geturl-queue".to_string());

        let request = SqsRequest {
            action: SqsAction::GetQueueUrl,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("GetQueueUrlResponse"));
        assert!(response.contains("QueueUrl"));
        assert!(response.contains("test-geturl-queue"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_delete_queue_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-delete-queue").await;

        // Delete the queue
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-delete-queue".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::DeleteQueue,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        // Response may be empty or contain DeleteQueueResponse - both are valid
        assert!(response.contains("DeleteQueueResponse") || !response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_list_queues_success() {
        let handler = create_test_handler();

        // Create a few queues
        create_test_queue(&handler, "test-list-queue-1").await;
        create_test_queue(&handler, "test-list-queue-2").await;

        // List queues
        let params = HashMap::new();

        let request = SqsRequest {
            action: SqsAction::ListQueues,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("ListQueuesResponse"));
        assert!(response.contains("test-list-queue-1"));
        assert!(response.contains("test-list-queue-2"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_send_message_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-send-queue").await;

        // Send a message
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-send-queue".to_string(),
        );
        params.insert("MessageBody".to_string(), "Hello, SQS!".to_string());

        let request = SqsRequest {
            action: SqsAction::SendMessage,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("SendMessageResponse"));
        assert!(response.contains("MessageId"));
        assert!(response.contains("MD5OfMessageBody"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_send_message_batch_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-send-batch-queue").await;

        // Send batch messages
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-send-batch-queue".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.1.MessageBody".to_string(),
            "Message 1".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.2.Id".to_string(),
            "msg2".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.2.MessageBody".to_string(),
            "Message 2".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SendMessageBatch,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("SendMessageBatchResponse"));
        assert!(response.contains("SendMessageBatchResultEntry"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_receive_message_success() {
        let handler = create_test_handler();

        // Create a queue and send a message
        create_test_queue(&handler, "test-receive-queue").await;

        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-receive-queue".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "Test message".to_string());

        handler
            .handle_request(SqsRequest {
                action: SqsAction::SendMessage,
                params: send_params,
                is_json: false,
            })
            .await;

        // Receive the message
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-receive-queue".to_string(),
        );
        params.insert("MaxNumberOfMessages".to_string(), "1".to_string());

        let request = SqsRequest {
            action: SqsAction::ReceiveMessage,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("ReceiveMessageResponse"));
        assert!(response.contains("Test message"));
        assert!(response.contains("ReceiptHandle"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_delete_message_success() {
        let handler = create_test_handler();

        // Create queue, send and receive message
        create_test_queue(&handler, "test-delete-msg-queue").await;

        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-delete-msg-queue".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "Delete me".to_string());

        handler
            .handle_request(SqsRequest {
                action: SqsAction::SendMessage,
                params: send_params,
                is_json: false,
            })
            .await;

        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-delete-msg-queue".to_string(),
        );

        let (receive_response, _) = handler
            .handle_request(SqsRequest {
                action: SqsAction::ReceiveMessage,
                params: receive_params,
                is_json: false,
            })
            .await;

        // Extract receipt handle (simple parsing)
        let receipt_start = receive_response.find("<ReceiptHandle>").unwrap() + 15;
        let receipt_end = receive_response.find("</ReceiptHandle>").unwrap();
        let receipt_handle = &receive_response[receipt_start..receipt_end];

        // Delete the message
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-delete-msg-queue".to_string(),
        );
        params.insert("ReceiptHandle".to_string(), receipt_handle.to_string());

        let request = SqsRequest {
            action: SqsAction::DeleteMessage,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("DeleteMessageResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_purge_queue_success() {
        let handler = create_test_handler();

        // Create queue and send messages
        create_test_queue(&handler, "test-purge-queue").await;

        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-purge-queue".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "Message to purge".to_string());

        handler
            .handle_request(SqsRequest {
                action: SqsAction::SendMessage,
                params: send_params,
                is_json: false,
            })
            .await;

        // Purge the queue
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-purge-queue".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::PurgeQueue,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        // Response may be empty or contain PurgeQueueResponse - both are valid
        assert!(response.contains("PurgeQueueResponse") || !response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_get_queue_attributes_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-getattrs-queue").await;

        // Get queue attributes
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-getattrs-queue".to_string(),
        );
        params.insert("AttributeName.1".to_string(), "All".to_string());

        let request = SqsRequest {
            action: SqsAction::GetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        assert!(response.contains("GetQueueAttributesResponse"));
        assert!(response.contains("Attribute"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_success() {
        let handler = create_test_handler();

        // Create a queue first
        create_test_queue(&handler, "test-setattrs-queue").await;

        // Set queue attributes
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-setattrs-queue".to_string(),
        );
        params.insert("Attribute.VisibilityTimeout".to_string(), "60".to_string());

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, content_type) = handler.handle_request(request).await;

        assert_eq!(content_type, "application/xml");
        // Response may be empty or contain SetQueueAttributesResponse - both are valid
        assert!(response.contains("SetQueueAttributesResponse") || !response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_delete_message_batch_success() {
        let handler = create_test_handler();

        // Create queue and send messages
        create_test_queue(&handler, "test-delete-batch-queue").await;

        for i in 1..=2 {
            let mut send_params = HashMap::new();
            send_params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-delete-batch-queue".to_string(),
            );
            send_params.insert("MessageBody".to_string(), format!("Message {}", i));

            handler
                .handle_request(SqsRequest {
                    action: SqsAction::SendMessage,
                    params: send_params,
                    is_json: false,
                })
                .await;
        }

        // Receive messages
        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-delete-batch-queue".to_string(),
        );
        receive_params.insert("MaxNumberOfMessages".to_string(), "10".to_string());

        let (receive_response, _) = handler
            .handle_request(SqsRequest {
                action: SqsAction::ReceiveMessage,
                params: receive_params,
                is_json: false,
            })
            .await;

        // Extract receipt handles (simple parsing - get first two)
        let receipt_handles: Vec<String> = receive_response
            .match_indices("<ReceiptHandle>")
            .take(2)
            .map(|(start, _)| {
                let content_start = start + 15;
                let content_end = receive_response[content_start..]
                    .find("</ReceiptHandle>")
                    .unwrap()
                    + content_start;
                receive_response[content_start..content_end].to_string()
            })
            .collect();

        if receipt_handles.len() >= 2 {
            // Delete messages in batch
            let mut params = HashMap::new();
            params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-delete-batch-queue".to_string(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.1.Id".to_string(),
                "entry1".to_string(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.1.ReceiptHandle".to_string(),
                receipt_handles[0].clone(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.2.Id".to_string(),
                "entry2".to_string(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.2.ReceiptHandle".to_string(),
                receipt_handles[1].clone(),
            );

            let request = SqsRequest {
                action: SqsAction::DeleteMessageBatch,
                params,
                is_json: false,
            };

            let (response, content_type) = handler.handle_request(request).await;

            assert_eq!(content_type, "application/xml");
            assert!(response.contains("DeleteMessageBatchResponse"));
            assert!(!response.contains("<Error>"));
        }
    }

    // ========================================================================
    // Advanced CreateQueue Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_queue_with_redrive_policy() {
        let handler = create_test_handler();

        // Create DLQ first
        create_test_queue(&handler, "dlq-queue").await;

        // Create queue with redrive policy
        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "queue-with-dlq".to_string());
        params.insert("Attribute.RedrivePolicy".to_string(),
            r#"{"deadLetterTargetArn":"arn:aws:sqs:us-east-1:000000000000:dlq-queue","maxReceiveCount":"5"}"#.to_string());

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("CreateQueueResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_create_queue_with_redrive_allow_policy() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "queue-with-rap".to_string());
        params.insert(
            "Attribute.RedriveAllowPolicy".to_string(),
            r#"{"redrivePermission":"allowAll"}"#.to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("CreateQueueResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_create_queue_missing_name() {
        let handler = create_test_handler();

        let params = HashMap::new();

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    // ========================================================================
    // GetQueueAttributes Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_queue_attributes_specific() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-attrs-queue").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-attrs-queue".to_string(),
        );
        params.insert(
            "AttributeName.1".to_string(),
            "VisibilityTimeout".to_string(),
        );
        params.insert("AttributeName.2".to_string(), "DelaySeconds".to_string());

        let request = SqsRequest {
            action: SqsAction::GetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("GetQueueAttributesResponse"));
        assert!(response.contains("VisibilityTimeout"));
    }

    #[tokio::test]
    async fn test_get_queue_attributes_missing_queue_url() {
        let handler = create_test_handler();

        let params = HashMap::new();

        let request = SqsRequest {
            action: SqsAction::GetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    // ========================================================================
    // SetQueueAttributes Tests
    // ========================================================================

    #[tokio::test]
    async fn test_set_queue_attributes_message_retention() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-set-retention").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-set-retention".to_string(),
        );
        params.insert(
            "Attribute.MessageRetentionPeriod".to_string(),
            "86400".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_invalid_visibility_timeout() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-invalid-vis").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-invalid-vis".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "VisibilityTimeout".to_string(),
        );
        params.insert("Attribute.1.Value".to_string(), "99999".to_string());

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("InvalidParameterValue"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_invalid_retention() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-invalid-retention").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-invalid-retention".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "MessageRetentionPeriod".to_string(),
        );
        params.insert("Attribute.1.Value".to_string(), "30".to_string()); // Too short

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_content_based_dedup_non_fifo() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-standard-queue").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-standard-queue".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "ContentBasedDeduplication".to_string(),
        );
        params.insert("Attribute.1.Value".to_string(), "true".to_string());

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("only valid for FIFO"));
    }

    // ========================================================================
    // Error Case Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_queue_url_not_found() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "nonexistent-queue".to_string());

        let request = SqsRequest {
            action: SqsAction::GetQueueUrl,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("NonExistentQueue"));
    }

    #[tokio::test]
    async fn test_send_message_missing_body() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-queue").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-queue".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SendMessage,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    #[tokio::test]
    async fn test_send_message_batch_empty() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-batch-empty").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-batch-empty".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SendMessageBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("No batch entries"));
    }

    #[tokio::test]
    async fn test_delete_message_missing_receipt() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-delete-missing").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-delete-missing".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::DeleteMessage,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    #[tokio::test]
    async fn test_change_visibility_missing_timeout() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-change-vis").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-change-vis".to_string(),
        );
        params.insert("ReceiptHandle".to_string(), "dummy-handle".to_string());

        let request = SqsRequest {
            action: SqsAction::ChangeMessageVisibility,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("MissingParameter"));
    }

    #[tokio::test]
    async fn test_change_visibility_batch_empty() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-change-batch-empty").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-change-batch-empty".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::ChangeMessageVisibilityBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("No batch entries"));
    }

    // ========================================================================
    // JSON Protocol Tests (xml_to_json_response coverage)
    // ========================================================================

    #[tokio::test]
    async fn test_create_queue_json_response() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "test-json-queue".to_string());

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        assert!(response.contains("QueueUrl"));
    }

    #[tokio::test]
    async fn test_send_message_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-json-send").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-send".to_string(),
        );
        params.insert("MessageBody".to_string(), "JSON test message".to_string());

        let request = SqsRequest {
            action: SqsAction::SendMessage,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        assert!(response.contains("MessageId"));
        assert!(response.contains("MD5OfMessageBody"));
    }

    #[tokio::test]
    async fn test_receive_message_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-json-receive").await;

        // Send a message first
        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-receive".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "JSON receive test".to_string());
        handler
            .handle_request(SqsRequest {
                action: SqsAction::SendMessage,
                params: send_params,
                is_json: false,
            })
            .await;

        // Receive with JSON response
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-receive".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::ReceiveMessage,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        assert!(response.contains("Messages"));
    }

    #[tokio::test]
    async fn test_list_queues_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-json-list-1").await;
        create_test_queue(&handler, "test-json-list-2").await;

        let params = HashMap::new();

        let request = SqsRequest {
            action: SqsAction::ListQueues,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        assert!(response.contains("QueueUrls"));
    }

    #[tokio::test]
    async fn test_get_queue_attributes_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-json-getattrs").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-getattrs".to_string(),
        );
        params.insert("AttributeName.1".to_string(), "All".to_string());

        let request = SqsRequest {
            action: SqsAction::GetQueueAttributes,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        assert!(response.contains("Attributes"));
    }

    #[tokio::test]
    async fn test_delete_message_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-json-delete").await;

        // Send and receive a message to get receipt handle
        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-delete".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "Delete me".to_string());
        handler
            .handle_request(SqsRequest {
                action: SqsAction::SendMessage,
                params: send_params,
                is_json: false,
            })
            .await;

        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-delete".to_string(),
        );
        let (receive_response, _) = handler
            .handle_request(SqsRequest {
                action: SqsAction::ReceiveMessage,
                params: receive_params,
                is_json: false,
            })
            .await;

        let receipt_start = receive_response.find("<ReceiptHandle>").unwrap() + 15;
        let receipt_end = receive_response.find("</ReceiptHandle>").unwrap();
        let receipt_handle = &receive_response[receipt_start..receipt_end];

        // Delete with JSON response
        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-delete".to_string(),
        );
        params.insert("ReceiptHandle".to_string(), receipt_handle.to_string());

        let request = SqsRequest {
            action: SqsAction::DeleteMessage,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        // DeleteMessage returns empty JSON
        assert_eq!(response, "{}");
    }

    #[tokio::test]
    async fn test_send_message_batch_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-json-batch").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-json-batch".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.1.MessageBody".to_string(),
            "Message 1".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.2.Id".to_string(),
            "msg2".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.2.MessageBody".to_string(),
            "Message 2".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SendMessageBatch,
            params,
            is_json: true,
        };

        let (response, content_type) = handler.handle_request(request).await;
        assert_eq!(content_type, "application/x-amz-json-1.0");
        assert!(response.contains("Successful"));
    }

    // ========================================================================
    // SetQueueAttributes Error Path Tests
    // ========================================================================

    #[tokio::test]
    async fn test_set_queue_attributes_invalid_redrive_policy_json() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-invalid-redrive-json").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-invalid-redrive-json".to_string(),
        );
        params.insert("Attribute.1.Name".to_string(), "RedrivePolicy".to_string());
        params.insert(
            "Attribute.1.Value".to_string(),
            "invalid json here".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("Invalid RedrivePolicy JSON"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_invalid_redrive_policy_format() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-invalid-redrive-format").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-invalid-redrive-format".to_string(),
        );
        params.insert("Attribute.1.Name".to_string(), "RedrivePolicy".to_string());
        // Missing deadLetterTargetArn field
        params.insert(
            "Attribute.1.Value".to_string(),
            r#"{"maxReceiveCount":"5"}"#.to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("Invalid RedrivePolicy format"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_invalid_redrive_allow_policy_json() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-invalid-rap-json").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-invalid-rap-json".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "RedriveAllowPolicy".to_string(),
        );
        params.insert(
            "Attribute.1.Value".to_string(),
            "not valid json".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("Invalid RedriveAllowPolicy JSON"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_invalid_redrive_permission_value() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-invalid-rap-perm").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-invalid-rap-perm".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "RedriveAllowPolicy".to_string(),
        );
        params.insert(
            "Attribute.1.Value".to_string(),
            r#"{"redrivePermission":"invalidValue"}"#.to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("Invalid redrivePermission value"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_missing_redrive_permission() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-missing-rap-perm").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-missing-rap-perm".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "RedriveAllowPolicy".to_string(),
        );
        params.insert(
            "Attribute.1.Value".to_string(),
            r#"{"somethingElse":"value"}"#.to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("<Error>"));
        assert!(response.contains("Missing redrivePermission"));
    }

    // ========================================================================
    // SendMessageBatch Error Path Tests
    // ========================================================================

    #[tokio::test]
    async fn test_send_message_batch_message_too_large() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-batch-too-large").await;

        let large_body = "x".repeat(300_000); // Exceeds SQS_MAX_MESSAGE_SIZE

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-batch-too-large".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        params.insert(
            "SendMessageBatchRequestEntry.1.MessageBody".to_string(),
            large_body,
        );

        let request = SqsRequest {
            action: SqsAction::SendMessageBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        // Should return a batch response with a failed entry
        assert!(response.contains("SendMessageBatchResponse"));
        assert!(response.contains("BatchResultErrorEntry"));
        assert!(response.contains("MessageTooLong"));
    }

    // ========================================================================
    // CreateQueue RedriveAllowPolicy Error Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_queue_invalid_redrive_allow_policy_json() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert(
            "QueueName".to_string(),
            "test-create-invalid-rap".to_string(),
        );
        params.insert(
            "Attribute.RedriveAllowPolicy".to_string(),
            "not valid json".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        // Invalid JSON should be silently ignored during create (AWS behavior)
        // The queue is created successfully
        assert!(response.contains("CreateQueueResponse"));
    }

    #[tokio::test]
    async fn test_create_queue_redrive_allow_policy_deny_all() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert("QueueName".to_string(), "test-create-rap-deny".to_string());
        params.insert(
            "Attribute.RedriveAllowPolicy".to_string(),
            r#"{"redrivePermission":"denyAll"}"#.to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("CreateQueueResponse"));
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_create_queue_redrive_allow_policy_by_queue() {
        let handler = create_test_handler();

        let mut params = HashMap::new();
        params.insert(
            "QueueName".to_string(),
            "test-create-rap-byqueue".to_string(),
        );
        params.insert("Attribute.RedriveAllowPolicy".to_string(),
            r#"{"redrivePermission":"byQueue","sourceQueueArns":["arn:aws:sqs:us-east-1:123456789012:source-queue"]}"#.to_string());

        let request = SqsRequest {
            action: SqsAction::CreateQueue,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("CreateQueueResponse"));
        assert!(!response.contains("<Error>"));
    }

    // ========================================================================
    // Message Attributes Tests
    // ========================================================================

    #[tokio::test]
    async fn test_send_message_with_binary_attribute() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-binary-attr").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-binary-attr".to_string(),
        );
        params.insert(
            "MessageBody".to_string(),
            "Message with binary attr".to_string(),
        );
        params.insert(
            "MessageAttribute.1.Name".to_string(),
            "BinaryData".to_string(),
        );
        params.insert(
            "MessageAttribute.1.Value.DataType".to_string(),
            "Binary".to_string(),
        );
        params.insert(
            "MessageAttribute.1.Value.BinaryValue".to_string(),
            "aGVsbG8=".to_string(),
        ); // base64 "hello"

        let request = SqsRequest {
            action: SqsAction::SendMessage,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("SendMessageResponse"));
        assert!(response.contains("MessageId"));
    }

    #[tokio::test]
    async fn test_receive_message_with_message_attributes() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-receive-attrs").await;

        // Send a message with attributes
        let mut send_params = HashMap::new();
        send_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-receive-attrs".to_string(),
        );
        send_params.insert("MessageBody".to_string(), "Test message".to_string());
        send_params.insert(
            "MessageAttribute.1.Name".to_string(),
            "TestAttr".to_string(),
        );
        send_params.insert(
            "MessageAttribute.1.Value.DataType".to_string(),
            "String".to_string(),
        );
        send_params.insert(
            "MessageAttribute.1.Value.StringValue".to_string(),
            "TestValue".to_string(),
        );

        handler
            .handle_request(SqsRequest {
                action: SqsAction::SendMessage,
                params: send_params,
                is_json: false,
            })
            .await;

        // Receive the message
        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-receive-attrs".to_string(),
        );
        receive_params.insert("MaxNumberOfMessages".to_string(), "1".to_string());

        let request = SqsRequest {
            action: SqsAction::ReceiveMessage,
            params: receive_params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("ReceiveMessageResponse"));
        assert!(response.contains("MessageAttribute"));
        assert!(response.contains("TestAttr"));
        assert!(response.contains("TestValue"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_redrive_allow_policy_by_queue() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-set-rap-byqueue").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-set-rap-byqueue".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "RedriveAllowPolicy".to_string(),
        );
        params.insert("Attribute.1.Value".to_string(),
            r#"{"redrivePermission":"byQueue","sourceQueueArns":["arn:aws:sqs:us-east-1:123456789012:source1","arn:aws:sqs:us-east-1:123456789012:source2"]}"#.to_string());

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(!response.contains("<Error>"));
    }

    #[tokio::test]
    async fn test_set_queue_attributes_redrive_allow_policy_deny_all() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-set-rap-deny").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-set-rap-deny".to_string(),
        );
        params.insert(
            "Attribute.1.Name".to_string(),
            "RedriveAllowPolicy".to_string(),
        );
        params.insert(
            "Attribute.1.Value".to_string(),
            r#"{"redrivePermission":"denyAll"}"#.to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::SetQueueAttributes,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(!response.contains("<Error>"));
    }

    // ========================================================================
    // Batch Operation Error Path Tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_message_batch_invalid_receipt() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-del-batch-invalid").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-del-batch-invalid".to_string(),
        );
        params.insert(
            "DeleteMessageBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        params.insert(
            "DeleteMessageBatchRequestEntry.1.ReceiptHandle".to_string(),
            "invalid-receipt-handle".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::DeleteMessageBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("DeleteMessageBatchResponse"));
        assert!(response.contains("BatchResultErrorEntry"));
        assert!(response.contains("ReceiptHandleIsInvalid"));
    }

    #[tokio::test]
    async fn test_change_visibility_batch_invalid_timeout() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-vis-batch-invalid").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-vis-batch-invalid".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.ReceiptHandle".to_string(),
            "dummy-handle".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.VisibilityTimeout".to_string(),
            "99999".to_string(),
        ); // Too large

        let request = SqsRequest {
            action: SqsAction::ChangeMessageVisibilityBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("ChangeMessageVisibilityBatchResponse"));
        assert!(response.contains("BatchResultErrorEntry"));
        assert!(response.contains("InvalidParameterValue"));
    }

    #[tokio::test]
    async fn test_change_visibility_batch_invalid_receipt() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-vis-batch-invalid-rec").await;

        let mut params = HashMap::new();
        params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-vis-batch-invalid-rec".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.Id".to_string(),
            "msg1".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.ReceiptHandle".to_string(),
            "invalid-receipt".to_string(),
        );
        params.insert(
            "ChangeMessageVisibilityBatchRequestEntry.1.VisibilityTimeout".to_string(),
            "30".to_string(),
        );

        let request = SqsRequest {
            action: SqsAction::ChangeMessageVisibilityBatch,
            params,
            is_json: false,
        };

        let (response, _) = handler.handle_request(request).await;
        assert!(response.contains("ChangeMessageVisibilityBatchResponse"));
        assert!(response.contains("BatchResultErrorEntry"));
        assert!(response.contains("ReceiptHandleIsInvalid"));
    }

    // ========================================================================
    // JSON Response Format Tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_message_batch_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-del-batch-json").await;

        // Send and receive messages
        for i in 1..=2 {
            let mut send_params = HashMap::new();
            send_params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-del-batch-json".to_string(),
            );
            send_params.insert("MessageBody".to_string(), format!("Message {}", i));
            handler
                .handle_request(SqsRequest {
                    action: SqsAction::SendMessage,
                    params: send_params,
                    is_json: false,
                })
                .await;
        }

        // Receive messages
        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-del-batch-json".to_string(),
        );
        receive_params.insert("MaxNumberOfMessages".to_string(), "10".to_string());

        let (receive_response, _) = handler
            .handle_request(SqsRequest {
                action: SqsAction::ReceiveMessage,
                params: receive_params,
                is_json: false,
            })
            .await;

        // Extract receipt handles
        let receipt_handles: Vec<String> = receive_response
            .match_indices("<ReceiptHandle>")
            .take(2)
            .map(|(start, _)| {
                let content_start = start + 15;
                let content_end = receive_response[content_start..]
                    .find("</ReceiptHandle>")
                    .unwrap()
                    + content_start;
                receive_response[content_start..content_end].to_string()
            })
            .collect();

        if receipt_handles.len() >= 2 {
            // Delete in batch with JSON response
            let mut params = HashMap::new();
            params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-del-batch-json".to_string(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.1.Id".to_string(),
                "entry1".to_string(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.1.ReceiptHandle".to_string(),
                receipt_handles[0].clone(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.2.Id".to_string(),
                "entry2".to_string(),
            );
            params.insert(
                "DeleteMessageBatchRequestEntry.2.ReceiptHandle".to_string(),
                receipt_handles[1].clone(),
            );

            let request = SqsRequest {
                action: SqsAction::DeleteMessageBatch,
                params,
                is_json: true,
            };

            let (response, content_type) = handler.handle_request(request).await;
            assert_eq!(content_type, "application/x-amz-json-1.0");
            assert!(response.contains("Successful"));
        }
    }

    #[tokio::test]
    async fn test_change_visibility_batch_json_response() {
        let handler = create_test_handler();
        create_test_queue(&handler, "test-vis-batch-json").await;

        // Send and receive messages
        for i in 1..=2 {
            let mut send_params = HashMap::new();
            send_params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-vis-batch-json".to_string(),
            );
            send_params.insert("MessageBody".to_string(), format!("Message {}", i));
            handler
                .handle_request(SqsRequest {
                    action: SqsAction::SendMessage,
                    params: send_params,
                    is_json: false,
                })
                .await;
        }

        // Receive messages
        let mut receive_params = HashMap::new();
        receive_params.insert(
            "QueueUrl".to_string(),
            "http://127.0.0.1:9324/queue/test-vis-batch-json".to_string(),
        );
        receive_params.insert("MaxNumberOfMessages".to_string(), "10".to_string());

        let (receive_response, _) = handler
            .handle_request(SqsRequest {
                action: SqsAction::ReceiveMessage,
                params: receive_params,
                is_json: false,
            })
            .await;

        // Extract receipt handles
        let receipt_handles: Vec<String> = receive_response
            .match_indices("<ReceiptHandle>")
            .take(2)
            .map(|(start, _)| {
                let content_start = start + 15;
                let content_end = receive_response[content_start..]
                    .find("</ReceiptHandle>")
                    .unwrap()
                    + content_start;
                receive_response[content_start..content_end].to_string()
            })
            .collect();

        if receipt_handles.len() >= 2 {
            // Change visibility in batch with JSON response
            let mut params = HashMap::new();
            params.insert(
                "QueueUrl".to_string(),
                "http://127.0.0.1:9324/queue/test-vis-batch-json".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.1.Id".to_string(),
                "entry1".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.1.ReceiptHandle".to_string(),
                receipt_handles[0].clone(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.1.VisibilityTimeout".to_string(),
                "30".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.2.Id".to_string(),
                "entry2".to_string(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.2.ReceiptHandle".to_string(),
                receipt_handles[1].clone(),
            );
            params.insert(
                "ChangeMessageVisibilityBatchRequestEntry.2.VisibilityTimeout".to_string(),
                "60".to_string(),
            );

            let request = SqsRequest {
                action: SqsAction::ChangeMessageVisibilityBatch,
                params,
                is_json: true,
            };

            let (response, content_type) = handler.handle_request(request).await;
            assert_eq!(content_type, "application/x-amz-json-1.0");
            assert!(response.contains("Successful"));
        }
    }
}
