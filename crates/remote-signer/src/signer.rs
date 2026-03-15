//! Remote HTTP signer implementing alloy's Signer and TxSigner traits.
//!
//! Uses `remote-signer-client` for HTTP and auth; signing is performed in
//! `spawn_blocking` because the rs-client is synchronous.

use alloy_consensus::{SignableTransaction, Transaction};
use alloy_dyn_abi::TypedData;
use alloy_network::TxSigner;
use alloy_primitives::{Address, B256, ChainId, Signature};
use alloy_signer::Signer;
use async_trait::async_trait;
use remote_signer_client::evm::{SignRequest, SignResponse};
use serde_json::Map;

/// TLS config for mTLS; certificate verification is always enabled (skip_verify is never set).
pub use remote_signer_client::TlsConfig;

/// Config stored for building the rs-client inside spawn_blocking (avoids
/// creating reqwest::blocking::Client in async context, which can panic).
#[derive(Clone)]
struct RemoteSignerConfig {
    base_url: String,
    api_key_id: String,
    api_key_hex: Option<String>,
    api_key_file: Option<String>,
    tls: Option<TlsConfig>,
}

/// Either lazy config (build client in spawn_blocking) or pre-built client (e.g. tests).
#[derive(Clone)]
enum ClientBacking {
    Config(RemoteSignerConfig),
    Client(remote_signer_client::Client),
}

/// Remote HTTP signer that delegates all signing to a remote service.
#[derive(Clone)]
pub struct RemoteHttpSigner {
    backing: ClientBacking,
    address: Address,
    chain_id: Option<ChainId>,
}

impl std::fmt::Debug for RemoteHttpSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteHttpSigner")
            .field("address", &self.address)
            .field("chain_id", &self.chain_id)
            .finish_non_exhaustive()
    }
}

impl RemoteHttpSigner {
    /// Creates a new remote HTTP signer using the given rs-client (for tests; client built in sync context).
    pub fn with_client(client: remote_signer_client::Client, address: Address) -> Self {
        Self {
            backing: ClientBacking::Client(client),
            address,
            chain_id: None,
        }
    }

    /// Creates a new remote HTTP signer from URL and API key. The rs-client is built inside
    /// `spawn_blocking` when signing (avoids creating reqwest::blocking::Client in async context).
    /// Exactly one of `api_key_hex` or `api_key_file` must be set.
    /// If `tls` is provided, mTLS is used. `skip_verify` in `tls` is respected (set via REMOTE_SIGNER_TLS_INSECURE_SKIP_VERIFY for testing).
    pub fn new(
        base_url: &str,
        api_key_id: String,
        api_key_hex: Option<&str>,
        api_key_file: Option<&str>,
        address: Address,
        tls: Option<TlsConfig>,
    ) -> eyre::Result<Self> {
        match (api_key_hex, api_key_file) {
            (Some(hex), None) => {
                let hex_trimmed = hex.strip_prefix("0x").unwrap_or(hex);
                let bytes =
                    hex::decode(hex_trimmed).map_err(|e| eyre::eyre!("invalid API key hex: {e}"))?;
                if bytes.len() != 32 && bytes.len() != 64 {
                    return Err(eyre::eyre!(
                        "Ed25519 private key must be 32 or 64 bytes, got {}",
                        bytes.len()
                    ));
                }
            }
            (None, Some(_)) => {}
            _ => return Err(eyre::eyre!("exactly one of api_key_hex or api_key_file is required")),
        }
        Ok(Self {
            backing: ClientBacking::Config(RemoteSignerConfig {
                base_url: base_url.trim_end_matches('/').to_string(),
                api_key_id,
                api_key_hex: api_key_hex.map(String::from),
                api_key_file: api_key_file.map(String::from),
                tls,
            }),
            address,
            chain_id: None,
        })
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

        let y_parity = if v >= 27 { v - 27 != 0 } else { v != 0 };

        Ok(Signature::new(r, s, y_parity))
    }

