//! `cast collect` command implementation.

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use clap::{Parser, Subcommand};
use eyre::Result;
use foundry_batch_ops::{collect, CollectResult};
use foundry_cli::{
    opts::EthereumOpts,
    utils::{LoadConfig, get_provider},
};

use crate::tx::SendTxOpts;

/// CLI arguments for `cast collect`.
#[derive(Debug, Parser)]
pub struct CollectArgs {
    #[command(subcommand)]
    command: CollectSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum CollectSubcommand {
    /// Collect/sweep native tokens from multiple wallets to a destination.
    #[command(name = "native")]
    Native(CollectNativeArgs),
    /// Collect/sweep ERC20 tokens from multiple wallets to a destination.
    #[command(name = "erc20")]
    Erc20(CollectErc20Args),
}

#[derive(Debug, Parser)]
pub struct CollectNativeArgs {
    /// Destination address to collect funds to.
    #[arg(long, value_name = "ADDRESS")]
    to: Address,

    /// Source addresses to collect from (comma-separated).
    /// Uses the wallet signer (e.g. remote-signer) for each address.
    /// Mutually exclusive with --sweep-mnemonic.
    #[arg(long, value_name = "ADDRESSES", value_delimiter = ',', conflicts_with_all = &["sweep_mnemonic", "sweep_passphrase", "start_index", "end_index"])]
    addresses: Option<Vec<Address>>,

    /// BIP-39 mnemonic to derive sender wallets.
    /// Mutually exclusive with --addresses.
    #[arg(id = "sweep_mnemonic", long = "sweep-mnemonic", value_name = "MNEMONIC", env = "COLLECT_MNEMONIC", conflicts_with = "addresses")]
    sweep_mnemonic: Option<String>,

    /// Optional mnemonic passphrase.
    #[arg(id = "sweep_passphrase", long = "sweep-passphrase", value_name = "PASSPHRASE")]
    sweep_passphrase: Option<String>,

    /// Start derivation index (inclusive).
    #[arg(long, default_value = "0")]
    start_index: u32,

    /// End derivation index (inclusive).
    #[arg(long, default_value = "9")]
    end_index: u32,

    #[command(flatten)]
    send_tx: SendTxOpts,
}

#[derive(Debug, Parser)]
pub struct CollectErc20Args {
    /// ERC20 token contract address.
    #[arg(long, value_name = "ADDRESS")]
    token: Address,

    /// Destination address to collect funds to.
    #[arg(long, value_name = "ADDRESS")]
    to: Address,

    /// Source addresses to collect from (comma-separated).
    /// Uses the wallet signer (e.g. remote-signer) for each address.
    /// Mutually exclusive with --sweep-mnemonic.
    #[arg(long, value_name = "ADDRESSES", value_delimiter = ',', conflicts_with_all = &["sweep_mnemonic", "sweep_passphrase", "start_index", "end_index"])]
    addresses: Option<Vec<Address>>,

    /// BIP-39 mnemonic to derive sender wallets.
    /// Mutually exclusive with --addresses.
    #[arg(id = "sweep_mnemonic", long = "sweep-mnemonic", value_name = "MNEMONIC", env = "COLLECT_MNEMONIC", conflicts_with = "addresses")]
    sweep_mnemonic: Option<String>,

    /// Optional mnemonic passphrase.
    #[arg(id = "sweep_passphrase", long = "sweep-passphrase", value_name = "PASSPHRASE")]
    sweep_passphrase: Option<String>,

    /// Start derivation index (inclusive).
    #[arg(long, default_value = "0")]
    start_index: u32,

    /// End derivation index (inclusive).
    #[arg(long, default_value = "9")]
    end_index: u32,

