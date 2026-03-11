//! Cobo MPC signer implementing alloy's Signer trait.

use crate::client::CoboMpcClient;
use crate::types::*;
use alloy_consensus::SignableTransaction;
use alloy_dyn_abi::TypedData;
use alloy_network::TxSigner;
use alloy_primitives::{Address, B256, ChainId, Signature};
use alloy_signer::Signer;
use async_trait::async_trait;

/// Cobo MPC wallet signer.
///
/// This signer communicates with the Cobo WaaS 2.0 API for:
/// - Message signing (EIP-191, EIP-712): returns signature bytes
/// - Transaction signing: NOTE - Cobo MPC signs+broadcasts atomically,
///   so `sign_transaction` is not supported in the standard flow.
///   Use `CoboMpcClient::call_contract()` directly for transactions.
#[derive(Debug, Clone)]
pub struct CoboMpcSigner {
    client: CoboMpcClient,
    address: Address,
    chain_id: Option<ChainId>,
}

impl CoboMpcSigner {
    /// Creates a new Cobo MPC signer.
    pub fn new(
        api_key_hex: &str,
        wallet_id: String,
        address: Address,
        cobo_chain_id: String,
        env: CoboEnv,
    ) -> eyre::Result<Self> {
        let client = CoboMpcClient::new(
            api_key_hex,
            wallet_id,
            format!("{address:?}"),
            cobo_chain_id,
            env,
        )?;

        Ok(Self {
            client,
            address,
            chain_id: None,
        })
    }

    /// Returns a reference to the underlying Cobo MPC client.
    pub fn client(&self) -> &CoboMpcClient {
        &self.client
    }

    /// Converts a 65-byte signature to alloy Signature.
    fn bytes_to_signature(sig_bytes: &[u8; 65]) -> alloy_signer::Result<Signature> {
        let r = alloy_primitives::U256::from_be_slice(&sig_bytes[..32]);
        let s = alloy_primitives::U256::from_be_slice(&sig_bytes[32..64]);
        let v = sig_bytes[64];

        // alloy expects v as a parity boolean (false=0, true=1)
        let v_parity = if v >= 27 { v - 27 } else { v };

        Ok(Signature::new(r, s, v_parity != 0))
    }
}

#[async_trait]
impl Signer for CoboMpcSigner {
    async fn sign_hash(&self, hash: &B256) -> alloy_signer::Result<Signature> {
        // Sign raw hash via Cobo API (treated as personal sign of the hash bytes)
        let sig = self
            .client
            .sign_message(hash.as_slice())
            .await
            .map_err(|e| alloy_signer::Error::other(e))?;
        Self::bytes_to_signature(&sig)
    }

    async fn sign_message(&self, message: &[u8]) -> alloy_signer::Result<Signature> {
        let sig = self
            .client
            .sign_message(message)
            .await
            .map_err(|e| alloy_signer::Error::other(e))?;
        Self::bytes_to_signature(&sig)
    }

    fn address(&self) -> Address {
        self.address
    }

    fn chain_id(&self) -> Option<ChainId> {
        self.chain_id
    }

    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        self.chain_id = chain_id;
    }

    async fn sign_dynamic_typed_data(
        &self,
        payload: &TypedData,
    ) -> alloy_signer::Result<Signature> {
        let json = serde_json::to_string(payload)
            .map_err(|e| alloy_signer::Error::other(e))?;
        let sig = self
            .client
            .sign_typed_data(&json)
            .await
            .map_err(|e| alloy_signer::Error::other(e))?;
        Self::bytes_to_signature(&sig)
    }
}

#[async_trait]
impl TxSigner<Signature> for CoboMpcSigner {
    fn address(&self) -> Address {
        Signer::address(self)
    }

    async fn sign_transaction(
        &self,
        _tx: &mut dyn SignableTransaction<Signature>,
    ) -> alloy_signer::Result<Signature> {
        // Cobo MPC cannot sign transactions separately from broadcasting.
        // Transaction sending must go through CoboMpcClient::call_contract().
        // This method should not be called in the normal flow.
        Err(alloy_signer::Error::other(
            "Cobo MPC signer does not support standalone transaction signing. \
             Transactions must be sent via the Cobo API (sign+broadcast is atomic). \
             Use `cast send --cobo` which handles this automatically.",
        ))
    }
}
