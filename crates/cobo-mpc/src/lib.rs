//! Cobo MPC wallet signer integration for Foundry.
//!
//! This crate provides a signer that communicates with the Cobo WaaS 2.0 REST API
//! for message signing and transaction sending.

mod auth;
mod client;
mod signer;
mod types;

pub use client::CoboMpcClient;
pub use signer::CoboMpcSigner;
pub use types::*;