    #[command(flatten)]
    send_tx: SendTxOpts,
}

impl CollectArgs {
    pub async fn run(self) -> Result<()> {
        match self.command {
            CollectSubcommand::Native(args) => {
                let config = args.send_tx.eth.load_config()?;
                let provider = get_provider(&config)?;

                let result = if let Some(addresses) = args.addresses {
                    // Signer-based mode: build wallet for each address using WalletOpts
                    let wallets = build_wallets_for_addresses(&args.send_tx, &addresses).await?;
                    collect::collect_native_from_wallets(
                        &provider, args.to, wallets, args.send_tx.dry_run,
                    ).await?
                } else if let Some(mnemonic) = args.sweep_mnemonic {
                    collect::collect_native_from_mnemonic(
                        &provider, args.to, &mnemonic,
                        args.sweep_passphrase.as_deref(),
                        args.start_index, args.end_index, args.send_tx.dry_run,
                    ).await?
                } else {
                    eyre::bail!("Must specify --addresses or --sweep-mnemonic")
                };

                print_result(&result);
                Ok(())
            }
            CollectSubcommand::Erc20(args) => {
                let config = args.send_tx.eth.load_config()?;
                let provider = get_provider(&config)?;

                let result = if let Some(addresses) = args.addresses {
                    let wallets = build_wallets_for_addresses(&args.send_tx, &addresses).await?;
                    collect::collect_erc20_from_wallets(
                        &provider, args.token, args.to, wallets, args.send_tx.dry_run,
                    ).await?
                } else if let Some(mnemonic) = args.sweep_mnemonic {
                    collect::collect_erc20_from_mnemonic(
                        &provider, args.token, args.to, &mnemonic,
                        args.sweep_passphrase.as_deref(),
                        args.start_index, args.end_index, args.send_tx.dry_run,
                    ).await?
                } else {
                    eyre::bail!("Must specify --addresses or --sweep-mnemonic")
                };

                print_result(&result);
                Ok(())
            }
        }
    }
}

/// Build an EthereumWallet for each address using the wallet options from SendTxOpts.
/// For remote-signer: creates a RemoteHttpSigner per address (shared connection config).
/// For private-key/keystore: uses the single configured signer (all addresses must match).
async fn build_wallets_for_addresses(
    send_tx: &SendTxOpts,
    addresses: &[Address],
) -> Result<Vec<(Address, EthereumWallet)>> {
    let wallet_opts = &send_tx.eth.wallet;
    let mut wallets = Vec::with_capacity(addresses.len());

    #[cfg(feature = "signer-remote")]
    {
        use foundry_wallets::WalletSigner;

        // Check if remote-signer is configured
        if let Some(url) = &wallet_opts.remote_signer_url {
            let api_key_id = wallet_opts.remote_signer_api_key_id.clone()
                .ok_or_else(|| eyre::eyre!("--remote-signer-api-key-id is required"))?;
            let api_key_hex = wallet_opts.remote_signer_api_key.as_deref();
            let api_key_file = wallet_opts.remote_signer_api_key_file.as_deref();

            let tls_paths = match (
                &wallet_opts.remote_signer_ca_file,
                &wallet_opts.remote_signer_cert_file,
                &wallet_opts.remote_signer_key_file,
            ) {
                (Some(ca), Some(cert), Some(key)) => Some((ca.clone(), cert.clone(), key.clone())),
                _ => None,
            };
            let skip_verify = wallet_opts.remote_signer_tls_insecure_skip_verify.unwrap_or(false);

            for &addr in addresses {
                let signer = WalletSigner::from_remote_signer(
                    url, api_key_id.clone(), api_key_hex, api_key_file,
                    addr, tls_paths.clone(), skip_verify,
                )?;
                wallets.push((addr, EthereumWallet::from(signer)));
            }
            return Ok(wallets);
        }
    }

    // Fallback: use the single configured signer (must match all addresses)
    let signer = wallet_opts.signer().await?;
    let signer_addr = alloy_signer::Signer::address(&signer);
    if addresses.len() > 1 || (addresses.len() == 1 && addresses[0] != signer_addr) {
        eyre::bail!(
            "Multi-address collect requires --remote-signer-*. \
             The configured signer only supports address {signer_addr}."
        );
    }
    for &addr in addresses {
        wallets.push((addr, EthereumWallet::from(signer)));
        break; // only one address supported without remote-signer
    }
    Ok(wallets)
}

fn print_result(result: &CollectResult) {
    sh_println!("\nCollect Summary:").ok();
    sh_println!("  Total:     {}", result.total).ok();
    sh_println!("  Succeeded: {}", result.succeeded).ok();
    sh_println!("  Failed:    {}", result.failed).ok();
    sh_println!("  Skipped:   {}", result.skipped).ok();
    sh_println!("  Amount:    {} wei", result.total_amount).ok();

    for r in &result.results {
        if let Some(hash) = r.tx_hash {
            sh_println!("  {:#x} -> {} ({} wei)", hash, r.transfer.to, r.transfer.amount).ok();
        } else if let Some(ref err) = r.error {
            sh_println!("  FAILED -> {} ({} wei): {}", r.transfer.to, r.transfer.amount, err).ok();
        } else {
            sh_println!("  [DRY RUN] -> {} ({} wei)", r.transfer.to, r.transfer.amount).ok();
        }
    }
}
