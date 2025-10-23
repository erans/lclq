//! SQS action handler - processes SQS actions using the storage backend.

use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use tracing::{debug, info};

use crate::config::LclqConfig;
use crate::sqs::{
    build_create_queue_response, build_delete_message_response, build_error_response,
    build_get_queue_url_response, build_list_queues_response, build_receive_message_response,
    build_send_message_response, calculate_md5_of_attributes, calculate_md5_of_body,
    extract_queue_name_from_url, MessageAttributeInfo, ReceivedMessageInfo, SqsAction,
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
            SqsAction::ReceiveMessage => self.handle_receive_message(request).await,
            SqsAction::DeleteMessage => self.handle_delete_message(request).await,
            SqsAction::PurgeQueue => self.handle_purge_queue(request).await,
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

