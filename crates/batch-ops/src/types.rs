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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_creation_and_field_access() {
        let addr: Address = "0x0000000000000000000000000000000000000001".parse().unwrap();
        let amount = U256::from(1_000_000u64);
        let transfer = Transfer { to: addr, amount };
        assert_eq!(transfer.to, addr);
        assert_eq!(transfer.amount, amount);
    }

    #[test]
    fn test_transfer_clone() {
        let addr: Address = "0x0000000000000000000000000000000000000042".parse().unwrap();
        let transfer = Transfer { to: addr, amount: U256::from(999u64) };
        let cloned = transfer.clone();
        assert_eq!(cloned.to, transfer.to);
        assert_eq!(cloned.amount, transfer.amount);
    }

    #[test]
    fn test_distribute_result_construction() {
        let result = DistributeResult {
            total: 3,
            succeeded: 2,
            failed: 1,
            results: vec![],
            total_amount: U256::from(5000u64),
        };
        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 1);
        assert!(result.results.is_empty());
        assert_eq!(result.total_amount, U256::from(5000u64));
    }

    #[test]
    fn test_collect_result_construction() {
        let result = CollectResult {
            total: 10,
            succeeded: 7,
            failed: 1,
            skipped: 2,
            results: vec![],
            total_amount: U256::from(42000u64),
        };
        assert_eq!(result.total, 10);
        assert_eq!(result.succeeded, 7);
        assert_eq!(result.failed, 1);
        assert_eq!(result.skipped, 2);
        assert!(result.results.is_empty());
        assert_eq!(result.total_amount, U256::from(42000u64));
    }

    #[test]
    fn test_transfer_result_with_tx_hash() {
        let addr: Address = "0x0000000000000000000000000000000000000001".parse().unwrap();
        let transfer = Transfer { to: addr, amount: U256::from(100u64) };
        let result = TransferResult {
            transfer,
            tx_hash: Some(alloy_primitives::TxHash::ZERO),
            error: None,
        };
        assert!(result.tx_hash.is_some());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_transfer_result_with_error() {
        let addr: Address = "0x0000000000000000000000000000000000000001".parse().unwrap();
        let transfer = Transfer { to: addr, amount: U256::from(100u64) };
        let result = TransferResult {
            transfer,
            tx_hash: None,
            error: Some("tx reverted".to_string()),
        };
        assert!(result.tx_hash.is_none());
        assert_eq!(result.error.as_deref(), Some("tx reverted"));
    }

    #[test]
    fn test_asset_type_native() {
        let asset = AssetType::Native;
        assert!(matches!(asset, AssetType::Native));
    }

    #[test]
    fn test_asset_type_erc20() {
        let addr: Address = "0x00000000000000000000000000000000DeaDBeef".parse().unwrap();
        let asset = AssetType::Erc20(addr);
        match asset {
            AssetType::Erc20(a) => assert_eq!(a, addr),
            _ => panic!("expected Erc20 variant"),
        }
    }

    #[test]
    fn test_batch_ops_error_display_csv_parse() {
        let err = BatchOpsError::CsvParse("bad format".to_string());
        assert_eq!(err.to_string(), "CSV parse error: bad format");
    }

    #[test]
    fn test_batch_ops_error_display_invalid_transfer() {
        let err = BatchOpsError::InvalidTransfer("zero amount".to_string());
        assert_eq!(err.to_string(), "Invalid transfer: zero amount");
    }

    #[test]
    fn test_batch_ops_error_display_provider() {
        let err = BatchOpsError::Provider("connection refused".to_string());
        assert_eq!(err.to_string(), "Provider error: connection refused");
    }

    #[test]
    fn test_batch_ops_error_display_signer() {
        let err = BatchOpsError::Signer("key not found".to_string());
        assert_eq!(err.to_string(), "Signer error: key not found");
    }

    #[test]
    fn test_batch_ops_error_display_insufficient_balance() {
        let err = BatchOpsError::InsufficientBalance {
            have: U256::from(100u64),
            need: U256::from(200u64),
        };
        assert_eq!(err.to_string(), "Insufficient balance: have 100, need 200");
    }

    #[test]
    fn test_batch_ops_error_display_other() {
        let err = BatchOpsError::Other("something went wrong".to_string());
        assert_eq!(err.to_string(), "something went wrong");
    }
}
