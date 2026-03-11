use serde::{Deserialize, Serialize};

/// Cobo environment selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoboEnv {
    /// Production environment.
    Prod,
    /// Development/sandbox environment.
    Dev,
}

impl CoboEnv {
    pub fn base_url(&self) -> &'static str {
        match self {
            CoboEnv::Prod => "https://api.cobo.com",
            CoboEnv::Dev => "https://api.dev.cobo.com",
        }
    }
}

impl std::str::FromStr for CoboEnv {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "prod" | "production" => Ok(CoboEnv::Prod),
            "dev" | "development" | "sandbox" => Ok(CoboEnv::Dev),
            _ => Err(eyre::eyre!("Invalid Cobo environment: '{}'. Use 'prod' or 'dev'", s)),
        }
    }
}

/// Transaction status from Cobo API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    #[serde(rename = "Submitted")]
    Submitted,
    #[serde(rename = "PendingScreening")]
    PendingScreening,
    #[serde(rename = "PendingAuthorization")]
    PendingAuthorization,
    #[serde(rename = "PendingApproval")]
    PendingApproval,
    #[serde(rename = "PendingSignature")]
    PendingSignature,
    #[serde(rename = "Broadcasting")]
    Broadcasting,
    #[serde(rename = "Confirming")]
    Confirming,
    #[serde(rename = "Completed")]
    Completed,
    #[serde(rename = "Failed")]
    Failed,
    #[serde(rename = "Rejected")]
    Rejected,
    #[serde(other)]
    Unknown,
}

impl TransactionStatus {
    /// Returns true if the transaction is in a final state.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Rejected)
    }

    /// Returns true if the transaction has been broadcast (confirming or completed).
    pub fn is_broadcast(&self) -> bool {
        matches!(self, Self::Confirming | Self::Completed)
    }
}

/// Message sign destination type for Cobo API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum MessageSignDestType {
    EVM_EIP_191,
    EVM_EIP_712,
}

impl std::fmt::Display for MessageSignDestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageSignDestType::EVM_EIP_191 => write!(f, "EVM_EIP_191"),
            MessageSignDestType::EVM_EIP_712 => write!(f, "EVM_EIP_712"),
        }
    }
}

/// Contract call destination for Cobo API.
#[derive(Debug, Clone, Serialize)]
pub struct ContractCallDestination {
    pub destination_type: String,
    pub account_output: AccountOutput,
    pub calldata: String,
    pub amount: Option<String>,
}

/// Account output for contract call.
#[derive(Debug, Clone, Serialize)]
pub struct AccountOutput {
    pub address: String,
    pub memo: Option<String>,
}

/// Fee configuration for contract calls.
#[derive(Debug, Clone, Serialize)]
pub struct TransactionRequestFee {
    pub fee_type: String,
    pub token_id: String,
    pub gas_price: Option<String>,
    pub gas_limit: Option<String>,
    pub max_fee: Option<String>,
    pub max_priority_fee: Option<String>,
}

/// Message sign request body.
#[derive(Debug, Clone, Serialize)]
pub struct MessageSignRequest {
    pub request_id: String,
    pub chain_id: String,
    pub source: TransactionSource,
    pub destination: MessageSignDestination,
    pub description: Option<String>,
}

/// Transaction source (wallet).
#[derive(Debug, Clone, Serialize)]
pub struct TransactionSource {
    pub source_type: String,
    pub wallet_id: String,
    pub address: String,
}

/// Message sign destination.
#[derive(Debug, Clone, Serialize)]
pub struct MessageSignDestination {
    pub destination_type: String,
    pub structured_data: Option<StructuredData>,
    pub message: Option<String>,
}

/// Structured data for EIP-712 signing.
#[derive(Debug, Clone, Serialize)]
pub struct StructuredData {
    #[serde(rename = "type")]
    pub data_type: String,
    pub data: String,
}

/// Contract call request body.
#[derive(Debug, Clone, Serialize)]
pub struct ContractCallRequest {
    pub request_id: String,
    pub chain_id: String,
    pub source: TransactionSource,
    pub destination: ContractCallDestination,
    pub fee: TransactionRequestFee,
    pub description: Option<String>,
}

/// Response from creating a transaction.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateTransactionResponse {
    pub transaction_id: String,
}

/// Transaction detail from Cobo API.
#[derive(Debug, Clone, Deserialize)]
pub struct TransactionDetail {
    pub transaction_id: String,
    pub status: TransactionStatus,
    pub transaction_hash: Option<String>,
    pub signature: Option<String>,
    #[serde(default)]
    pub failed_reason: Option<String>,
}

/// Cobo API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct CoboApiError {
    pub error_code: Option<i64>,
    pub error_message: Option<String>,
    pub error_id: Option<String>,
}

impl std::fmt::Display for CoboApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cobo API error: code={}, message={}",
            self.error_code.unwrap_or(0),
            self.error_message.as_deref().unwrap_or("unknown")
        )
    }
}

/// Error types for Cobo MPC operations.
#[derive(Debug, thiserror::Error)]
pub enum CoboMpcError {
    #[error("Cobo API error: {0}")]
    Api(CoboApiError),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Transaction failed: {reason}")]
    TransactionFailed { reason: String },
    #[error("Transaction rejected")]
    TransactionRejected,
    #[error("Transaction polling timeout after {retries} retries")]
    PollingTimeout { retries: u32 },
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Missing transaction hash in response")]
    MissingTransactionHash,
    #[error("{0}")]
    Other(String),
}