    fn build_transaction_payload(tx: &dyn Transaction) -> serde_json::Value {
        let mut obj = Map::new();
        if let Some(to_addr) = tx.to() {
            obj.insert("to".into(), serde_json::Value::String(format!("{:?}", to_addr)));
        }
        obj.insert("value".into(), serde_json::Value::String(tx.value().to_string()));
        obj.insert(
            "data".into(),
            serde_json::Value::String(format!("0x{}", hex::encode(tx.input().as_ref()))),
        );
        obj.insert("gas".into(), serde_json::Number::from(tx.gas_limit()).into());
        obj.insert("nonce".into(), serde_json::Number::from(tx.nonce()).into());
        let tx_type = if tx.is_dynamic_fee() {
            "eip1559"
        } else if tx.access_list().is_some() {
            "eip2930"
        } else {
            "legacy"
        };
        obj.insert("txType".into(), serde_json::Value::String(tx_type.to_string()));
        if tx_type == "legacy" || tx_type == "eip2930" {
            let gas_price = tx.gas_price().unwrap_or(0);
            obj.insert("gasPrice".into(), serde_json::Value::String(gas_price.to_string()));
        } else {
            obj.insert(
                "gasFeeCap".into(),
                serde_json::Value::String(tx.max_fee_per_gas().to_string()),
            );
            obj.insert(
                "gasTipCap".into(),
                serde_json::Value::String(tx.max_priority_fee_per_gas().unwrap_or(0).to_string()),
            );
        }
        serde_json::Value::Object(obj)
    }

    async fn do_sign(
        &self,
        sign_type: &str,
        payload: serde_json::Value,
    ) -> alloy_signer::Result<Signature> {
        let chain_id_str = self
            .chain_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "1".to_string());

        let req = SignRequest {
            chain_id: chain_id_str,
            signer_address: format!("{:?}", self.address),
            sign_type: sign_type.to_string(),
            payload,
        };

        let backing = self.backing.clone();
        let resp = tokio::task::spawn_blocking(move || {
            let client = match backing {
                ClientBacking::Client(c) => c,
                ClientBacking::Config(cfg) => {
                    let mut c = remote_signer_client::Config::default();
                    c.base_url = cfg.base_url.clone();
                    c.api_key_id = cfg.api_key_id.clone();
                    if let Some(hex) = &cfg.api_key_hex {
                        c.private_key_hex = Some(hex.clone());
                    } else if let Some(path) = &cfg.api_key_file {
                        c.private_key_file = Some(path.clone());
                    }
                    c.tls = cfg.tls.clone();
                    remote_signer_client::Client::new(c)
                        .map_err(|e| alloy_signer::Error::other(e.to_string()))?
                }
            };
            client.evm.sign.execute(&req).map_err(|e| alloy_signer::Error::other(e.to_string()))
        })
        .await
        .map_err(|e| alloy_signer::Error::other(e))?
        .map_err(|e| alloy_signer::Error::other(e))?;

        Self::extract_signature(&resp)
    }
}

#[async_trait]
impl Signer for RemoteHttpSigner {
    async fn sign_hash(&self, hash: &B256) -> alloy_signer::Result<Signature> {
        let payload = serde_json::json!({
            "hash": format!("0x{}", hex::encode(hash.as_slice()))
        });
        self.do_sign("hash", payload).await
    }

