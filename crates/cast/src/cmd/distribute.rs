//! `cast distribute` command implementation.

use alloy_network::{AnyNetwork, EthereumWallet};
use alloy_primitives::Address;
use alloy_provider::ProviderBuilder;
use clap::{Parser, Subcommand};
use eyre::Result;
use foundry_batch_ops::{distribute, input, DistributeResult};
use foundry_cli::utils::{LoadConfig, get_provider};

use crate::tx::SendTxOpts;

/// CLI arguments for `cast distribute`.
#[derive(Debug, Parser)]
pub struct DistributeArgs {
    #[command(subcommand)]
    command: DistributeSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DistributeSubcommand {
    /// Distribute native tokens (ETH, etc.) to multiple recipients.
    #[command(name = "native")]
    Native {
        /// Path to CSV file with transfers (address,amount).
        #[arg(long, value_name = "PATH", conflicts_with_all = &["to", "stdin"])]
        csv: Option<String>,

        /// Inline transfers in "address:amount,address:amount" format.
        #[arg(long, value_name = "TRANSFERS", conflicts_with_all = &["csv", "stdin"])]
        to: Option<String>,

        /// Read transfers from stdin (CSV format).
        #[arg(long, conflicts_with_all = &["csv", "to"])]
        stdin: bool,

        /// Dry run mode - simulate without sending transactions.
        #[arg(long)]
        dry_run: bool,

        #[command(flatten)]
        send_tx: SendTxOpts,
    },
    /// Distribute ERC20 tokens to multiple recipients.
    #[command(name = "erc20")]
    Erc20 {
        /// ERC20 token contract address.
        #[arg(long, value_name = "ADDRESS")]
        token: Address,

        /// Path to CSV file with transfers (address,amount).
        #[arg(long, value_name = "PATH", conflicts_with_all = &["to", "stdin"])]
        csv: Option<String>,

        /// Inline transfers in "address:amount,address:amount" format.
        #[arg(long, value_name = "TRANSFERS", conflicts_with_all = &["csv", "stdin"])]
        to: Option<String>,

        /// Read transfers from stdin (CSV format).
        #[arg(long, conflicts_with_all = &["csv", "to"])]
        stdin: bool,

        /// Dry run mode - simulate without sending transactions.
        #[arg(long)]
        dry_run: bool,

        #[command(flatten)]
        send_tx: SendTxOpts,
    },
}

impl DistributeArgs {
    pub async fn run(self) -> Result<()> {
        match self.command {
            DistributeSubcommand::Native { csv, to, stdin, dry_run, send_tx } => {
                let transfers = parse_transfers(csv.as_deref(), to.as_deref(), stdin)?;
                let config = send_tx.eth.load_config()?;
                let provider = get_provider(&config)?;

                let result = if dry_run {
                    distribute::distribute_native(&provider, &transfers, true).await?
                } else {
                    let signer = send_tx.eth.wallet.signer().await?;
                    let wallet = EthereumWallet::from(signer);
                    let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
                        .with_recommended_fillers()
                        .wallet(wallet)
                        .connect_provider(&provider);
                    distribute::distribute_native(&provider, &transfers, false).await?
                };

                print_result(&result);
                Ok(())
            }
            DistributeSubcommand::Erc20 { token, csv, to, stdin, dry_run, send_tx } => {
                let transfers = parse_transfers(csv.as_deref(), to.as_deref(), stdin)?;
                let config = send_tx.eth.load_config()?;
                let provider = get_provider(&config)?;

                let result = if dry_run {
                    distribute::distribute_erc20(&provider, token, &transfers, true).await?
                } else {
                    let signer = send_tx.eth.wallet.signer().await?;
                    let wallet = EthereumWallet::from(signer);
                    let provider = ProviderBuilder::<_, _, AnyNetwork>::default()
                        .with_recommended_fillers()
                        .wallet(wallet)
                        .connect_provider(&provider);
                    distribute::distribute_erc20(&provider, token, &transfers, false).await?
                };

                print_result(&result);
                Ok(())
            }
        }
    }
}

fn parse_transfers(
    csv: Option<&str>,
    to: Option<&str>,
    stdin: bool,
) -> Result<Vec<foundry_batch_ops::Transfer>> {
    if let Some(path) = csv {
        Ok(input::parse_csv(path)?)
    } else if let Some(inline) = to {
        Ok(input::parse_inline(inline)?)
    } else if stdin {
        Ok(input::parse_stdin()?)
    } else {
        eyre::bail!("Must specify --csv, --to, or --stdin for transfer inputs")
    }
}

fn print_result(result: &DistributeResult) {
    sh_println!("\nDistribute Summary:").ok();
    sh_println!("  Total:     {}", result.total).ok();
    sh_println!("  Succeeded: {}", result.succeeded).ok();
    sh_println!("  Failed:    {}", result.failed).ok();
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
