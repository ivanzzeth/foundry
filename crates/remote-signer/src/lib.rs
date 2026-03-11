//! Remote HTTP signer integration for Foundry.
//!
//! This crate provides a signer that communicates with a remote-signer HTTP service
//! for transaction signing with parameter-level ACL support.

mod auth;
mod client;
mod signer;
mod types;

pub use client::RemoteSignerClient;
pub use signer::RemoteHttpSigner;
pub use types::*;