    async fn sign_message(&self, message: &[u8]) -> alloy_signer::Result<Signature> {
        let message_str = String::from_utf8(message.to_vec())
            .unwrap_or_else(|_| format!("0x{}", hex::encode(message)));
        let payload = serde_json::json!({
            "message": message_str
        });
        self.do_sign("personal", payload).await
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
        typed_data: &TypedData,
    ) -> alloy_signer::Result<Signature> {
        let typed_data_value = serde_json::to_value(typed_data)
            .map_err(|e| alloy_signer::Error::other(e))?;
        let payload = serde_json::json!({
            "typed_data": typed_data_value
        });
        self.do_sign("typed_data", payload).await
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
        let transaction = Self::build_transaction_payload(tx as &dyn Transaction);
        let payload = serde_json::json!({ "transaction": transaction });

        // Prefer chain_id from the transaction itself (set by the provider
        // from --chain / RPC), falling back to self.chain_id / default "1".
        let chain_id_str = tx
            .chain_id()
            .map(|id| id.to_string())
            .or_else(|| self.chain_id.map(|id| id.to_string()))
            .unwrap_or_else(|| "1".to_string());

        let req = SignRequest {
            chain_id: chain_id_str,
            signer_address: format!("{:?}", self.address),
            sign_type: "transaction".to_string(),
            payload,
        };

        let backing = self.backing.clone();
        let resp = tokio::task::spawn_blocking(move || {
            let client = match backing {
                ClientBacking::Client(c) => c,
                ClientBacking::Config(cfg) => {
                    let mut c = remote_signer_client::Config::default();
                    c.base_url = cfg.base_url.clone();
                    c.api_key_id = cfg.api_key_id.clone();
                    if let Some(hex) = &cfg.api_key_hex {
                        c.private_key_hex = Some(hex.clone());
                    } else if let Some(path) = &cfg.api_key_file {
                        c.private_key_file = Some(path.clone());
                    }
                    c.tls = cfg.tls.clone();
                    remote_signer_client::Client::new(c)
                        .map_err(|e| alloy_signer::Error::other(e.to_string()))?
                }
            };
            client
                .evm
                .sign
                .execute(&req)
                .map_err(|e| alloy_signer::Error::other(e.to_string()))
        })
        .await
        .map_err(|e| alloy_signer::Error::other(e))?
        .map_err(|e| alloy_signer::Error::other(e))?;

        Self::extract_signature(&resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::TxLegacy;
    use alloy_primitives::{address, bytes, TxKind};

    const TEST_KEY_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";
    const TEST_ADDR: Address = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");

    fn make_signer() -> RemoteHttpSigner {
        RemoteHttpSigner::new(
            "http://localhost:9999",
            "test-key".into(),
            Some(TEST_KEY_HEX),
            None,
            TEST_ADDR,
            None,
        )
        .unwrap()
    }

    #[test]
    fn new_valid_construction() {
        let signer = RemoteHttpSigner::new(
            "http://localhost:8080",
            "key-id".into(),
            Some(TEST_KEY_HEX),
            None,
            TEST_ADDR,
            None,
        );
        assert!(signer.is_ok());
    }

    #[test]
    fn new_invalid_key_fails() {
        let signer = RemoteHttpSigner::new(
            "http://localhost:8080",
            "key-id".into(),
            Some("bad"),
            None,
            TEST_ADDR,
            None,
        );
        assert!(signer.is_err());
    }

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

    fn make_response(sig_hex: Option<String>) -> SignResponse {
        SignResponse {
            request_id: "req-1".to_string(),
            status: "completed".to_string(),
            signature: sig_hex,
            signed_data: None,
            message: None,
            rule_matched_id: None,
        }
    }

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
        assert!(!sig.v(), "v=27 should map to y_parity=false");
    }

    #[test]
    fn extract_signature_v28() {
        let resp = make_response(Some(fake_sig_hex(28)));
        let sig = RemoteHttpSigner::extract_signature(&resp).unwrap();
        assert!(sig.v(), "v=28 should map to y_parity=true");
    }

    #[test]
    fn extract_signature_no_signature_field() {
        let resp = make_response(None);
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("no signature"), "got: {err}");
    }

    #[test]
    fn extract_signature_invalid_hex() {
        let resp = make_response(Some("0xZZZZZZ".to_string()));
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
    }

    #[test]
    fn extract_signature_wrong_length() {
        let resp = make_response(Some("0x0011".to_string()));
        let result = RemoteHttpSigner::extract_signature(&resp);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("expected 65 byte"), "got: {err}");
    }

    #[test]
    fn build_transaction_payload_legacy() {
        let to_addr = address!("0x742d35Cc6634C0532925a3b844Bc454e4438f44e");
        let tx = TxLegacy {
            chain_id: Some(1),
            nonce: 2,
            gas_price: 20_000_000_000,
            gas_limit: 21000,
            to: TxKind::Call(to_addr),
            value: alloy_primitives::U256::from(1_000_000_000u64),
            input: bytes!("deadbeef"),
        };
        let payload = RemoteHttpSigner::build_transaction_payload(&tx);
        assert_eq!(payload["to"], format!("{:?}", to_addr));
        assert_eq!(payload["value"], "1000000000");
        assert_eq!(payload["data"], "0xdeadbeef");
        assert_eq!(payload["gas"], 21000);
        assert_eq!(payload["nonce"], 2);
        assert_eq!(payload["txType"], "legacy");
        assert_eq!(payload["gasPrice"], "20000000000");
    }
}
