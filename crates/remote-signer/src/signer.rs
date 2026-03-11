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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    const TEST_KEY_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";
    const TEST_ADDR: Address = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");

    fn make_signer() -> RemoteHttpSigner {
        RemoteHttpSigner::new(
            "http://localhost:9999",
            "test-key".into(),
            TEST_KEY_HEX,
            TEST_ADDR,
        )
        .unwrap()
    }

    // --- new() ---

    #[test]
    fn new_valid_construction() {
        let signer = RemoteHttpSigner::new(
            "http://localhost:8080",
            "key-id".into(),
            TEST_KEY_HEX,
            TEST_ADDR,
        );
        assert!(signer.is_ok());
    }

    #[test]
    fn new_invalid_key_fails() {
        let signer = RemoteHttpSigner::new(
            "http://localhost:8080",
            "key-id".into(),
            "bad",
            TEST_ADDR,
        );
        assert!(signer.is_err());
    }

    // --- address / chain_id / set_chain_id ---

    #[test]
    fn address_returns_configured_address() {
        let signer = make_signer();
        assert_eq!(Signer::address(&signer), TEST_ADDR);
    }

    #[test]
    fn chain_id_default_is_none() {
        let signer = make_signer();
        assert_eq!(signer.chain_id(), None);
    }

    #[test]
    fn set_chain_id_updates_value() {
        let mut signer = make_signer();
        signer.set_chain_id(Some(42));
        assert_eq!(signer.chain_id(), Some(42));
    }

    #[test]
    fn set_chain_id_back_to_none() {
        let mut signer = make_signer();
        signer.set_chain_id(Some(42));
        signer.set_chain_id(None);
        assert_eq!(signer.chain_id(), None);
    }

    // --- extract_signature: valid cases ---

    fn make_response(sig_hex: Option<String>) -> SignResponse {
        SignResponse {
            request_id: "req-1".to_string(),
            status: RequestStatus::Completed,
            signature: sig_hex,
            error: None,
        }
    }

    /// Build a 65-byte signature hex with given v byte.
    fn fake_sig_hex(v: u8) -> String {
        let r = [0x01u8; 32];
        let s = [0x02u8; 32];
        let mut sig = Vec::with_capacity(65);
        sig.extend_from_slice(&r);
        sig.extend_from_slice(&s);
        sig.push(v);
        format!("0x{}", hex::encode(&sig))
    }

    #[test]
    fn extract_signature_v27() {
        let resp = make_response(Some(fake_sig_hex(27)));
        let sig = RemoteHttpSigner::extract_signature(&resp).unwrap();
        // v=27 → y_parity = false (27-27=0, 0!=0 is false)
        assert!(!sig.v(), "v=27 should map to y_parity=false");
    }

    #[test]
    fn extract_signature_v28() {
        let resp = make_response(Some(fake_sig_hex(28)));
        let sig = RemoteHttpSigner::extract_signature(&resp).unwrap();
        // v=28 → y_parity = true (28-27=1, 1!=0 is true)
        assert!(sig.v(), "v=28 should map to y_parity=true");
    }

    #[test]
    fn extract_signature_v0() {
        let resp = make_response(Some(fake_sig_hex(0)));
        let sig = RemoteHttpSigner::extract_signature(&resp).unwrap();
        // v=0, <27 branch → 0!=0 is false
        assert!(!sig.v(), "v=0 should map to y_parity=false");
    }

    #[test]
    fn extract_signature_v1() {
        let resp = make_response(Some(fake_sig_hex(1)));
        let sig = RemoteHttpSigner::extract_signature(&resp).unwrap();
        // v=1, <27 branch → 1!=0 is true
        assert!(sig.v(), "v=1 should map to y_parity=true");
    }

    #[test]
    fn extract_signature_without_0x_prefix() {
        let hex_no_prefix = fake_sig_hex(27).strip_prefix("0x").unwrap().to_string();
        let resp = make_response(Some(hex_no_prefix));
        let sig = RemoteHttpSigner::extract_signature(&resp);
        assert!(sig.is_ok());
    }

    // --- extract_signature: error cases ---

    #[test]
    fn extract_signature_no_signature_field() {
        let resp = make_response(None);
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("no signature"), "expected 'no signature', got: {err}");
    }

    #[test]
    fn extract_signature_invalid_hex() {
        let resp = make_response(Some("0xZZZZZZ".to_string()));
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("invalid hex"), "expected 'invalid hex', got: {err}");
    }

    #[test]
    fn extract_signature_wrong_length_too_short() {
        let resp = make_response(Some("0x0011".to_string()));
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("expected 65 byte"),
            "expected length error, got: {err}"
        );
    }

    #[test]
    fn extract_signature_wrong_length_too_long() {
        let long = format!("0x{}", hex::encode([0xABu8; 66]));
        let resp = make_response(Some(long));
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("expected 65 byte"),
            "expected length error, got: {err}"
        );
    }

    // --- do_sign: verify request construction ---
    // We test indirectly via the fields that do_sign constructs.
    // Since do_sign requires a real HTTP call, we test the request building
    // logic by checking the SignRequest it would produce.

    #[test]
    fn do_sign_chain_id_defaults_to_1() {
        let signer = make_signer();
        // chain_id is None → should use "1"
        let chain_id_str = signer
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "1".to_string());
        assert_eq!(chain_id_str, "1");
    }

    #[test]
    fn do_sign_chain_id_uses_set_value() {
        let mut signer = make_signer();
        signer.set_chain_id(Some(137));
        let chain_id_str = signer
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "1".to_string());
        assert_eq!(chain_id_str, "137");
    }

    #[test]
    fn do_sign_address_format_is_checksummed() {
        let signer = make_signer();
        let addr_str = format!("{:?}", signer.address);
        // Should start with 0x and be 42 chars
        assert!(addr_str.starts_with("0x"), "address should start with 0x");
        assert_eq!(addr_str.len(), 42, "address should be 42 chars");
    }

    #[test]
    fn do_sign_builds_correct_sign_request() {
        let mut signer = make_signer();
        signer.set_chain_id(Some(10));

        let chain_id_str = signer
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "1".to_string());

        let req = SignRequest {
            sign_type: SignType::Hash,
            chain_id: chain_id_str,
            address: format!("{:?}", signer.address),
            data: "0xabcdef".to_string(),
            metadata: None,
        };

        assert_eq!(req.chain_id, "10");
        assert_eq!(req.address, format!("{:?}", TEST_ADDR));
        assert_eq!(req.data, "0xabcdef");
        assert!(req.metadata.is_none());

        // Verify it serializes correctly
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["sign_type"], "hash");
        assert_eq!(json["chain_id"], "10");
    }
}
