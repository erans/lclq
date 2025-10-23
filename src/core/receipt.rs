//! Receipt handle generation and parsing.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::MessageId;
use crate::{Error, Result};

/// Receipt handle data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptHandleData {
    /// Queue ID.
    pub queue_id: String,
    /// Message ID.
    pub message_id: MessageId,
    /// Nonce for security (prevents reuse).
    pub nonce: String,
}

/// Generate a receipt handle from queue ID and message ID.
pub fn generate_receipt_handle(queue_id: &str, message_id: &MessageId) -> String {
    let data = ReceiptHandleData {
        queue_id: queue_id.to_string(),
        message_id: message_id.clone(),
        nonce: Uuid::new_v4().to_string(),
    };

    let json = serde_json::to_string(&data).expect("Failed to serialize receipt handle");
    STANDARD.encode(json.as_bytes())
}

/// Parse a receipt handle and extract the data.
pub fn parse_receipt_handle(receipt_handle: &str) -> Result<ReceiptHandleData> {
    let decoded = STANDARD
        .decode(receipt_handle)
        .map_err(|_| Error::InvalidReceiptHandle)?;

    let json = String::from_utf8(decoded).map_err(|_| Error::InvalidReceiptHandle)?;

    let data: ReceiptHandleData =
        serde_json::from_str(&json).map_err(|_| Error::InvalidReceiptHandle)?;

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receipt_handle_roundtrip() {
        let queue_id = "test-queue";
        let message_id = MessageId::new();

        let handle = generate_receipt_handle(queue_id, &message_id);
        let parsed = parse_receipt_handle(&handle).unwrap();

        assert_eq!(parsed.queue_id, queue_id);
        assert_eq!(parsed.message_id, message_id);
        assert!(!parsed.nonce.is_empty());
    }

    #[test]
    fn test_parse_invalid_receipt_handle() {
        assert!(parse_receipt_handle("invalid").is_err());
        assert!(parse_receipt_handle("").is_err());
    }
}
