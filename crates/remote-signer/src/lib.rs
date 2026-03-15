//! Remote HTTP signer integration for Foundry.
//!
//! Uses `remote-signer-client` for HTTP and auth; provides an alloy `Signer`/`TxSigner`
//! implementation that delegates signing to a remote-signer service.

mod signer;

pub use signer::{RemoteHttpSigner, TlsConfig};
