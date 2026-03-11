//! Remote HTTP signer implementing alloy's Signer and TxSigner traits.

use crate::client::RemoteSignerClient;
use crate::types::*;
use alloy_consensus::SignableTransaction;
use alloy_dyn_abi::TypedData;
use alloy_network::TxSigner;
use alloy_primitives::{Address, B256, ChainId, Signature};
use alloy_signer::Signer;
use async_trait::async_trait;

/// Remote HTTP signer that delegates all signing to a remote service.
///
/// Unlike Cobo MPC, this signer uses standard sign→broadcast flow:
/// the remote service returns a signature, and broadcasting is done separately.
#[derive(Debug, Clone)]
pub struct RemoteHttpSigner {
    client: RemoteSignerClient,
    address: Address,
    chain_id: Option<ChainId>,
}

impl RemoteHttpSigner {
    /// Creates a new remote HTTP signer.
    pub fn new(
        base_url: &str,
        api_key_id: String,
        api_key_hex: &str,
        address: Address,
    ) -> eyre::Result<Self> {
        let client = RemoteSignerClient::new(base_url, api_key_id, api_key_hex)?;
        Ok(Self {
            client,
            address,
            chain_id: None,
        })
    }

    /// Returns a reference to the underlying client.
    pub fn client(&self) -> &RemoteSignerClient {
        &self.client
    }

    /// Extracts a 65-byte signature from the sign response.
    fn extract_signature(resp: &SignResponse) -> alloy_signer::Result<Signature> {
        let sig_hex = resp
            .signature
            .as_ref()
            .ok_or_else(|| alloy_signer::Error::other("no signature in response"))?;

        let sig_bytes = hex::decode(sig_hex.strip_prefix("0x").unwrap_or(sig_hex))
            .map_err(|e| alloy_signer::Error::other(format!("invalid hex signature: {e}")))?;

        if sig_bytes.len() != 65 {
            return Err(alloy_signer::Error::other(format!(
                "expected 65 byte signature, got {}",
                sig_bytes.len()
            )));
        }

        let r = alloy_primitives::U256::from_be_slice(&sig_bytes[..32]);
        let s = alloy_primitives::U256::from_be_slice(&sig_bytes[32..64]);
        let v = sig_bytes[64];

        // Normalize v to parity bool (false=0, true=1)
        let y_parity = if v >= 27 { v - 27 != 0 } else { v != 0 };

        Ok(Signature::new(r, s, y_parity))
    }

    /// Helper to create and send a sign request.
    async fn do_sign(
        &self,
        sign_type: SignType,
        data: String,
    ) -> alloy_signer::Result<Signature> {
        let chain_id_str = self
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "1".to_string());

        let req = SignRequest {
            sign_type,
            chain_id: chain_id_str,
            address: format!("{:?}", self.address),
            data,
            metadata: None,
        };

        let resp = self
            .client
            .sign(&req)
            .await
            .map_err(|e| alloy_signer::Error::other(e))?;

        Self::extract_signature(&resp)
    }
}

#[async_trait]
impl Signer for RemoteHttpSigner {
    async fn sign_hash(&self, hash: &B256) -> alloy_signer::Result<Signature> {
        let data = format!("0x{}", hex::encode(hash.as_slice()));
        self.do_sign(SignType::Hash, data).await
    }

    async fn sign_message(&self, message: &[u8]) -> alloy_signer::Result<Signature> {
        let data = format!("0x{}", hex::encode(message));
        self.do_sign(SignType::Personal, data).await
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
        self.do_sign(SignType::TypedData, json).await
    }
}

#[async_trait]
impl TxSigner<Signature> for RemoteHttpSigner {
    fn address(&self) -> Address {
        Signer::address(self)
    }

    async fn sign_transaction(
        &self,
        tx: &mut dyn SignableTransaction<Signature>,
    ) -> alloy_signer::Result<Signature> {
        // Encode the transaction for signing
        let mut buf = Vec::new();
        tx.encode_for_signing(&mut buf);
        let data = format!("0x{}", hex::encode(&buf));
        self.do_sign(SignType::Transaction, data).await
    }
}
