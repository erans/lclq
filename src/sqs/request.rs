//! SQS request parsing from form-encoded POST bodies.

use crate::sqs::types::{SqsAction, SqsMessageAttributeValue, SqsMessageAttributes};
use axum::http::HeaderMap;
use serde_json::Value;
use std::collections::HashMap;

/// Parsed SQS request.
#[derive(Debug, Clone)]
pub struct SqsRequest {
    /// The SQS action.
    pub action: SqsAction,
    /// Request parameters.
    pub params: HashMap<String, String>,
    /// Whether this is a JSON request (AWS JSON 1.0 protocol).
    pub is_json: bool,
}

impl SqsRequest {
    /// Parse a request with headers (supports both JSON and form-encoded).
    pub fn parse_with_headers(body: &str, headers: &HeaderMap) -> Result<Self, String> {
        tracing::info!("Parsing request body: {}", body);

        // Check if this is a JSON request (AWS JSON 1.0 protocol)
        if body.trim().starts_with('{') {
            return Self::parse_json(body, headers);
        }

        // Otherwise, parse as form-encoded
        Self::parse(body)
    }

    /// Parse a JSON request (AWS JSON 1.0 protocol).
    fn parse_json(body: &str, headers: &HeaderMap) -> Result<Self, String> {
        // Extract action from X-Amz-Target header
        // Format: "AmazonSQS.CreateQueue"
        let target = headers
            .get("x-amz-target")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| "Missing X-Amz-Target header for JSON request".to_string())?;

        let action_str = target
            .strip_prefix("AmazonSQS.")
            .ok_or_else(|| format!("Invalid X-Amz-Target format: {}", target))?;

        let action = action_str.parse::<SqsAction>()
            .map_err(|e| format!("Unknown action: {}", e))?;

        // Parse JSON body
        let json: Value = serde_json::from_str(body)
            .map_err(|e| format!("Failed to parse JSON body: {}", e))?;

