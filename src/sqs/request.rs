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

        let action = action_str
            .parse::<SqsAction>()
            .map_err(|e| format!("Unknown action: {}", e))?;

        // Parse JSON body
        let json: Value =
            serde_json::from_str(body).map_err(|e| format!("Failed to parse JSON body: {}", e))?;

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
                            if let Ok(attrs) =
                                serde_json::from_str::<serde_json::Map<String, Value>>(s)
                            {
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

                // Handle MessageAttributes - expand into numbered parameters
                // MessageAttributes: {Author: {DataType: "String", StringValue: "SDK"}, ...}
                // becomes:
                // MessageAttribute.1.Name=Author
                // MessageAttribute.1.Value.DataType=String
                // MessageAttribute.1.Value.StringValue=SDK
                if key == "MessageAttributes" {
                    if let Value::Object(attrs) = value {
                        let mut index = 1;
                        for (attr_name, attr_value) in attrs {
                            params.insert(
                                format!("MessageAttribute.{}.Name", index),
                                attr_name.clone(),
                            );

                            if let Value::Object(attr_obj) = attr_value {
                                // Extract DataType
                                if let Some(Value::String(data_type)) = attr_obj.get("DataType") {
                                    params.insert(
                                        format!("MessageAttribute.{}.Value.DataType", index),
                                        data_type.clone(),
                                    );
                                }

                                // Extract StringValue if present
                                if let Some(Value::String(string_value)) =
                                    attr_obj.get("StringValue")
                                {
                                    params.insert(
                                        format!("MessageAttribute.{}.Value.StringValue", index),
                                        string_value.clone(),
                                    );
                                }

                                // Extract BinaryValue if present
                                if let Some(Value::String(binary_value)) =
                                    attr_obj.get("BinaryValue")
                                {
                                    params.insert(
                                        format!("MessageAttribute.{}.Value.BinaryValue", index),
                                        binary_value.clone(),
                                    );
                                }
                            }

                            index += 1;
                        }
                        continue; // Don't add MessageAttributes itself
                    }
                }

                // Convert JSON value to string or expand arrays
                match value {
                    Value::String(s) => {
                        params.insert(key.clone(), s.clone());
                    }
                    Value::Number(n) => {
                        params.insert(key.clone(), n.to_string());
                    }
                    Value::Bool(b) => {
                        params.insert(key.clone(), b.to_string());
                    }
                    Value::Array(arr) => {
                        // Keep "Entries" as JSON string for batch operations
                        if key == "Entries" {
                            let value_str =
                                serde_json::to_string(value).unwrap_or_else(|_| "[]".to_string());
                            params.insert(key.clone(), value_str);
                        } else {
                            // Expand other arrays into numbered parameters
                            // e.g., AttributeNames: ["All", "Policy"] becomes:
                            // AttributeName.1 = All, AttributeName.2 = Policy
                            let singular_key = if key.ends_with("s") {
                                &key[..key.len() - 1] // Remove trailing 's'
                            } else {
                                key.as_str()
                            };

                            for (i, item) in arr.iter().enumerate() {
                                let numbered_key = format!("{}.{}", singular_key, i + 1);
                                let item_str = match item {
                                    Value::String(s) => s.clone(),
                                    _ => item.to_string(),
                                };
                                params.insert(numbered_key, item_str);
                            }
                        }
                    }
                    Value::Object(_) => {
                        let value_str = serde_json::to_string(value)
                            .map_err(|e| format!("Failed to serialize object: {}", e))?;
                        params.insert(key.clone(), value_str);
                    }
                    Value::Null => {
                        // Skip null values
                    }
                }
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
            "RedriveAllowPolicy",
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
                        if let (Some(id), Some(body)) =
                            (entry["Id"].as_str(), entry["MessageBody"].as_str())
                        {
                            let delay_seconds = entry["DelaySeconds"].as_u64().map(|d| d as u32);

                            let dedup_id =
                                entry["MessageDeduplicationId"].as_str().map(String::from);

                            let group_id = entry["MessageGroupId"].as_str().map(String::from);

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
                let delay_seconds = self.get_param(&delay_key).and_then(|s| s.parse().ok());

                let dedup_id_key = format!(
                    "SendMessageBatchRequestEntry.{}.MessageDeduplicationId",
                    index
                );
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

    /// Parse batch entries for DeleteMessageBatch.
    pub fn parse_delete_message_batch_entries(&self) -> Vec<DeleteMessageBatchEntry> {
        let mut entries = Vec::new();

        // For JSON requests, Entries is a JSON array string
        if let Some(entries_json) = self.get_param("Entries") {
            // Try to parse as JSON array
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(entries_json) {
                if let Some(arr) = json_value.as_array() {
                    for entry in arr {
                        if let (Some(id), Some(receipt_handle)) =
                            (entry["Id"].as_str(), entry["ReceiptHandle"].as_str())
                        {
                            entries.push(DeleteMessageBatchEntry {
                                id: id.to_string(),
                                receipt_handle: receipt_handle.to_string(),
                            });
                        }
                    }
                    return entries;
                }
            }
        }

        // For form-encoded requests, batch entries are encoded as:
        // DeleteMessageBatchRequestEntry.1.Id=msg1
        // DeleteMessageBatchRequestEntry.1.ReceiptHandle=handle1
        // ...

        let mut index = 1;
        loop {
            let id_key = format!("DeleteMessageBatchRequestEntry.{}.Id", index);
            let handle_key = format!("DeleteMessageBatchRequestEntry.{}.ReceiptHandle", index);

            if let (Some(id), Some(handle)) = (self.get_param(&id_key), self.get_param(&handle_key))
            {
                entries.push(DeleteMessageBatchEntry {
                    id: id.to_string(),
                    receipt_handle: handle.to_string(),
                });

                index += 1;
            } else {
                break;
            }
        }

        entries
    }

    /// Parse batch entries for ChangeMessageVisibilityBatch.
    pub fn parse_change_visibility_batch_entries(&self) -> Vec<ChangeVisibilityBatchEntry> {
        let mut entries = Vec::new();

        // For JSON requests, Entries is a JSON array string
        if let Some(entries_json) = self.get_param("Entries") {
            // Try to parse as JSON array
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(entries_json) {
                if let Some(arr) = json_value.as_array() {
                    for entry in arr {
                        if let (Some(id), Some(receipt_handle), Some(timeout)) = (
                            entry["Id"].as_str(),
                            entry["ReceiptHandle"].as_str(),
                            entry["VisibilityTimeout"].as_u64(),
                        ) {
                            entries.push(ChangeVisibilityBatchEntry {
                                id: id.to_string(),
                                receipt_handle: receipt_handle.to_string(),
                                visibility_timeout: timeout as u32,
                            });
                        }
                    }
                    return entries;
                }
            }
        }

        // For form-encoded requests
        let mut index = 1;
        loop {
            let id_key = format!("ChangeMessageVisibilityBatchRequestEntry.{}.Id", index);
            let handle_key = format!(
                "ChangeMessageVisibilityBatchRequestEntry.{}.ReceiptHandle",
                index
            );
            let timeout_key = format!(
                "ChangeMessageVisibilityBatchRequestEntry.{}.VisibilityTimeout",
                index
            );

            if let (Some(id), Some(handle), Some(timeout_str)) = (
                self.get_param(&id_key),
                self.get_param(&handle_key),
                self.get_param(&timeout_key),
            ) {
                if let Ok(timeout) = timeout_str.parse::<u32>() {
                    entries.push(ChangeVisibilityBatchEntry {
                        id: id.to_string(),
                        receipt_handle: handle.to_string(),
                        visibility_timeout: timeout,
                    });
                }

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

/// Batch entry for DeleteMessageBatch.
#[derive(Debug, Clone)]
pub struct DeleteMessageBatchEntry {
    /// Entry ID.
    pub id: String,
    /// Receipt handle.
    pub receipt_handle: String,
}

/// Batch entry for ChangeMessageVisibilityBatch.
#[derive(Debug, Clone)]
pub struct ChangeVisibilityBatchEntry {
    /// Entry ID.
    pub id: String,
    /// Receipt handle.
    pub receipt_handle: String,
    /// Visibility timeout in seconds.
    pub visibility_timeout: u32,
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

    // ========================================================================
    // JSON Request Parsing Tests (parse_json method)
    // ========================================================================

    #[test]
    fn test_parse_json_create_queue() {
        use axum::http::HeaderMap;

        let json_body = r#"{"QueueName": "test-queue"}"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSQS.CreateQueue".parse().unwrap());

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();

        assert_eq!(req.action, SqsAction::CreateQueue);
        assert_eq!(req.get_param("QueueName"), Some("test-queue"));
        assert!(req.is_json);
    }

    #[test]
    fn test_parse_json_send_message() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueUrl": "http://localhost:9324/queue/test",
            "MessageBody": "Hello JSON World"
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSQS.SendMessage".parse().unwrap());

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();

        assert_eq!(req.action, SqsAction::SendMessage);
        assert_eq!(
            req.get_param("QueueUrl"),
            Some("http://localhost:9324/queue/test")
        );
        assert_eq!(req.get_param("MessageBody"), Some("Hello JSON World"));
    }

    #[test]
    fn test_parse_json_with_message_attributes() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueUrl": "http://localhost:9324/queue/test",
            "MessageBody": "Test",
            "MessageAttributes": {
                "attr1": {
                    "DataType": "String",
                    "StringValue": "value1"
                },
                "attr2": {
                    "DataType": "Number",
                    "StringValue": "123"
                }
            }
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSQS.SendMessage".parse().unwrap());

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();
        let attrs = req.parse_message_attributes();

        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs.get("attr1").unwrap().data_type, "String");
        assert_eq!(
            attrs.get("attr1").unwrap().string_value,
            Some("value1".to_string())
        );
        assert_eq!(attrs.get("attr2").unwrap().data_type, "Number");
        assert_eq!(
            attrs.get("attr2").unwrap().string_value,
            Some("123".to_string())
        );
    }

    #[test]
    fn test_parse_json_with_queue_attributes() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueName": "test-queue",
            "Attributes": {
                "VisibilityTimeout": "60",
                "MessageRetentionPeriod": "86400"
            }
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSQS.CreateQueue".parse().unwrap());

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();
        let attrs = req.parse_queue_attributes();

        assert_eq!(attrs.get("VisibilityTimeout"), Some(&"60".to_string()));
        assert_eq!(
            attrs.get("MessageRetentionPeriod"),
            Some(&"86400".to_string())
        );
    }

    #[test]
    fn test_parse_json_send_message_batch() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueUrl": "http://localhost:9324/queue/test",
            "Entries": [
                {
                    "Id": "msg1",
                    "MessageBody": "First message"
                },
                {
                    "Id": "msg2",
                    "MessageBody": "Second message",
                    "DelaySeconds": 5
                }
            ]
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amz-target",
            "AmazonSQS.SendMessageBatch".parse().unwrap(),
        );

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();
        let entries = req.parse_send_message_batch_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "msg1");
        assert_eq!(entries[0].message_body, "First message");
        assert_eq!(entries[0].delay_seconds, None);
        assert_eq!(entries[1].id, "msg2");
        assert_eq!(entries[1].message_body, "Second message");
        assert_eq!(entries[1].delay_seconds, Some(5));
    }

    #[test]
    fn test_parse_json_delete_message_batch() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueUrl": "http://localhost:9324/queue/test",
            "Entries": [
                {
                    "Id": "msg1",
                    "ReceiptHandle": "handle1"
                },
                {
                    "Id": "msg2",
                    "ReceiptHandle": "handle2"
                }
            ]
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amz-target",
            "AmazonSQS.DeleteMessageBatch".parse().unwrap(),
        );

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();
        let entries = req.parse_delete_message_batch_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "msg1");
        assert_eq!(entries[0].receipt_handle, "handle1");
        assert_eq!(entries[1].id, "msg2");
        assert_eq!(entries[1].receipt_handle, "handle2");
    }

    #[test]
    fn test_parse_json_change_visibility_batch() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueUrl": "http://localhost:9324/queue/test",
            "Entries": [
                {
                    "Id": "msg1",
                    "ReceiptHandle": "handle1",
                    "VisibilityTimeout": 30
                },
                {
                    "Id": "msg2",
                    "ReceiptHandle": "handle2",
                    "VisibilityTimeout": 60
                }
            ]
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amz-target",
            "AmazonSQS.ChangeMessageVisibilityBatch".parse().unwrap(),
        );

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();
        let entries = req.parse_change_visibility_batch_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "msg1");
        assert_eq!(entries[0].receipt_handle, "handle1");
        assert_eq!(entries[0].visibility_timeout, 30);
        assert_eq!(entries[1].id, "msg2");
        assert_eq!(entries[1].receipt_handle, "handle2");
        assert_eq!(entries[1].visibility_timeout, 60);
    }

    #[test]
    fn test_parse_json_get_queue_attributes_with_attribute_names() {
        use axum::http::HeaderMap;

        let json_body = r#"{
            "QueueUrl": "http://localhost:9324/queue/test",
            "AttributeNames": ["VisibilityTimeout", "MessageRetentionPeriod", "QueueArn"]
        }"#;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-amz-target",
            "AmazonSQS.GetQueueAttributes".parse().unwrap(),
        );

        let req = SqsRequest::parse_with_headers(json_body, &headers).unwrap();
        let attr_names = req.parse_attribute_names();

        assert_eq!(attr_names.len(), 3);
        assert!(attr_names.contains(&"VisibilityTimeout".to_string()));
        assert!(attr_names.contains(&"MessageRetentionPeriod".to_string()));
        assert!(attr_names.contains(&"QueueArn".to_string()));
    }

    #[test]
    fn test_parse_json_missing_x_amz_target() {
        use axum::http::HeaderMap;

        let json_body = r#"{"QueueName": "test-queue"}"#;
        let headers = HeaderMap::new();

        let result = SqsRequest::parse_with_headers(json_body, &headers);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_invalid_action() {
        use axum::http::HeaderMap;

        let json_body = r#"{"QueueName": "test-queue"}"#;
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSQS.InvalidAction".parse().unwrap());

        let result = SqsRequest::parse_with_headers(json_body, &headers);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_malformed_json() {
        use axum::http::HeaderMap;

        let json_body = r#"{"QueueName": "test-queue"#; // Missing closing brace
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSQS.CreateQueue".parse().unwrap());

        let result = SqsRequest::parse_with_headers(json_body, &headers);
        assert!(result.is_err());
    }

    // ========================================================================
    // Form-Encoded Parsing Edge Cases
    // ========================================================================

    #[test]
    fn test_parse_form_multiple_message_attributes() {
        let body = "Action=SendMessage&MessageAttribute.1.Name=attr1&MessageAttribute.1.Value.DataType=String&MessageAttribute.1.Value.StringValue=value1&MessageAttribute.2.Name=attr2&MessageAttribute.2.Value.DataType=Number&MessageAttribute.2.Value.StringValue=42";
        let req = SqsRequest::parse(body).unwrap();

        let attrs = req.parse_message_attributes();
        assert_eq!(attrs.len(), 2);
        assert_eq!(
            attrs.get("attr1").unwrap().string_value,
            Some("value1".to_string())
        );
        assert_eq!(
            attrs.get("attr2").unwrap().string_value,
            Some("42".to_string())
        );
    }

    #[test]
    fn test_parse_form_queue_attributes() {
        let body = "Action=CreateQueue&QueueName=test&Attribute.1.Name=VisibilityTimeout&Attribute.1.Value=60&Attribute.2.Name=DelaySeconds&Attribute.2.Value=10";
        let req = SqsRequest::parse(body).unwrap();

        let attrs = req.parse_queue_attributes();
        assert_eq!(attrs.get("VisibilityTimeout"), Some(&"60".to_string()));
        assert_eq!(attrs.get("DelaySeconds"), Some(&"10".to_string()));
    }

    #[test]
    fn test_parse_form_attribute_names() {
        let body = "Action=GetQueueAttributes&QueueUrl=http%3A%2F%2Flocalhost%3A9324%2Fqueue%2Ftest&AttributeName.1=VisibilityTimeout&AttributeName.2=MessageRetentionPeriod";
        let req = SqsRequest::parse(body).unwrap();

        let attr_names = req.parse_attribute_names();
        assert_eq!(attr_names.len(), 2);
        assert!(attr_names.contains(&"VisibilityTimeout".to_string()));
        assert!(attr_names.contains(&"MessageRetentionPeriod".to_string()));
    }

    #[test]
    fn test_parse_form_attribute_names_with_all() {
        let body = "Action=GetQueueAttributes&QueueUrl=http%3A%2F%2Flocalhost%3A9324%2Fqueue%2Ftest&AttributeName.1=All";
        let req = SqsRequest::parse(body).unwrap();

        let attr_names = req.parse_attribute_names();
        assert_eq!(attr_names.len(), 1);
        assert_eq!(attr_names[0], "All");
    }

    #[test]
    fn test_parse_send_message_batch_with_fifo_fields() {
        let body = "Action=SendMessageBatch&SendMessageBatchRequestEntry.1.Id=msg1&SendMessageBatchRequestEntry.1.MessageBody=Hello&SendMessageBatchRequestEntry.1.MessageGroupId=group1&SendMessageBatchRequestEntry.1.MessageDeduplicationId=dedup1";
        let req = SqsRequest::parse(body).unwrap();

        let entries = req.parse_send_message_batch_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message_group_id, Some("group1".to_string()));
        assert_eq!(
            entries[0].message_deduplication_id,
            Some("dedup1".to_string())
        );
    }

    #[test]
    fn test_extract_queue_name_edge_cases() {
        // Valid cases
        assert_eq!(
            extract_queue_name_from_url("http://localhost:9324/queue/test-queue"),
            Some("test-queue".to_string())
        );
        assert_eq!(
            extract_queue_name_from_url(
                "https://sqs.us-east-1.amazonaws.com/123456789012/my-queue"
            ),
            Some("my-queue".to_string())
        );

        // Invalid cases
        assert_eq!(
            extract_queue_name_from_url("http://localhost:9324/queue/"),
            None
        );
        assert_eq!(extract_queue_name_from_url("http://localhost:9324/"), None);
        assert_eq!(extract_queue_name_from_url(""), None);
    }

    #[test]
    fn test_get_required_param_missing() {
        let body = "Action=CreateQueue";
        let req = SqsRequest::parse(body).unwrap();

        let result = req.get_required_param("QueueName");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("QueueName"));
    }

    #[test]
    fn test_parse_missing_action() {
        let body = "QueueName=test-queue";
        let result = SqsRequest::parse(body);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_action() {
        let body = "Action=InvalidAction&QueueName=test";
        let result = SqsRequest::parse(body);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_url_encoding_special_chars() {
        let body = "Action=SendMessage&MessageBody=Hello%20World%21%40%23%24%25";
        let req = SqsRequest::parse(body).unwrap();

        assert_eq!(req.get_param("MessageBody"), Some("Hello World!@#$%"));
    }

    #[test]
    fn test_parse_empty_message_attributes() {
        let body = "Action=SendMessage&MessageBody=Test";
        let req = SqsRequest::parse(body).unwrap();

        let attrs = req.parse_message_attributes();
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn test_parse_empty_batch_entries() {
        let body = "Action=SendMessageBatch&QueueUrl=http://localhost:9324/queue/test";
        let req = SqsRequest::parse(body).unwrap();

        let entries = req.parse_send_message_batch_entries();
        assert_eq!(entries.len(), 0);
    }
}
