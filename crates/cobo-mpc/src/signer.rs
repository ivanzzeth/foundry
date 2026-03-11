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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    /// Valid 32-byte Ed25519 private key hex for testing.
    const TEST_KEY_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";

    fn test_address() -> Address {
        "0xAbC1230001112223334445556667778889990000"
            .parse::<Address>()
            .unwrap()
    }

    fn test_signer() -> CoboMpcSigner {
        CoboMpcSigner::new(
            TEST_KEY_HEX,
            "wallet-123".to_string(),
            test_address(),
            "ETH".to_string(),
            CoboEnv::Dev,
        )
        .unwrap()
    }

    // --- CoboMpcSigner::new ---

    #[test]
    fn test_new_valid() {
        let signer = CoboMpcSigner::new(
            TEST_KEY_HEX,
            "w1".to_string(),
            test_address(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(signer.is_ok());
    }

    #[test]
    fn test_new_invalid_key() {
        let signer = CoboMpcSigner::new(
            "invalid",
            "w1".to_string(),
            test_address(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(signer.is_err());
    }

    // --- address / chain_id / set_chain_id ---

    #[test]
    fn test_signer_address() {
        let signer = test_signer();
        assert_eq!(Signer::address(&signer), test_address());
    }

    #[test]
    fn test_signer_chain_id_default_none() {
        let signer = test_signer();
        assert_eq!(signer.chain_id(), None);
    }

    #[test]
    fn test_signer_set_chain_id() {
        let mut signer = test_signer();
        signer.set_chain_id(Some(1));
        assert_eq!(signer.chain_id(), Some(1));
    }

    #[test]
    fn test_signer_set_chain_id_back_to_none() {
        let mut signer = test_signer();
        signer.set_chain_id(Some(42));
        assert_eq!(signer.chain_id(), Some(42));
        signer.set_chain_id(None);
        assert_eq!(signer.chain_id(), None);
    }

    // --- bytes_to_signature ---

    #[test]
    fn test_bytes_to_signature_v27() {
        let mut sig = [0xAAu8; 65];
        sig[64] = 27; // V = 27 -> parity = false (0)
        let result = CoboMpcSigner::bytes_to_signature(&sig).unwrap();
        let expected_r = U256::from_be_slice(&sig[..32]);
        let expected_s = U256::from_be_slice(&sig[32..64]);
        assert_eq!(result.r(), expected_r);
        assert_eq!(result.s(), expected_s);
        // V=27 means v_parity=0 (false)
        assert!(!result.v(), "V=27 should give parity=false");
    }

    #[test]
    fn test_bytes_to_signature_v28() {
        let mut sig = [0xBBu8; 65];
        sig[64] = 28; // V = 28 -> parity = true (1)
        let result = CoboMpcSigner::bytes_to_signature(&sig).unwrap();
        let expected_r = U256::from_be_slice(&sig[..32]);
        let expected_s = U256::from_be_slice(&sig[32..64]);
        assert_eq!(result.r(), expected_r);
        assert_eq!(result.s(), expected_s);
        // V=28 means v_parity=1 (true)
        assert!(result.v(), "V=28 should give parity=true");
    }

    #[test]
    fn test_bytes_to_signature_v0() {
        let mut sig = [0xCCu8; 65];
        sig[64] = 0; // V = 0 -> parity = false
        let result = CoboMpcSigner::bytes_to_signature(&sig).unwrap();
        assert!(!result.v(), "V=0 should give parity=false");
    }

    #[test]
    fn test_bytes_to_signature_v1() {
        let mut sig = [0xDDu8; 65];
        sig[64] = 1; // V = 1 -> parity = true
        let result = CoboMpcSigner::bytes_to_signature(&sig).unwrap();
        assert!(result.v(), "V=1 should give parity=true");
    }

    #[test]
    fn test_bytes_to_signature_r_s_extraction() {
        let mut sig = [0u8; 65];
        // Set r to a known value
        sig[31] = 1; // r = 1
        // Set s to a known value
        sig[63] = 2; // s = 2
        sig[64] = 27;
        let result = CoboMpcSigner::bytes_to_signature(&sig).unwrap();
        assert_eq!(result.r(), U256::from(1));
        assert_eq!(result.s(), U256::from(2));
    }

    // --- TxSigner::sign_transaction ---

    #[tokio::test]
    async fn test_sign_transaction_returns_error() {
        let signer = test_signer();
        // Create a minimal transaction to pass to sign_transaction
        let mut tx = alloy_consensus::TxLegacy {
            chain_id: Some(1),
            nonce: 0,
            gas_price: 0,
            gas_limit: 21000,
            to: alloy_primitives::TxKind::Call(test_address()),
            value: alloy_primitives::U256::ZERO,
            input: alloy_primitives::Bytes::new(),
        };

        let result = TxSigner::sign_transaction(&signer, &mut tx).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("does not support standalone transaction signing"),
            "unexpected error: {err_msg}"
        );
    }

    // --- client accessor ---

    #[test]
    fn test_client_accessor() {
        let signer = test_signer();
        let client = signer.client();
        assert_eq!(client.cobo_chain_id(), "ETH");
        assert_eq!(
            client.address(),
            &format!("{:?}", test_address())
        );
    }
}
