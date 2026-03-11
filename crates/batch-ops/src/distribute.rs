//! Distribute (send tokens from one wallet to many recipients).

use crate::types::*;
use alloy_network::AnyNetwork;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_sol_types::{sol, SolCall};
use tracing::{info, warn};

// ERC20 transfer function
sol! {
    function transfer(address to, uint256 amount) external returns (bool);
}

/// Executes a native token distribute operation.
///
/// Sends individual transactions from the sender to each recipient.
/// V1 implementation: sequential transactions (no batching contract).
pub async fn distribute_native<P: Provider<AnyNetwork>>(
    provider: &P,
    transfers: &[Transfer],
    dry_run: bool,
) -> eyre::Result<DistributeResult> {
    let mut results = Vec::with_capacity(transfers.len());
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut total_amount = U256::ZERO;

    for transfer in transfers {
        if dry_run {
            info!(
                to = ?transfer.to,
                amount = %transfer.amount,
                "[DRY RUN] Would transfer native token"
            );
            results.push(TransferResult {
                transfer: transfer.clone(),
                tx_hash: None,
                error: None,
            });
            total_amount += transfer.amount;
            succeeded += 1;
            continue;
        }

        let tx = WithOtherFields::new(
            TransactionRequest::default()
                .to(transfer.to)
                .value(transfer.amount),
        );

        match provider.send_transaction(tx).await {
            Ok(pending) => {
                let tx_hash = *pending.inner().tx_hash();
                info!(
                    to = ?transfer.to,
                    amount = %transfer.amount,
                    tx_hash = ?tx_hash,
                    "Transfer sent"
                );
                results.push(TransferResult {
                    transfer: transfer.clone(),
                    tx_hash: Some(tx_hash),
                    error: None,
                });
                total_amount += transfer.amount;
                succeeded += 1;
            }
            Err(e) => {
                warn!(
                    to = ?transfer.to,
                    amount = %transfer.amount,
                    error = %e,
                    "Transfer failed"
                );
                results.push(TransferResult {
                    transfer: transfer.clone(),
                    tx_hash: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    Ok(DistributeResult {
        total: transfers.len(),
        succeeded,
        failed,
        results,
        total_amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_call_selector() {
        // ERC20 transfer(address,uint256) selector is 0xa9059cbb
        let call = transferCall {
            to: Address::ZERO,
            amount: U256::ZERO,
        };
        let encoded = call.abi_encode();
        // First 4 bytes are the function selector
        assert_eq!(
            &encoded[..4],
            &[0xa9, 0x05, 0x9c, 0xbb],
            "transfer selector should be 0xa9059cbb"
        );
    }

    #[test]
    fn test_transfer_call_encoding_length() {
        let call = transferCall {
            to: Address::ZERO,
            amount: U256::from(1000u64),
        };
        let encoded = call.abi_encode();
        // 4 bytes selector + 32 bytes address + 32 bytes uint256
        assert_eq!(encoded.len(), 4 + 32 + 32);
    }
}

/// Executes an ERC20 token distribute operation.
///
/// Sends individual ERC20 transfer transactions to each recipient.
pub async fn distribute_erc20<P: Provider<AnyNetwork>>(
    provider: &P,
    token: Address,
    transfers: &[Transfer],
    dry_run: bool,
) -> eyre::Result<DistributeResult> {
    let mut results = Vec::with_capacity(transfers.len());
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut total_amount = U256::ZERO;

    for transfer in transfers {
        if dry_run {
            info!(
                token = ?token,
                to = ?transfer.to,
                amount = %transfer.amount,
                "[DRY RUN] Would transfer ERC20 token"
            );
            results.push(TransferResult {
                transfer: transfer.clone(),
                tx_hash: None,
                error: None,
            });
            total_amount += transfer.amount;
            succeeded += 1;
            continue;
        }

        let calldata = transferCall {
            to: transfer.to,
            amount: transfer.amount,
        }
        .abi_encode();

        let tx = WithOtherFields::new(
            TransactionRequest::default()
                .to(token)
                .input(Bytes::from(calldata).into()),
        );

        match provider.send_transaction(tx).await {
            Ok(pending) => {
                let tx_hash = *pending.inner().tx_hash();
                info!(
                    token = ?token,
                    to = ?transfer.to,
                    amount = %transfer.amount,
                    tx_hash = ?tx_hash,
                    "ERC20 transfer sent"
                );
                results.push(TransferResult {
                    transfer: transfer.clone(),
                    tx_hash: Some(tx_hash),
                    error: None,
                });
                total_amount += transfer.amount;
                succeeded += 1;
            }
            Err(e) => {
                warn!(
                    token = ?token,
                    to = ?transfer.to,
                    amount = %transfer.amount,
                    error = %e,
                    "ERC20 transfer failed"
                );
                results.push(TransferResult {
                    transfer: transfer.clone(),
                    tx_hash: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    Ok(DistributeResult {
        total: transfers.len(),
        succeeded,
        failed,
        results,
        total_amount,
    })
}
