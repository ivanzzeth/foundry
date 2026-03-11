//! `cast collect` command implementation.

use alloy_primitives::Address;
use clap::{Parser, Subcommand};
use eyre::Result;
use foundry_batch_ops::{collect, CollectResult};
use foundry_cli::{
    opts::EthereumOpts,
    utils::{LoadConfig, get_provider},
};

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

    /// BIP-39 mnemonic to derive sender wallets.
    #[arg(id = "sweep_mnemonic", long = "sweep-mnemonic", value_name = "MNEMONIC", env = "COLLECT_MNEMONIC")]
    sweep_mnemonic: String,

    /// Optional mnemonic passphrase.
    #[arg(id = "sweep_passphrase", long = "sweep-passphrase", value_name = "PASSPHRASE")]
    sweep_passphrase: Option<String>,

    /// Start derivation index (inclusive).
    #[arg(long, default_value = "0")]
    start_index: u32,

    /// End derivation index (inclusive).
    #[arg(long, default_value = "9")]
    end_index: u32,

    /// Dry run mode - simulate without sending transactions.
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    eth: EthereumOpts,
}

#[derive(Debug, Parser)]
pub struct CollectErc20Args {
    /// ERC20 token contract address.
    #[arg(long, value_name = "ADDRESS")]
    token: Address,

    /// Destination address to collect funds to.
    #[arg(long, value_name = "ADDRESS")]
    to: Address,

    /// BIP-39 mnemonic to derive sender wallets.
    #[arg(id = "sweep_mnemonic", long = "sweep-mnemonic", value_name = "MNEMONIC", env = "COLLECT_MNEMONIC")]
    sweep_mnemonic: String,

    /// Optional mnemonic passphrase.
    #[arg(id = "sweep_passphrase", long = "sweep-passphrase", value_name = "PASSPHRASE")]
    sweep_passphrase: Option<String>,

    /// Start derivation index (inclusive).
    #[arg(long, default_value = "0")]
    start_index: u32,

    /// End derivation index (inclusive).
    #[arg(long, default_value = "9")]
    end_index: u32,

    /// Dry run mode - simulate without sending transactions.
    #[arg(long)]
    dry_run: bool,

    #[command(flatten)]
    eth: EthereumOpts,
}

impl CollectArgs {
    pub async fn run(self) -> Result<()> {
        match self.command {
            CollectSubcommand::Native(args) => {
                let config = args.eth.load_config()?;
                let provider = get_provider(&config)?;

                let result = collect::collect_native_from_mnemonic(
                    &provider,
                    args.to,
                    &args.sweep_mnemonic,
                    args.sweep_passphrase.as_deref(),
                    args.start_index,
                    args.end_index,
                    args.dry_run,
                )
                .await?;

                print_result(&result);
                Ok(())
            }
            CollectSubcommand::Erc20(args) => {
                let config = args.eth.load_config()?;
                let provider = get_provider(&config)?;

                let result = collect::collect_erc20_from_mnemonic(
                    &provider,
                    args.token,
                    args.to,
                    &args.sweep_mnemonic,
                    args.sweep_passphrase.as_deref(),
                    args.start_index,
                    args.end_index,
                    args.dry_run,
                )
                .await?;

                print_result(&result);
                Ok(())
            }
        }
    }
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
