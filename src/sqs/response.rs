//! SQS XML response generation.

use crate::sqs::types::SqsErrorCode;
use uuid::Uuid;

/// XML response builder for SQS.
pub struct XmlResponseBuilder {
    action: String,
    request_id: String,
    elements: Vec<(String, String)>,
}

impl XmlResponseBuilder {
    /// Create a new response builder for a specific action.
    pub fn new(action: &str) -> Self {
        Self {
            action: action.to_string(),
            request_id: Uuid::new_v4().to_string(),
            elements: Vec::new(),
        }
    }

    /// Add an element to the response.
    pub fn add_element(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.elements.push((name.into(), value.into()));
        self
    }

    /// Build the XML response.
    pub fn build(self) -> String {
        let mut xml = String::new();
        xml.push_str(r#"<?xml version="1.0"?>"#);
        xml.push('\n');
        xml.push_str(&format!(
            r#"<{}Response xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#,
            self.action
        ));
        xml.push('\n');
        xml.push_str(&format!("  <{}Result>\n", self.action));

        for (name, value) in &self.elements {
            xml.push_str(&format!("    <{}>{}</{}>\n", name, escape_xml(value), name));
        }

        xml.push_str(&format!("  </{}Result>\n", self.action));
        xml.push_str("  <ResponseMetadata>\n");
        xml.push_str(&format!("    <RequestId>{}</RequestId>\n", self.request_id));
        xml.push_str("  </ResponseMetadata>\n");
        xml.push_str(&format!("</{}Response>", self.action));

        xml
    }
}

/// Build an error response.
pub fn build_error_response(code: SqsErrorCode, message: &str) -> String {
    let request_id = Uuid::new_v4();
    format!(
        r#"<?xml version="1.0"?>
<ErrorResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">
  <Error>
    <Type>Sender</Type>
    <Code>{}</Code>
    <Message>{}</Message>
  </Error>
  <RequestId>{}</RequestId>
</ErrorResponse>"#,
        code.as_str(),
        escape_xml(message),
        request_id
    )
}

/// Build a SendMessage response.
pub fn build_send_message_response(message_id: &str, md5_of_body: &str, md5_of_attrs: Option<&str>) -> String {
    let builder = XmlResponseBuilder::new("SendMessage")
        .add_element("MessageId", message_id)
        .add_element("MD5OfMessageBody", md5_of_body);

    if let Some(md5) = md5_of_attrs {
        builder.add_element("MD5OfMessageAttributes", md5).build()
    } else {
        builder.build()
    }
}

/// Batch result entry for SendMessageBatch.
pub struct BatchResultEntry {
    /// Entry ID.
    pub id: String,
    /// Message ID.
    pub message_id: String,
    /// MD5 of message body.
    pub md5_of_body: String,
    /// MD5 of message attributes (optional).
    pub md5_of_attrs: Option<String>,
}

/// Batch error entry for SendMessageBatch.
pub struct BatchErrorEntry {
    /// Entry ID.
    pub id: String,
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
    /// Sender fault flag.
    pub sender_fault: bool,
}

/// Build a SendMessageBatch response.
pub fn build_send_message_batch_response(
    successful: &[BatchResultEntry],
    failed: &[BatchErrorEntry],
) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0"?>"#);
    xml.push('\n');
    xml.push_str(r#"<SendMessageBatchResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#);
    xml.push('\n');
    xml.push_str("  <SendMessageBatchResult>\n");

    // Add successful entries
    for entry in successful {
        xml.push_str("    <SendMessageBatchResultEntry>\n");
        xml.push_str(&format!("      <Id>{}</Id>\n", escape_xml(&entry.id)));
        xml.push_str(&format!("      <MessageId>{}</MessageId>\n", escape_xml(&entry.message_id)));
        xml.push_str(&format!("      <MD5OfMessageBody>{}</MD5OfMessageBody>\n", escape_xml(&entry.md5_of_body)));
        if let Some(md5_attrs) = &entry.md5_of_attrs {
            xml.push_str(&format!("      <MD5OfMessageAttributes>{}</MD5OfMessageAttributes>\n", escape_xml(md5_attrs)));
        }
        xml.push_str("    </SendMessageBatchResultEntry>\n");
    }

    // Add failed entries
    for entry in failed {
        xml.push_str("    <BatchResultErrorEntry>\n");
        xml.push_str(&format!("      <Id>{}</Id>\n", escape_xml(&entry.id)));
        xml.push_str(&format!("      <Code>{}</Code>\n", escape_xml(&entry.code)));
        xml.push_str(&format!("      <Message>{}</Message>\n", escape_xml(&entry.message)));
        xml.push_str(&format!("      <SenderFault>{}</SenderFault>\n", entry.sender_fault));
        xml.push_str("    </BatchResultErrorEntry>\n");
    }

    xml.push_str("  </SendMessageBatchResult>\n");
    xml.push_str("  <ResponseMetadata>\n");
    xml.push_str(&format!("    <RequestId>{}</RequestId>\n", Uuid::new_v4()));
    xml.push_str("  </ResponseMetadata>\n");
    xml.push_str("</SendMessageBatchResponse>");

    xml
}

/// Build a DeleteMessageBatch response.
pub fn build_delete_message_batch_response(
    successful: &[String],  // Just IDs for successful deletes
    failed: &[BatchErrorEntry],
) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0"?>"#);
    xml.push('\n');
    xml.push_str(r#"<DeleteMessageBatchResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#);
    xml.push('\n');
    xml.push_str("  <DeleteMessageBatchResult>\n");

    // Add successful entries
    for id in successful {
        xml.push_str("    <DeleteMessageBatchResultEntry>\n");
        xml.push_str(&format!("      <Id>{}</Id>\n", escape_xml(id)));
        xml.push_str("    </DeleteMessageBatchResultEntry>\n");
    }

    // Add failed entries
    for entry in failed {
        xml.push_str("    <BatchResultErrorEntry>\n");
        xml.push_str(&format!("      <Id>{}</Id>\n", escape_xml(&entry.id)));
        xml.push_str(&format!("      <Code>{}</Code>\n", escape_xml(&entry.code)));
        xml.push_str(&format!("      <Message>{}</Message>\n", escape_xml(&entry.message)));
        xml.push_str(&format!("      <SenderFault>{}</SenderFault>\n", entry.sender_fault));
        xml.push_str("    </BatchResultErrorEntry>\n");
    }

    xml.push_str("  </DeleteMessageBatchResult>\n");
    xml.push_str("  <ResponseMetadata>\n");
    xml.push_str(&format!("    <RequestId>{}</RequestId>\n", Uuid::new_v4()));
    xml.push_str("  </ResponseMetadata>\n");
    xml.push_str("</DeleteMessageBatchResponse>");

    xml
}

/// Build a ChangeMessageVisibilityBatch response.
pub fn build_change_visibility_batch_response(
    successful: &[String],  // Just IDs for successful changes
    failed: &[BatchErrorEntry],
) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0"?>"#);
    xml.push('\n');
    xml.push_str(r#"<ChangeMessageVisibilityBatchResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#);
    xml.push('\n');
    xml.push_str("  <ChangeMessageVisibilityBatchResult>\n");

    // Add successful entries
    for id in successful {
        xml.push_str("    <ChangeMessageVisibilityBatchResultEntry>\n");
        xml.push_str(&format!("      <Id>{}</Id>\n", escape_xml(id)));
        xml.push_str("    </ChangeMessageVisibilityBatchResultEntry>\n");
    }

    // Add failed entries
    for entry in failed {
        xml.push_str("    <BatchResultErrorEntry>\n");
        xml.push_str(&format!("      <Id>{}</Id>\n", escape_xml(&entry.id)));
        xml.push_str(&format!("      <Code>{}</Code>\n", escape_xml(&entry.code)));
        xml.push_str(&format!("      <Message>{}</Message>\n", escape_xml(&entry.message)));
        xml.push_str(&format!("      <SenderFault>{}</SenderFault>\n", entry.sender_fault));
        xml.push_str("    </BatchResultErrorEntry>\n");
    }

    xml.push_str("  </ChangeMessageVisibilityBatchResult>\n");
    xml.push_str("  <ResponseMetadata>\n");
    xml.push_str(&format!("    <RequestId>{}</RequestId>\n", Uuid::new_v4()));
    xml.push_str("  </ResponseMetadata>\n");
    xml.push_str("</ChangeMessageVisibilityBatchResponse>");

    xml
}

/// Build a ReceiveMessage response.
pub fn build_receive_message_response(messages: &[ReceivedMessageInfo]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0"?>"#);
    xml.push('\n');
    xml.push_str(r#"<ReceiveMessageResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#);
    xml.push('\n');
    xml.push_str("  <ReceiveMessageResult>\n");

    for msg in messages {
        xml.push_str("    <Message>\n");
        xml.push_str(&format!("      <MessageId>{}</MessageId>\n", msg.message_id));
        xml.push_str(&format!("      <ReceiptHandle>{}</ReceiptHandle>\n", msg.receipt_handle));
        xml.push_str(&format!("      <MD5OfBody>{}</MD5OfBody>\n", msg.md5_of_body));
        xml.push_str(&format!("      <Body>{}</Body>\n", escape_xml(&msg.body)));

        // Add attributes
        for (key, value) in &msg.attributes {
            xml.push_str("      <Attribute>\n");
            xml.push_str(&format!("        <Name>{}</Name>\n", key));
            xml.push_str(&format!("        <Value>{}</Value>\n", escape_xml(value)));
            xml.push_str("      </Attribute>\n");
        }

        // Add message attributes
        for (key, value) in &msg.message_attributes {
            xml.push_str("      <MessageAttribute>\n");
            xml.push_str(&format!("        <Name>{}</Name>\n", key));
            xml.push_str("        <Value>\n");
            xml.push_str(&format!("          <DataType>{}</DataType>\n", value.data_type));
            if let Some(ref string_value) = value.string_value {
                xml.push_str(&format!("          <StringValue>{}</StringValue>\n", escape_xml(string_value)));
            }
            xml.push_str("        </Value>\n");
            xml.push_str("      </MessageAttribute>\n");
        }

        xml.push_str("    </Message>\n");
    }

    xml.push_str("  </ReceiveMessageResult>\n");
    xml.push_str("  <ResponseMetadata>\n");
    xml.push_str(&format!("    <RequestId>{}</RequestId>\n", Uuid::new_v4()));
    xml.push_str("  </ResponseMetadata>\n");
    xml.push_str("</ReceiveMessageResponse>");

    xml
}

/// Build a CreateQueue response.
pub fn build_create_queue_response(queue_url: &str) -> String {
    XmlResponseBuilder::new("CreateQueue")
        .add_element("QueueUrl", queue_url)
        .build()
}

/// Build a GetQueueUrl response.
pub fn build_get_queue_url_response(queue_url: &str) -> String {
    XmlResponseBuilder::new("GetQueueUrl")
        .add_element("QueueUrl", queue_url)
        .build()
}

/// Build a DeleteMessage response (empty result).
pub fn build_delete_message_response() -> String {
    XmlResponseBuilder::new("DeleteMessage").build()
}

/// Build a TagQueue response (empty result).
pub fn build_tag_queue_response() -> String {
    XmlResponseBuilder::new("TagQueue").build()
}

/// Build an UntagQueue response (empty result).
pub fn build_untag_queue_response() -> String {
    XmlResponseBuilder::new("UntagQueue").build()
}

/// Build a ChangeMessageVisibility response (empty result).
pub fn build_change_message_visibility_response() -> String {
    XmlResponseBuilder::new("ChangeMessageVisibility").build()
}

/// Build a ListQueues response.
pub fn build_list_queues_response(queue_urls: &[String]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0"?>"#);
    xml.push('\n');
    xml.push_str(r#"<ListQueuesResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#);
    xml.push('\n');
    xml.push_str("  <ListQueuesResult>\n");

    for url in queue_urls {
        xml.push_str(&format!("    <QueueUrl>{}</QueueUrl>\n", escape_xml(url)));
    }

    xml.push_str("  </ListQueuesResult>\n");
    xml.push_str("  <ResponseMetadata>\n");
    xml.push_str(&format!("    <RequestId>{}</RequestId>\n", Uuid::new_v4()));
    xml.push_str("  </ResponseMetadata>\n");
    xml.push_str("</ListQueuesResponse>");

    xml
}

/// Build a GetQueueAttributes response.
pub fn build_get_queue_attributes_response(attributes: &[(String, String)]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0"?>"#);
    xml.push('\n');
    xml.push_str(r#"<GetQueueAttributesResponse xmlns="http://queue.amazonaws.com/doc/2012-11-05/">"#);
    xml.push('\n');
    xml.push_str("  <GetQueueAttributesResult>\n");

    for (name, value) in attributes {
        xml.push_str("    <Attribute>\n");
        xml.push_str(&format!("      <Name>{}</Name>\n", escape_xml(name)));
        xml.push_str(&format!("      <Value>{}</Value>\n", escape_xml(value)));
        xml.push_str("    </Attribute>\n");
    }

    xml.push_str("  </GetQueueAttributesResult>\n");
    xml.push_str("  <ResponseMetadata>\n");
    xml.push_str(&format!("    <RequestId>{}</RequestId>\n", Uuid::new_v4()));
    xml.push_str("  </ResponseMetadata>\n");
    xml.push_str("</GetQueueAttributesResponse>");

    xml
}

/// Information about a received message for XML response.
pub struct ReceivedMessageInfo {
    /// Message ID.
    pub message_id: String,
    /// Receipt handle.
    pub receipt_handle: String,
    /// MD5 of body.
    pub md5_of_body: String,
    /// Message body.
    pub body: String,
    /// System attributes.
    pub attributes: Vec<(String, String)>,
    /// Message attributes.
    pub message_attributes: Vec<(String, MessageAttributeInfo)>,
}

/// Message attribute information for XML response.
pub struct MessageAttributeInfo {
    /// Data type.
    pub data_type: String,
    /// String value.
    pub string_value: Option<String>,
}

/// Escape XML special characters.
pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Unescape XML entities.
pub fn unescape_xml(s: &str) -> String {
    // Note: Replace &amp; last to avoid double-unescaping
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(escape_xml("Hello <World>"), "Hello &lt;World&gt;");
        assert_eq!(escape_xml("A&B"), "A&amp;B");
    }

    #[test]
    fn test_create_queue_response() {
        let response = build_create_queue_response("http://localhost:9324/queue/test");
        assert!(response.contains("CreateQueueResponse"));
        assert!(response.contains("http://localhost:9324/queue/test"));
        assert!(response.contains("RequestId"));
    }

    #[test]
    fn test_error_response() {
        let response = build_error_response(
            SqsErrorCode::QueueDoesNotExist,
            "Queue does not exist",
        );
        assert!(response.contains("ErrorResponse"));
        assert!(response.contains("AWS.SimpleQueueService.NonExistentQueue"));
        assert!(response.contains("Queue does not exist"));
    }

    #[test]
    fn test_send_message_response() {
        let response = build_send_message_response(
            "msg-123",
            "abc123",
            Some("def456"),
        );
        assert!(response.contains("SendMessageResponse"));
        assert!(response.contains("msg-123"));
        assert!(response.contains("abc123"));
        assert!(response.contains("def456"));
    }

    #[test]
    fn test_list_queues_response() {
        let urls = vec![
            "http://localhost:9324/queue/q1".to_string(),
            "http://localhost:9324/queue/q2".to_string(),
        ];
        let response = build_list_queues_response(&urls);
        assert!(response.contains("ListQueuesResponse"));
        assert!(response.contains("queue/q1"));
        assert!(response.contains("queue/q2"));
    }
}
