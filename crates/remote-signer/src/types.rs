use serde::{Deserialize, Serialize};

/// Sign request to the remote signer service.
#[derive(Debug, Clone, Serialize)]
pub struct SignRequest {
    /// The type of signing operation.
    pub sign_type: SignType,
    /// The chain ID for the signing operation.
    pub chain_id: String,
    /// The address of the signer.
    pub address: String,
    /// The data to sign (hex-encoded or JSON depending on sign_type).
    pub data: String,
    /// Optional metadata for the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
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
