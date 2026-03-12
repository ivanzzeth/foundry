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
/// Matches Go SDK: NewEvmContractCallDestination(destinationType, address, calldata)
#[derive(Debug, Clone, Serialize)]
pub struct ContractCallDestination {
    pub destination_type: String,
    /// Target contract address (direct field, not nested in account_output)
    pub address: String,
    pub calldata: String,
    /// Optional value for payable calls (wei as string)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
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

/// Transfer source for Cobo API.
#[derive(Debug, Clone, Serialize)]
pub struct TransferSource {
    pub source_type: String,
    pub wallet_id: String,
    pub address: String,
}

/// Transfer destination for Cobo API.
#[derive(Debug, Clone, Serialize)]
pub struct TransferDestination {
    pub destination_type: String,
    pub account_output: TransferAccountOutput,
}

/// Account output for transfer.
#[derive(Debug, Clone, Serialize)]
pub struct TransferAccountOutput {
    pub address: String,
    pub amount: String,
}

/// Transfer request body for /v2/transactions/transfer API.
#[derive(Debug, Clone, Serialize)]
pub struct TransferRequest {
    pub request_id: String,
    pub source: TransferSource,
    pub token_id: String,
    pub destination: TransferDestination,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<TransactionRequestFee>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- CoboEnv::from_str ---

    #[test]
    fn test_cobo_env_from_str_prod() {
        assert_eq!("prod".parse::<CoboEnv>().unwrap(), CoboEnv::Prod);
    }

    #[test]
    fn test_cobo_env_from_str_production() {
        assert_eq!("production".parse::<CoboEnv>().unwrap(), CoboEnv::Prod);
    }

    #[test]
    fn test_cobo_env_from_str_dev() {
        assert_eq!("dev".parse::<CoboEnv>().unwrap(), CoboEnv::Dev);
    }

    #[test]
    fn test_cobo_env_from_str_development() {
        assert_eq!("development".parse::<CoboEnv>().unwrap(), CoboEnv::Dev);
    }

    #[test]
    fn test_cobo_env_from_str_sandbox() {
        assert_eq!("sandbox".parse::<CoboEnv>().unwrap(), CoboEnv::Dev);
    }

    #[test]
    fn test_cobo_env_from_str_case_insensitive() {
        assert_eq!("PROD".parse::<CoboEnv>().unwrap(), CoboEnv::Prod);
        assert_eq!("Prod".parse::<CoboEnv>().unwrap(), CoboEnv::Prod);
        assert_eq!("DEV".parse::<CoboEnv>().unwrap(), CoboEnv::Dev);
        assert_eq!("Dev".parse::<CoboEnv>().unwrap(), CoboEnv::Dev);
    }

    #[test]
    fn test_cobo_env_from_str_invalid() {
        assert!("staging".parse::<CoboEnv>().is_err());
        assert!("".parse::<CoboEnv>().is_err());
        assert!("test".parse::<CoboEnv>().is_err());
    }

    // --- CoboEnv::base_url ---

    #[test]
    fn test_cobo_env_base_url_prod() {
        assert_eq!(CoboEnv::Prod.base_url(), "https://api.cobo.com");
    }

    #[test]
    fn test_cobo_env_base_url_dev() {
        assert_eq!(CoboEnv::Dev.base_url(), "https://api.dev.cobo.com");
    }

    // --- TransactionStatus::is_final ---

    #[test]
    fn test_transaction_status_is_final() {
        assert!(TransactionStatus::Completed.is_final());
        assert!(TransactionStatus::Failed.is_final());
        assert!(TransactionStatus::Rejected.is_final());
    }

    #[test]
    fn test_transaction_status_is_not_final() {
        assert!(!TransactionStatus::Submitted.is_final());
        assert!(!TransactionStatus::PendingScreening.is_final());
        assert!(!TransactionStatus::PendingAuthorization.is_final());
        assert!(!TransactionStatus::PendingApproval.is_final());
        assert!(!TransactionStatus::PendingSignature.is_final());
        assert!(!TransactionStatus::Broadcasting.is_final());
        assert!(!TransactionStatus::Confirming.is_final());
        assert!(!TransactionStatus::Unknown.is_final());
    }

    // --- TransactionStatus::is_broadcast ---

