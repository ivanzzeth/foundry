//! Collect/sweep tokens from multiple wallets to a single destination.

use crate::types::*;
use alloy_network::{AnyNetwork, EthereumWallet};
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English};
use alloy_sol_types::{sol, SolCall};
use tracing::{debug, info, warn};

// ERC20 functions
sol! {
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

/// Gas safety margin multiplier numerator (1.2x = 6/5).
const GAS_MARGIN_NUM: u64 = 6;
/// Gas safety margin multiplier denominator.
const GAS_MARGIN_DEN: u64 = 5;
/// Default gas limit for native transfers.
const NATIVE_TRANSFER_GAS: u64 = 21000;

/// Collects native tokens from mnemonic-derived wallets to a destination.
pub async fn collect_native_from_mnemonic<P: Provider<AnyNetwork>>(
    provider: &P,
    destination: Address,
    mnemonic: &str,
    passphrase: Option<&str>,
    start_index: u32,
    end_index: u32,
    dry_run: bool,
) -> eyre::Result<CollectResult> {
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_amount = U256::ZERO;

    for index in start_index..=end_index {
        let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
        if let Some(pass) = passphrase {
            builder = builder.password(pass);
        }
        let signer = builder.index(index)?.build()?;
        let address = signer.address();

        match sweep_native_single(provider, &signer, address, destination, dry_run).await {
            Ok(Some(result)) => {
                total_amount += result.transfer.amount;
                if result.error.is_some() {
                    failed += 1;
                } else {
                    succeeded += 1;
                }
                results.push(result);
            }
            Ok(None) => {
                debug!(address = ?address, index = index, "Skipped: insufficient balance");
                skipped += 1;
            }
            Err(e) => {
                warn!(address = ?address, index = index, error = %e, "Failed to sweep");
                results.push(TransferResult {
                    transfer: Transfer { to: destination, amount: U256::ZERO },
                    tx_hash: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    let total = (end_index - start_index + 1) as usize;
    Ok(CollectResult {
        total,
        succeeded,
        failed,
        skipped,
        results,
        total_amount,
    })
}

/// Collects ERC20 tokens from mnemonic-derived wallets to a destination.
pub async fn collect_erc20_from_mnemonic<P: Provider<AnyNetwork>>(
    provider: &P,
    token: Address,
    destination: Address,
    mnemonic: &str,
    passphrase: Option<&str>,
    start_index: u32,
    end_index: u32,
    dry_run: bool,
) -> eyre::Result<CollectResult> {
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_amount = U256::ZERO;

    for index in start_index..=end_index {
        let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
        if let Some(pass) = passphrase {
            builder = builder.password(pass);
        }
        let signer = builder.index(index)?.build()?;
        let address = signer.address();

        match sweep_erc20_single(provider, &signer, address, token, destination, dry_run).await {
            Ok(Some(result)) => {
                total_amount += result.transfer.amount;
                if result.error.is_some() {
                    failed += 1;
                } else {
                    succeeded += 1;
                }
                results.push(result);
            }
            Ok(None) => {
                debug!(address = ?address, index = index, "Skipped: zero ERC20 balance");
                skipped += 1;
            }
            Err(e) => {
                warn!(address = ?address, index = index, error = %e, "Failed to sweep ERC20");
                results.push(TransferResult {
                    transfer: Transfer { to: destination, amount: U256::ZERO },
                    tx_hash: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    let total = (end_index - start_index + 1) as usize;
    Ok(CollectResult {
        total,
        succeeded,
        failed,
        skipped,
        results,
        total_amount,
    })
}

/// Sweeps native tokens from a single wallet.
/// Returns None if balance is insufficient to cover gas.
async fn sweep_native_single<P: Provider<AnyNetwork>>(
    provider: &P,
    signer: &PrivateKeySigner,
    from: Address,
    to: Address,
    dry_run: bool,
) -> eyre::Result<Option<TransferResult>> {
    let balance = provider.get_balance(from).await?;
    let gas_price = provider.get_gas_price().await?;

    // Calculate gas cost with safety margin
    let gas_cost =
        U256::from(NATIVE_TRANSFER_GAS) * U256::from(gas_price) * U256::from(GAS_MARGIN_NUM) / U256::from(GAS_MARGIN_DEN);

    if balance <= gas_cost {
        return Ok(None);
    }

    let amount = balance - gas_cost;

    if dry_run {
        info!(
            from = ?from,
            to = ?to,
            amount = %amount,
            balance = %balance,
            "[DRY RUN] Would sweep native tokens"
        );
        return Ok(Some(TransferResult {
            transfer: Transfer { to, amount },
            tx_hash: None,
            error: None,
        }));
    }

    let wallet = EthereumWallet::new(signer.clone());
    let signed_provider = ProviderBuilder::<_, _, AnyNetwork>::default()
        .wallet(wallet)
        .connect_provider(provider);

    let tx = WithOtherFields::new(
        TransactionRequest::default()
            .from(from)
            .to(to)
            .value(amount)
            .gas_limit(NATIVE_TRANSFER_GAS)
            .gas_price(gas_price),
    );

    match signed_provider.send_transaction(tx).await {
        Ok(pending) => {
            let tx_hash = *pending.inner().tx_hash();
            info!(from = ?from, to = ?to, amount = %amount, tx_hash = ?tx_hash, "Swept native tokens");
            Ok(Some(TransferResult {
                transfer: Transfer { to, amount },
                tx_hash: Some(tx_hash),
                error: None,
            }))
        }
        Err(e) => Ok(Some(TransferResult {
            transfer: Transfer { to, amount },
            tx_hash: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Sweeps ERC20 tokens from a single wallet.
/// Returns None if balance is zero.
async fn sweep_erc20_single<P: Provider<AnyNetwork>>(
    provider: &P,
    signer: &PrivateKeySigner,
    from: Address,
    token: Address,
    to: Address,
    dry_run: bool,
) -> eyre::Result<Option<TransferResult>> {
    // Check ERC20 balance
    let balance_call = balanceOfCall { account: from }.abi_encode();
    let balance_result = provider
        .call(
            WithOtherFields::new(
                TransactionRequest::default()
                    .to(token)
                    .input(Bytes::from(balance_call).into()),
            ),
        )
        .await?;

    let balance = U256::from_be_slice(&balance_result);
    if balance.is_zero() {
        return Ok(None);
    }

    if dry_run {
        info!(
            from = ?from,
            token = ?token,
            to = ?to,
            amount = %balance,
            "[DRY RUN] Would sweep ERC20 tokens"
        );
        return Ok(Some(TransferResult {
            transfer: Transfer { to, amount: balance },
            tx_hash: None,
            error: None,
        }));
    }

    let calldata = transferCall {
        to,
        amount: balance,
    }
    .abi_encode();

    let wallet = EthereumWallet::new(signer.clone());
    let signed_provider = ProviderBuilder::<_, _, AnyNetwork>::default()
        .wallet(wallet)
        .connect_provider(provider);

    let tx = WithOtherFields::new(
        TransactionRequest::default()
            .from(from)
            .to(token)
            .input(Bytes::from(calldata).into()),
    );

    match signed_provider.send_transaction(tx).await {
        Ok(pending) => {
            let tx_hash = *pending.inner().tx_hash();
            info!(
                from = ?from,
                token = ?token,
                to = ?to,
                amount = %balance,
                tx_hash = ?tx_hash,
                "Swept ERC20 tokens"
            );
            Ok(Some(TransferResult {
                transfer: Transfer { to, amount: balance },
                tx_hash: Some(tx_hash),
                error: None,
            }))
        }
        Err(e) => Ok(Some(TransferResult {
            transfer: Transfer { to, amount: balance },
            tx_hash: None,
            error: Some(e.to_string()),
        })),
    }
}
