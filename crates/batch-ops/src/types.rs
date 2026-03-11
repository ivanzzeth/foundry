use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

/// A single transfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    /// Recipient address.
    pub to: Address,
    /// Amount to transfer (in wei for native, raw units for ERC20).
    pub amount: U256,
}

/// Result of a single transfer in a distribute operation.
#[derive(Debug, Clone)]
pub struct TransferResult {
    /// The transfer that was attempted.
    pub transfer: Transfer,
    /// The transaction hash if successful.
    pub tx_hash: Option<alloy_primitives::TxHash>,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Summary of a distribute operation.
#[derive(Debug, Clone)]
pub struct DistributeResult {
    /// Total number of transfers attempted.
    pub total: usize,
    /// Number of successful transfers.
    pub succeeded: usize,
    /// Number of failed transfers.
    pub failed: usize,
    /// Individual transfer results.
    pub results: Vec<TransferResult>,
    /// Total amount distributed.
    pub total_amount: U256,
}

/// Summary of a collect operation.
#[derive(Debug, Clone)]
pub struct CollectResult {
    /// Total number of wallets swept.
    pub total: usize,
    /// Number of successful sweeps.
    pub succeeded: usize,
    /// Number of failed sweeps.
    pub failed: usize,
    /// Number of wallets skipped (insufficient balance).
    pub skipped: usize,
    /// Individual transfer results.
    pub results: Vec<TransferResult>,
    /// Total amount collected.
    pub total_amount: U256,
}

/// Asset type for batch operations.
#[derive(Debug, Clone)]
pub enum AssetType {
    /// Native token (ETH, MATIC, etc.)
    Native,
    /// ERC20 token with contract address.
    Erc20(Address),
}

/// Error types for batch operations.
#[derive(Debug, thiserror::Error)]
pub enum BatchOpsError {
    #[error("CSV parse error: {0}")]
    CsvParse(String),
    #[error("Invalid transfer: {0}")]
    InvalidTransfer(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Signer error: {0}")]
    Signer(String),
    #[error("Insufficient balance: have {have}, need {need}")]
    InsufficientBalance { have: U256, need: U256 },
    #[error("{0}")]
    Other(String),
}