        // Convert JSON to HashMap<String, String>
        let mut params = HashMap::new();
        if let Some(obj) = json.as_object() {
            for (key, value) in obj {
                // Handle Attributes as a nested object or JSON string
                if key == "Attributes" {
                    match value {
                        Value::Object(attrs) => {
                            // Nested object - flatten it
                            for (attr_key, attr_val) in attrs {
                                let attr_str = match attr_val {
                                    Value::String(s) => s.clone(),
                                    _ => attr_val.to_string(),
                                };
                                params.insert(attr_key.clone(), attr_str);
                            }
                            continue; // Don't add Attributes itself
                        }
                        Value::String(s) if s.trim().starts_with('{') => {
                            // JSON string - parse and flatten
                            if let Ok(attrs) = serde_json::from_str::<serde_json::Map<String, Value>>(s) {
                                for (attr_key, attr_val) in attrs {
                                    let attr_str = match attr_val {
                                        Value::String(s) => s,
                                        _ => attr_val.to_string(),
                                    };
                                    params.insert(attr_key, attr_str);
                                }
                            }
                            continue; // Don't add Attributes itself
                        }
                        _ => {} // Fall through to normal handling
                    }
                }

                // Convert JSON value to string
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Object(_) => serde_json::to_string(value)
                        .map_err(|e| format!("Failed to serialize object: {}", e))?,
                    _ => value.to_string(),
                };
                params.insert(key.clone(), value_str);
            }
        }

        tracing::info!("Parsed JSON parameters: {:?}", params);

        Ok(Self {
            action,
            params,
            is_json: true,
        })
    }

    /// Parse a form-encoded request body.
    pub fn parse(body: &str) -> Result<Self, String> {
        tracing::info!("Parsing request body: {}", body);
        let params = parse_form_data(body);
        tracing::info!("Parsed parameters: {:?}", params);

        let action = params
            .get("Action")
            .and_then(|a| a.parse::<SqsAction>().ok())
            .ok_or_else(|| "Missing or invalid Action parameter".to_string())?;

        Ok(Self {
            action,
            params,
            is_json: false,
        })
    }

    /// Get a parameter value.
    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    /// Get a required parameter.
    pub fn get_required_param(&self, key: &str) -> Result<&str, String> {
        self.get_param(key)
            .ok_or_else(|| format!("Missing required parameter: {}", key))
    }

    /// Parse message attributes from the request.
    pub fn parse_message_attributes(&self) -> SqsMessageAttributes {
        let mut attributes = SqsMessageAttributes::new();

        // Message attributes are encoded as:
        // MessageAttribute.1.Name=attr1
        // MessageAttribute.1.Value.DataType=String
        // MessageAttribute.1.Value.StringValue=value1
        // MessageAttribute.2.Name=attr2
        // ...

        let mut index = 1;
        loop {
            let name_key = format!("MessageAttribute.{}.Name", index);
            let type_key = format!("MessageAttribute.{}.Value.DataType", index);

            if let (Some(name), Some(data_type)) =
                (self.get_param(&name_key), self.get_param(&type_key))
            {
                let string_value_key = format!("MessageAttribute.{}.Value.StringValue", index);
                let binary_value_key = format!("MessageAttribute.{}.Value.BinaryValue", index);

                let string_value = self.get_param(&string_value_key).map(String::from);
                let binary_value = self.get_param(&binary_value_key).map(String::from);

                attributes.insert(
                    name.to_string(),
                    SqsMessageAttributeValue {
                        data_type: data_type.to_string(),
                        string_value,
                        binary_value,
                    },
                );

                index += 1;
            } else {
                break;
            }
        }

        attributes
    }

    /// Parse queue attributes from the request.
    pub fn parse_queue_attributes(&self) -> HashMap<String, String> {
        let mut attributes = HashMap::new();

        // For JSON protocol, attributes are flattened directly into params
        // Check common attribute names first
        let known_attributes = [
            "FifoQueue",
            "ContentBasedDeduplication",
            "VisibilityTimeout",
            "MessageRetentionPeriod",
            "DelaySeconds",
            "MaximumMessageSize",
            "ReceiveMessageWaitTimeSeconds",
            "RedrivePolicy",
        ];

        for attr_name in &known_attributes {
            if let Some(value) = self.get_param(attr_name) {
                attributes.insert(attr_name.to_string(), value.to_string());
            }
        }

        // Also parse query protocol format:
        // Attribute.1.Name=VisibilityTimeout
        // Attribute.1.Value=30
        // Attribute.2.Name=MessageRetentionPeriod
        // Attribute.2.Value=345600
        // ...

        let mut index = 1;
        loop {
            let name_key = format!("Attribute.{}.Name", index);
            let value_key = format!("Attribute.{}.Value", index);

            if let (Some(name), Some(value)) =
                (self.get_param(&name_key), self.get_param(&value_key))
            {
                attributes.insert(name.to_string(), value.to_string());
                index += 1;
            } else {
                break;
            }
        }

        attributes
    }

    /// Parse attribute names from the request (for GetQueueAttributes).
    pub fn parse_attribute_names(&self) -> Vec<String> {
        let mut names = Vec::new();

        // For JSON requests, AttributeNames is a JSON array string
        if let Some(attr_names_json) = self.get_param("AttributeNames") {
            // Try to parse as JSON array
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(attr_names_json) {
                if let Some(arr) = json_value.as_array() {
                    for val in arr {
                        if let Some(s) = val.as_str() {
                            names.push(s.to_string());
                        }
                    }
                    return names;
                }
            }
        }

        // For form-encoded requests, attribute names are encoded as:
        // AttributeName.1=VisibilityTimeout
        // AttributeName.2=DelaySeconds
        // ...
        let mut index = 1;
        loop {
            let key = format!("AttributeName.{}", index);
            if let Some(name) = self.get_param(&key) {
                names.push(name.to_string());
                index += 1;
            } else {
                break;
            }
        }

        names
    }

    /// Parse batch entries for SendMessageBatch.
    pub fn parse_send_message_batch_entries(&self) -> Vec<SendMessageBatchEntry> {
        let mut entries = Vec::new();

        // For JSON requests, Entries is a JSON array string
        if let Some(entries_json) = self.get_param("Entries") {
            // Try to parse as JSON array
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(entries_json) {
                if let Some(arr) = json_value.as_array() {
                    for entry in arr {
                        if let (Some(id), Some(body)) = (
                            entry["Id"].as_str(),
                            entry["MessageBody"].as_str(),
                        ) {
                            let delay_seconds = entry["DelaySeconds"]
                                .as_u64()
                                .map(|d| d as u32);

                            let dedup_id = entry["MessageDeduplicationId"]
                                .as_str()
                                .map(String::from);

                            let group_id = entry["MessageGroupId"]
                                .as_str()
                                .map(String::from);

                            entries.push(SendMessageBatchEntry {
                                id: id.to_string(),
                                message_body: body.to_string(),
                                delay_seconds,
                                message_deduplication_id: dedup_id,
                                message_group_id: group_id,
                            });
                        }
                    }
                    return entries;
                }
            }
        }

        // For form-encoded requests, batch entries are encoded as:
        // SendMessageBatchRequestEntry.1.Id=msg1
        // SendMessageBatchRequestEntry.1.MessageBody=Hello
        // SendMessageBatchRequestEntry.2.Id=msg2
        // SendMessageBatchRequestEntry.2.MessageBody=World
        // ...

        let mut index = 1;
        loop {
            let id_key = format!("SendMessageBatchRequestEntry.{}.Id", index);
            let body_key = format!("SendMessageBatchRequestEntry.{}.MessageBody", index);

            if let (Some(id), Some(body)) = (self.get_param(&id_key), self.get_param(&body_key)) {
                let delay_key = format!("SendMessageBatchRequestEntry.{}.DelaySeconds", index);
                let delay_seconds = self
                    .get_param(&delay_key)
                    .and_then(|s| s.parse().ok());

                let dedup_id_key = format!("SendMessageBatchRequestEntry.{}.MessageDeduplicationId", index);
                let dedup_id = self.get_param(&dedup_id_key).map(String::from);

                let group_id_key = format!("SendMessageBatchRequestEntry.{}.MessageGroupId", index);
                let group_id = self.get_param(&group_id_key).map(String::from);

                entries.push(SendMessageBatchEntry {
                    id: id.to_string(),
                    message_body: body.to_string(),
                    delay_seconds,
                    message_deduplication_id: dedup_id,
                    message_group_id: group_id,
                });

                index += 1;
            } else {
                break;
            }
        }

        entries
    }
}

