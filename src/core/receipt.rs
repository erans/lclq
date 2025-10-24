//! Receipt handle generation and parsing with HMAC signatures.

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::types::MessageId;
use crate::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

/// Receipt handle data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptHandleData {
    /// Queue ID.
    pub queue_id: String,
    /// Message ID.
    pub message_id: MessageId,
    /// Nonce for security (prevents reuse).
    pub nonce: String,
    /// HMAC signature (hex-encoded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Get the secret key for HMAC signing.
/// Uses LCLQ_RECEIPT_SECRET environment variable.
///
/// SECURITY: For production use, ALWAYS set LCLQ_RECEIPT_SECRET to a strong random value.
/// The function will panic on first use if the environment variable is not set,
/// forcing explicit configuration rather than using a predictable default.
fn get_secret_key() -> Vec<u8> {
    std::env::var("LCLQ_RECEIPT_SECRET")
        .expect(
            "LCLQ_RECEIPT_SECRET environment variable must be set. \
            For security, receipt handles require a secret key for HMAC signatures. \
            Generate a strong random secret: openssl rand -hex 32"
        )
        .into_bytes()
}

/// Generate a receipt handle from queue ID and message ID.
pub fn generate_receipt_handle(queue_id: &str, message_id: &MessageId) -> String {
    // Create data without signature first
    let mut data = ReceiptHandleData {
        queue_id: queue_id.to_string(),
        message_id: message_id.clone(),
        nonce: Uuid::new_v4().to_string(),
        signature: None,
    };

    // Serialize without signature to compute HMAC
    let json_without_sig = serde_json::to_string(&data).expect("Failed to serialize receipt handle");

    // Compute HMAC signature
    let secret_key = get_secret_key();
    let mut mac = HmacSha256::new_from_slice(&secret_key)
        .expect("HMAC can take key of any size");
    mac.update(json_without_sig.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    // Add signature to data
    data.signature = Some(signature);

    // Serialize with signature and base64 encode
    let json_with_sig = serde_json::to_string(&data).expect("Failed to serialize receipt handle");
    STANDARD.encode(json_with_sig.as_bytes())
}

/// Parse a receipt handle and extract the data.
pub fn parse_receipt_handle(receipt_handle: &str) -> Result<ReceiptHandleData> {
    // Base64 decode
    let decoded = STANDARD
        .decode(receipt_handle)
        .map_err(|_| Error::InvalidReceiptHandle)?;

    // Deserialize JSON
    let json = String::from_utf8(decoded).map_err(|_| Error::InvalidReceiptHandle)?;

    let mut data: ReceiptHandleData =
        serde_json::from_str(&json).map_err(|_| Error::InvalidReceiptHandle)?;

    // Extract and verify signature
    let provided_signature = data
        .signature
        .take()
        .ok_or(Error::InvalidReceiptHandle)?;

    // Serialize without signature to recompute HMAC
    let json_without_sig = serde_json::to_string(&data)
        .map_err(|_| Error::InvalidReceiptHandle)?;

    // Compute expected HMAC
    let secret_key = get_secret_key();
    let mut mac = HmacSha256::new_from_slice(&secret_key)
        .expect("HMAC can take key of any size");
    mac.update(json_without_sig.as_bytes());

    // Verify signature (constant-time comparison)
    let expected_signature_bytes = mac.finalize().into_bytes();
    let provided_signature_bytes = hex::decode(&provided_signature)
        .map_err(|_| Error::InvalidReceiptHandle)?;

    // Use constant-time comparison to prevent timing attacks
    if expected_signature_bytes.len() != provided_signature_bytes.len() {
        return Err(Error::InvalidReceiptHandle);
    }

    let mut diff = 0u8;
    for (a, b) in expected_signature_bytes.iter().zip(provided_signature_bytes.iter()) {
        diff |= a ^ b;
    }

    if diff != 0 {
        return Err(Error::InvalidReceiptHandle);
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up test environment with required secret key
    fn setup_test_env() {
        // SAFETY: This is safe in tests because:
        // 1. Tests run sequentially by default (not parallel for this module)
        // 2. We set the variable before any code reads it
        // 3. The value doesn't change after being set
        unsafe {
            std::env::set_var("LCLQ_RECEIPT_SECRET", "test-secret-key-for-testing-only");
        }
    }

    #[test]
    fn test_receipt_handle_roundtrip() {
        setup_test_env();

        let queue_id = "test-queue";
        let message_id = MessageId::new();

        let handle = generate_receipt_handle(queue_id, &message_id);
        let parsed = parse_receipt_handle(&handle).unwrap();

        assert_eq!(parsed.queue_id, queue_id);
        assert_eq!(parsed.message_id, message_id);
        assert!(!parsed.nonce.is_empty());
        // Signature should be None after parsing (it's removed during verification)
        assert!(parsed.signature.is_none());
    }

    #[test]
    fn test_parse_invalid_receipt_handle() {
        setup_test_env();

        assert!(parse_receipt_handle("invalid").is_err());
        assert!(parse_receipt_handle("").is_err());
    }

    #[test]
    fn test_forged_receipt_handle_rejected() {
        setup_test_env();

        // Create a valid receipt handle
        let queue_id = "test-queue";
        let message_id = MessageId::new();
        let valid_handle = generate_receipt_handle(queue_id, &message_id);

        // Decode it
        let decoded = STANDARD.decode(&valid_handle).unwrap();
        let json = String::from_utf8(decoded).unwrap();
        let mut data: ReceiptHandleData = serde_json::from_str(&json).unwrap();

        // Tamper with the queue_id (forgery attempt)
        data.queue_id = "different-queue".to_string();

        // Re-encode without updating signature
        let forged_json = serde_json::to_string(&data).unwrap();
        let forged_handle = STANDARD.encode(forged_json.as_bytes());

        // Should be rejected due to invalid signature
        assert!(parse_receipt_handle(&forged_handle).is_err());
    }

    #[test]
    fn test_receipt_handle_without_signature_rejected() {
        setup_test_env();

        // Create data without signature
        let data = ReceiptHandleData {
            queue_id: "test-queue".to_string(),
            message_id: MessageId::new(),
            nonce: Uuid::new_v4().to_string(),
            signature: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        let handle = STANDARD.encode(json.as_bytes());

        // Should be rejected because signature is missing
        assert!(parse_receipt_handle(&handle).is_err());
    }
}
