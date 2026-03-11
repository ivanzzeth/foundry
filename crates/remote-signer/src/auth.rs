//! Ed25519-based request signing for remote-signer API authentication.
//!
//! Request signing format (nonce mode):
//! `{timestamp}|{nonce}|{method}|{path}|{sha256(body)}`
//!
//! Headers: X-API-Key-ID, X-Timestamp, X-Signature, X-Nonce

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

/// Generates the authentication signature for a remote-signer API request.
pub fn sign_request(
    signing_key: &SigningKey,
    method: &str,
    path: &str,
    timestamp: i64,
    nonce: &str,
    body: &[u8],
) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    let message = format!("{timestamp}|{nonce}|{method}|{path}|{body_hash}");
    let signature = signing_key.sign(message.as_bytes());
    hex::encode(signature.to_bytes())
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
        let sig1 = sign_request(&key, "POST", "/api/v1/sign", 1000, "nonce1", b"{}");
        let sig2 = sign_request(&key, "POST", "/api/v1/sign", 1000, "nonce1", b"{}");
        assert_eq!(sig1, sig2);
    }
}
