//! Batch distribute/collect operations for Foundry cast.
//!
//! Supports:
//! - Distributing native tokens and ERC20 tokens to multiple recipients
//! - Collecting/sweeping native tokens and ERC20 tokens from multiple wallets

pub mod collect;
pub mod distribute;
pub mod input;
pub mod types;

pub use types::*;