/// Batch entry for SendMessageBatch.
#[derive(Debug, Clone)]
pub struct SendMessageBatchEntry {
    /// Entry ID.
    pub id: String,
    /// Message body.
    pub message_body: String,
    /// Delay seconds.
    pub delay_seconds: Option<u32>,
    /// Message deduplication ID (FIFO).
    pub message_deduplication_id: Option<String>,
    /// Message group ID (FIFO).
    pub message_group_id: Option<String>,
}

/// Parse form-encoded data into a HashMap.
fn parse_form_data(body: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();

    for pair in body.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            // Replace + with space before URL decoding (application/x-www-form-urlencoded format)
            let value_with_spaces = value.replace('+', " ");
            let decoded_key = urlencoding::decode(key).unwrap_or_default();
            let decoded_value = urlencoding::decode(&value_with_spaces).unwrap_or_default();
            params.insert(decoded_key.to_string(), decoded_value.to_string());
        }
    }

    params
}

/// Extract queue name from queue URL.
pub fn extract_queue_name_from_url(queue_url: &str) -> Option<String> {
    // Queue URL format: http://localhost:9324/queue/my-queue
    queue_url
        .split('/')
        .next_back()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_request() {
        let body = "Action=CreateQueue&QueueName=test-queue&Version=2012-11-05";
        let req = SqsRequest::parse(body).unwrap();

        assert_eq!(req.action, SqsAction::CreateQueue);
        assert_eq!(req.get_param("QueueName"), Some("test-queue"));
    }

    #[test]
    fn test_parse_send_message() {
        let body = "Action=SendMessage&QueueUrl=http%3A%2F%2Flocalhost%3A9324%2Fqueue%2Ftest&MessageBody=Hello+World";
        let req = SqsRequest::parse(body).unwrap();

        assert_eq!(req.action, SqsAction::SendMessage);
        assert_eq!(req.get_param("MessageBody"), Some("Hello World"));
    }

    #[test]
    fn test_parse_message_attributes() {
        let body = "Action=SendMessage&MessageAttribute.1.Name=attr1&MessageAttribute.1.Value.DataType=String&MessageAttribute.1.Value.StringValue=value1";
        let req = SqsRequest::parse(body).unwrap();

        let attrs = req.parse_message_attributes();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs.get("attr1").unwrap().data_type, "String");
        assert_eq!(
            attrs.get("attr1").unwrap().string_value,
            Some("value1".to_string())
        );
    }

    #[test]
    fn test_extract_queue_name() {
        assert_eq!(
            extract_queue_name_from_url("http://localhost:9324/queue/my-queue"),
            Some("my-queue".to_string())
        );
        assert_eq!(
            extract_queue_name_from_url("http://localhost:9324/queue/my-fifo.fifo"),
            Some("my-fifo.fifo".to_string())
        );
    }

    #[test]
    fn test_parse_batch_entries() {
        let body = "Action=SendMessageBatch&SendMessageBatchRequestEntry.1.Id=msg1&SendMessageBatchRequestEntry.1.MessageBody=Hello&SendMessageBatchRequestEntry.2.Id=msg2&SendMessageBatchRequestEntry.2.MessageBody=World";
        let req = SqsRequest::parse(body).unwrap();

        let entries = req.parse_send_message_batch_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "msg1");
        assert_eq!(entries[0].message_body, "Hello");
        assert_eq!(entries[1].id, "msg2");
        assert_eq!(entries[1].message_body, "World");
    }
}
