//! Ed25519-based request signing for remote-signer API authentication.
//!
//! Request signing format (matches Go reference and server implementation):
//! `{timestamp_ms}|{method}|{path}|{sha256(body)}`
//!
//! Headers: X-API-Key-ID, X-Timestamp, X-Signature
//! - Timestamp is in **milliseconds** since epoch
//! - Signature is **base64** encoded

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

/// Generates the authentication signature for a remote-signer API request.
///
/// Format: `{timestamp_ms}|{method}|{path}|{sha256(body)}`
/// Returns base64-encoded Ed25519 signature.
pub fn sign_request(
    signing_key: &SigningKey,
    method: &str,
    path: &str,
    timestamp_ms: i64,
    body: &[u8],
) -> String {
    let body_hash = Sha256::digest(body);
    // Use lowercase hex for body hash to match Go's %x format
    let message = format!("{timestamp_ms}|{method}|{path}|{:x}", body_hash);
    let signature = signing_key.sign(message.as_bytes());
    BASE64.encode(signature.to_bytes())
}

/// Parses a hex-encoded Ed25519 private key into a SigningKey.
pub fn parse_signing_key(hex_key: &str) -> eyre::Result<SigningKey> {
    let key_bytes = hex::decode(hex_key.strip_prefix("0x").unwrap_or(hex_key))?;
    if key_bytes.len() != 32 {
        return Err(eyre::eyre!(
            "Invalid Ed25519 private key length: expected 32 bytes, got {}",
            key_bytes.len()
        ));
    }
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    Ok(SigningKey::from_bytes(&key_array))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_signing_key() {
        let key = SigningKey::generate(&mut rand::thread_rng());
        let hex_key = hex::encode(key.to_bytes());
        let parsed = parse_signing_key(&hex_key).unwrap();
        assert_eq!(key.to_bytes(), parsed.to_bytes());
    }

    #[test]
    fn test_sign_request_deterministic() {
        let key = parse_signing_key(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        let sig1 = sign_request(&key, "POST", "/api/v1/evm/sign", 1000, b"{}");
        let sig2 = sign_request(&key, "POST", "/api/v1/evm/sign", 1000, b"{}");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_sign_request_is_base64() {
        let key = parse_signing_key(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        let sig = sign_request(&key, "POST", "/api/v1/evm/sign", 1000, b"{}");
        // Should be valid base64
        assert!(BASE64.decode(&sig).is_ok(), "signature should be valid base64");
    }
}
