use serde::{Deserialize, Serialize};

/// Sign request to the remote signer service.
/// Matches Go SDK: ChainID, SignerAddress, SignType, Payload
#[derive(Debug, Clone, Serialize)]
pub struct SignRequest {
    /// The chain ID for the signing operation.
    pub chain_id: String,
    /// The address of the signer (matches Go field: signer_address).
    pub signer_address: String,
    /// The type of signing operation.
    pub sign_type: SignType,
    /// The payload to sign (JSON object, structure depends on sign_type).
    pub payload: serde_json::Value,
}

/// Supported sign types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignType {
    /// Pre-hashed data (32 bytes).
    Hash,
    /// Raw bytes message.
    RawMessage,
    /// EIP-191 formatted message.
    Eip191,
    /// personal_sign (EIP-191 0x45).
    Personal,
    /// EIP-712 typed data.
    TypedData,
    /// Full Ethereum transaction.
    Transaction,
}

/// Response from a sign request.
#[derive(Debug, Clone, Deserialize)]
pub struct SignResponse {
    /// The request ID for tracking.
    pub request_id: String,
    /// The current status of the request.
    pub status: RequestStatus,
    /// The signature (hex-encoded, present when completed).
    #[serde(default)]
    pub signature: Option<String>,
    /// Error message if failed.
    #[serde(default)]
    pub error: Option<String>,
}

/// Status of a signing request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    /// Awaiting processing.
    Pending,
    /// Under review / authorization.
    Authorizing,
    /// Being signed.
    Signing,
    /// Successfully completed.
    Completed,
    /// Rejected by policy.
    Rejected,
    /// Failed during signing.
    Failed,
}

impl RequestStatus {
    /// Returns true if the status is final (no more transitions).
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Completed | Self::Rejected | Self::Failed)
    }
}

/// Error types for remote signer operations.
#[derive(Debug, thiserror::Error)]
pub enum RemoteSignerError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Sign request rejected: {reason}")]
    Rejected { reason: String },
    #[error("Sign request failed: {reason}")]
    Failed { reason: String },
    #[error("Polling timeout after {elapsed_secs}s")]
    PollingTimeout { elapsed_secs: u64 },
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Server error: {status} {body}")]
    ServerError { status: u16, body: String },
    #[error("{0}")]
    Other(String),
}

/// Health check response.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SignType serialization ---

    #[test]
    fn sign_type_serializes_to_snake_case() {
        let cases = vec![
            (SignType::Hash, "\"hash\""),
            (SignType::RawMessage, "\"raw_message\""),
            (SignType::Eip191, "\"eip191\""),
            (SignType::Personal, "\"personal\""),
            (SignType::TypedData, "\"typed_data\""),
            (SignType::Transaction, "\"transaction\""),
        ];
        for (variant, expected) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected, "SignType::{variant:?} serialization mismatch");
        }
    }

    #[test]
    fn sign_type_round_trips() {
        let variants = vec![
            SignType::Hash,
            SignType::RawMessage,
            SignType::Eip191,
            SignType::Personal,
            SignType::TypedData,
            SignType::Transaction,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: SignType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    // --- RequestStatus::is_final ---

    #[test]
    fn request_status_is_final_for_terminal_states() {
        assert!(RequestStatus::Completed.is_final());
        assert!(RequestStatus::Rejected.is_final());
        assert!(RequestStatus::Failed.is_final());
    }

    #[test]
    fn request_status_is_not_final_for_non_terminal_states() {
        assert!(!RequestStatus::Pending.is_final());
        assert!(!RequestStatus::Authorizing.is_final());
        assert!(!RequestStatus::Signing.is_final());
    }

    // --- RemoteSignerError Display ---

    #[test]
    fn error_display_http() {
        // Build a reqwest error by constructing a client with an invalid URL
        let client = reqwest::Client::new();
        let err = client
            .get("http://not a valid url")
            .build()
            .unwrap_err();
        let remote_err = RemoteSignerError::Http(err);
        let msg = format!("{remote_err}");
        assert!(msg.starts_with("HTTP request failed:"), "got: {msg}");
    }

    #[test]
    fn error_display_rejected() {
        let err = RemoteSignerError::Rejected {
            reason: "policy violation".to_string(),
        };
        assert_eq!(format!("{err}"), "Sign request rejected: policy violation");
    }

    #[test]
    fn error_display_failed() {
        let err = RemoteSignerError::Failed {
            reason: "key unavailable".to_string(),
        };
        assert_eq!(format!("{err}"), "Sign request failed: key unavailable");
    }

    #[test]
    fn error_display_polling_timeout() {
        let err = RemoteSignerError::PollingTimeout { elapsed_secs: 300 };
        assert_eq!(format!("{err}"), "Polling timeout after 300s");
    }

    #[test]
    fn error_display_invalid_signature() {
        let err = RemoteSignerError::InvalidSignature("bad hex".to_string());
        assert_eq!(format!("{err}"), "Invalid signature: bad hex");
    }

    #[test]
    fn error_display_server_error() {
        let err = RemoteSignerError::ServerError {
            status: 500,
            body: "internal error".to_string(),
        };
        assert_eq!(format!("{err}"), "Server error: 500 internal error");
    }

    #[test]
    fn error_display_other() {
        let err = RemoteSignerError::Other("something went wrong".to_string());
        assert_eq!(format!("{err}"), "something went wrong");
    }
}
