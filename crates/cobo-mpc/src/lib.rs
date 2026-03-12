//! Cobo MPC wallet signer integration for Foundry.
//!
//! This crate provides a signer that communicates with the Cobo WaaS 2.0 REST API
//! for message signing and transaction sending.
//!
//! # Architecture
//!
//! Unlike standard signers that return signatures for separate broadcasting,
//! Cobo MPC uses atomic sign+broadcast. This requires special handling:
//!
//! - `CoboMpcSigner`: Implements Signer trait for message signing (EIP-191, EIP-712)
//! - `CoboMpcClient`: Low-level API client for Cobo WaaS 2.0
//! - `CoboMpcProvider`: Provider wrapper that routes transactions via Cobo API

mod auth;
mod client;
mod provider;
mod signer;
mod types;

pub use client::CoboMpcClient;
pub use provider::CoboMpcProvider;
pub use signer::CoboMpcSigner;
pub use types::*;
