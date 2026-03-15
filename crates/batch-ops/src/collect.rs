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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_margin_constants() {
        // Gas margin should be 1.2x (6/5)
        assert_eq!(GAS_MARGIN_NUM, 6);
        assert_eq!(GAS_MARGIN_DEN, 5);
        // Verify the ratio
        assert_eq!(GAS_MARGIN_NUM / GAS_MARGIN_DEN, 1);
        // 6.0 / 5.0 = 1.2
        assert_eq!((GAS_MARGIN_NUM as f64) / (GAS_MARGIN_DEN as f64), 1.2);
    }

    #[test]
    fn test_native_transfer_gas() {
        assert_eq!(NATIVE_TRANSFER_GAS, 21_000);
    }

    #[test]
    fn test_balance_of_call_selector() {
        // ERC20 balanceOf(address) selector is 0x70a08231
        let call = balanceOfCall { account: Address::ZERO };
        let encoded = call.abi_encode();
        assert_eq!(
            &encoded[..4],
            &[0x70, 0xa0, 0x82, 0x31],
            "balanceOf selector should be 0x70a08231"
        );
    }

    #[test]
    fn test_balance_of_call_encoding_length() {
        let call = balanceOfCall { account: Address::ZERO };
        let encoded = call.abi_encode();
        // 4 bytes selector + 32 bytes address
        assert_eq!(encoded.len(), 4 + 32);
    }

    #[test]
    fn test_transfer_call_selector() {
        // ERC20 transfer(address,uint256) selector is 0xa9059cbb
        let call = transferCall {
            to: Address::ZERO,
            amount: U256::ZERO,
        };
        let encoded = call.abi_encode();
        assert_eq!(
            &encoded[..4],
            &[0xa9, 0x05, 0x9c, 0xbb],
            "transfer selector should be 0xa9059cbb"
        );
    }

    #[test]
    fn test_gas_cost_calculation() {
        // Verify the gas cost formula used in sweep_native_single:
        // gas_cost = NATIVE_TRANSFER_GAS * gas_price * GAS_MARGIN_NUM / GAS_MARGIN_DEN
        let gas_price = 20_000_000_000u64; // 20 gwei
        let gas_cost = U256::from(NATIVE_TRANSFER_GAS)
            * U256::from(gas_price)
            * U256::from(GAS_MARGIN_NUM)
            / U256::from(GAS_MARGIN_DEN);

        // 21000 * 20_000_000_000 * 6 / 5 = 21000 * 20_000_000_000 * 1.2
        //   = 21000 * 24_000_000_000 = 504_000_000_000_000
        let expected = U256::from(504_000_000_000_000u64);
        assert_eq!(gas_cost, expected);
    }
}

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
    let mut wallets = Vec::new();
    for index in start_index..=end_index {
        let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
        if let Some(pass) = passphrase {
            builder = builder.password(pass);
        }
        let signer = builder.index(index)?.build()?;
        let address = signer.address();
        wallets.push((address, EthereumWallet::new(signer)));
    }
    collect_native_from_wallets(provider, destination, wallets, dry_run).await
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
    let mut wallets = Vec::new();
    for index in start_index..=end_index {
        let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic);
        if let Some(pass) = passphrase {
            builder = builder.password(pass);
        }
        let signer = builder.index(index)?.build()?;
        let address = signer.address();
        wallets.push((address, EthereumWallet::new(signer)));
    }
    collect_erc20_from_wallets(provider, token, destination, wallets, dry_run).await
}

/// Collects native tokens from pre-built wallets to a destination.
/// Each entry is (source_address, wallet). Use this for remote-signer or any non-mnemonic signer.
pub async fn collect_native_from_wallets<P: Provider<AnyNetwork>>(
    provider: &P,
    destination: Address,
    wallets: Vec<(Address, EthereumWallet)>,
    dry_run: bool,
) -> eyre::Result<CollectResult> {
    let total = wallets.len();
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_amount = U256::ZERO;

    for (address, wallet) in wallets {
        match sweep_native_single(provider, wallet, address, destination, dry_run).await {
            Ok(Some(result)) => {
                total_amount += result.transfer.amount;
                if result.error.is_some() { failed += 1; } else { succeeded += 1; }
                results.push(result);
            }
            Ok(None) => {
                debug!(address = ?address, "Skipped: insufficient balance");
                skipped += 1;
            }
            Err(e) => {
                warn!(address = ?address, error = %e, "Failed to sweep");
                results.push(TransferResult {
                    transfer: Transfer { to: destination, amount: U256::ZERO },
                    tx_hash: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    Ok(CollectResult { total, succeeded, failed, skipped, results, total_amount })
}

/// Collects ERC20 tokens from pre-built wallets to a destination.
/// Each entry is (source_address, wallet). Use this for remote-signer or any non-mnemonic signer.
pub async fn collect_erc20_from_wallets<P: Provider<AnyNetwork>>(
    provider: &P,
    token: Address,
    destination: Address,
    wallets: Vec<(Address, EthereumWallet)>,
    dry_run: bool,
) -> eyre::Result<CollectResult> {
    let total = wallets.len();
    let mut results = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_amount = U256::ZERO;

    for (address, wallet) in wallets {
        match sweep_erc20_single(provider, wallet, address, token, destination, dry_run).await {
            Ok(Some(result)) => {
                total_amount += result.transfer.amount;
                if result.error.is_some() { failed += 1; } else { succeeded += 1; }
                results.push(result);
            }
            Ok(None) => {
                debug!(address = ?address, "Skipped: zero ERC20 balance");
                skipped += 1;
            }
            Err(e) => {
                warn!(address = ?address, error = %e, "Failed to sweep ERC20");
                results.push(TransferResult {
                    transfer: Transfer { to: destination, amount: U256::ZERO },
                    tx_hash: None,
                    error: Some(e.to_string()),
                });
                failed += 1;
            }
        }
    }

    Ok(CollectResult { total, succeeded, failed, skipped, results, total_amount })
}

/// Sweeps native tokens from a single wallet.
/// Returns None if balance is insufficient to cover gas.
async fn sweep_native_single<P: Provider<AnyNetwork>>(
    provider: &P,
    wallet: EthereumWallet,
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

    let signed_provider = ProviderBuilder::<_, _, AnyNetwork>::default()
        .with_recommended_fillers()
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
    wallet: EthereumWallet,
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

    let signed_provider = ProviderBuilder::<_, _, AnyNetwork>::default()
        .with_recommended_fillers()
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
