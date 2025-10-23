//! SQS request parsing from form-encoded POST bodies.

use crate::sqs::types::{SqsAction, SqsMessageAttributeValue, SqsMessageAttributes};
use std::collections::HashMap;

/// Parsed SQS request.
#[derive(Debug, Clone)]
pub struct SqsRequest {
    /// The SQS action.
    pub action: SqsAction,
    /// Request parameters.
    pub params: HashMap<String, String>,
}

impl SqsRequest {
    /// Parse a form-encoded request body.
    pub fn parse(body: &str) -> Result<Self, String> {
        let params = parse_form_data(body);

        let action = params
            .get("Action")
            .and_then(|a| SqsAction::from_str(a))
            .ok_or_else(|| "Missing or invalid Action parameter".to_string())?;

        Ok(Self { action, params })
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

        // Queue attributes are encoded as:
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

        // Attribute names are encoded as:
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

        // Batch entries are encoded as:
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
        .last()
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