    #[test]
    fn test_transaction_status_is_broadcast() {
        assert!(TransactionStatus::Confirming.is_broadcast());
        assert!(TransactionStatus::Completed.is_broadcast());
    }

    #[test]
    fn test_transaction_status_is_not_broadcast() {
        assert!(!TransactionStatus::Submitted.is_broadcast());
        assert!(!TransactionStatus::PendingScreening.is_broadcast());
        assert!(!TransactionStatus::PendingAuthorization.is_broadcast());
        assert!(!TransactionStatus::PendingApproval.is_broadcast());
        assert!(!TransactionStatus::PendingSignature.is_broadcast());
        assert!(!TransactionStatus::Broadcasting.is_broadcast());
        assert!(!TransactionStatus::Failed.is_broadcast());
        assert!(!TransactionStatus::Rejected.is_broadcast());
        assert!(!TransactionStatus::Unknown.is_broadcast());
    }

    // --- CoboApiError::Display ---

    #[test]
    fn test_cobo_api_error_display_full() {
        let err = CoboApiError {
            error_code: Some(12001),
            error_message: Some("Invalid API key".to_string()),
            error_id: Some("abc123".to_string()),
        };
        let display = format!("{err}");
        assert!(display.contains("12001"));
        assert!(display.contains("Invalid API key"));
    }

    #[test]
    fn test_cobo_api_error_display_defaults() {
        let err = CoboApiError {
            error_code: None,
            error_message: None,
            error_id: None,
        };
        let display = format!("{err}");
        assert!(display.contains("0"));
        assert!(display.contains("unknown"));
    }

    // --- CoboMpcError::Display ---

    #[test]
    fn test_cobo_mpc_error_display_api() {
        let api_err = CoboApiError {
            error_code: Some(500),
            error_message: Some("internal".to_string()),
            error_id: None,
        };
        let err = CoboMpcError::Api(api_err);
        let display = format!("{err}");
        assert!(display.contains("Cobo API error"));
    }

    #[test]
    fn test_cobo_mpc_error_display_transaction_failed() {
        let err = CoboMpcError::TransactionFailed {
            reason: "insufficient funds".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("Transaction failed"));
        assert!(display.contains("insufficient funds"));
    }

    #[test]
    fn test_cobo_mpc_error_display_transaction_rejected() {
        let err = CoboMpcError::TransactionRejected;
        let display = format!("{err}");
        assert!(display.contains("Transaction rejected"));
    }

    #[test]
    fn test_cobo_mpc_error_display_polling_timeout() {
        let err = CoboMpcError::PollingTimeout { retries: 50 };
        let display = format!("{err}");
        assert!(display.contains("50"));
        assert!(display.contains("timeout"));
    }

    #[test]
    fn test_cobo_mpc_error_display_invalid_signature() {
        let err = CoboMpcError::InvalidSignature("too short".to_string());
        let display = format!("{err}");
        assert!(display.contains("Invalid signature"));
        assert!(display.contains("too short"));
    }

    #[test]
    fn test_cobo_mpc_error_display_missing_tx_hash() {
        let err = CoboMpcError::MissingTransactionHash;
        let display = format!("{err}");
        assert!(display.contains("Missing transaction hash"));
    }

    #[test]
    fn test_cobo_mpc_error_display_other() {
        let err = CoboMpcError::Other("custom error".to_string());
        let display = format!("{err}");
        assert!(display.contains("custom error"));
    }

    // --- TransactionStatus serde ---

    #[test]
    fn test_transaction_status_deserialize_known() {
        let s: TransactionStatus = serde_json::from_str("\"Completed\"").unwrap();
        assert_eq!(s, TransactionStatus::Completed);
    }

    #[test]
    fn test_transaction_status_deserialize_unknown() {
        let s: TransactionStatus = serde_json::from_str("\"SomeNewStatus\"").unwrap();
        assert_eq!(s, TransactionStatus::Unknown);
    }

    // --- MessageSignDestType::Display ---

    #[test]
    fn test_message_sign_dest_type_display() {
        assert_eq!(format!("{}", MessageSignDestType::EVM_EIP_191), "EVM_EIP_191");
        assert_eq!(format!("{}", MessageSignDestType::EVM_EIP_712), "EVM_EIP_712");
    }
}
