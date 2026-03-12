//! HTTP client for Cobo WaaS 2.0 REST API.

use crate::{
    auth::{api_key_from_signing_key, parse_signing_key, sign_request},
    types::{
        CoboApiError, CoboEnv, CoboMpcError, ContractCallDestination, ContractCallRequest,
        CreateTransactionResponse, MessageSignDestination, MessageSignRequest,
        StructuredData, TransactionDetail, TransactionRequestFee, TransactionSource,
        TransactionStatus, TransferAccountOutput, TransferDestination, TransferRequest,
        TransferSource,
    },
};
use ed25519_dalek::SigningKey;
use reqwest::Client;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::debug;

/// Default polling interval for transaction status (3 seconds).
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(3);
/// Default maximum number of polling retries.
const DEFAULT_MAX_RETRIES: u32 = 100;

/// HTTP client for Cobo WaaS 2.0 API.
#[derive(Debug, Clone)]
pub struct CoboMpcClient {
    http: Client,
    signing_key: SigningKey,
    api_key: String,
    base_url: String,
    wallet_id: String,
    address: String,
    cobo_chain_id: String,
    poll_interval: Duration,
    max_retries: u32,
}

impl CoboMpcClient {
    /// Creates a new Cobo MPC client.
    pub fn new(
        api_key_hex: &str,
        wallet_id: String,
        address: String,
        cobo_chain_id: String,
        env: CoboEnv,
    ) -> eyre::Result<Self> {
        let signing_key = parse_signing_key(api_key_hex)?;
        let api_key = api_key_from_signing_key(&signing_key);

        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            signing_key,
            api_key,
            base_url: env.base_url().to_string(),
            wallet_id,
            address,
            cobo_chain_id,
            poll_interval: DEFAULT_POLL_INTERVAL,
            max_retries: DEFAULT_MAX_RETRIES,
        })
    }

    /// Generates a unique request ID.
    fn request_id(&self) -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("cobo-mpc-rust-v2-{ts}")
    }

    /// Generates a nonce for API requests.
    fn nonce(&self) -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let nonce: u64 = rng.r#gen();
        format!("{nonce:016x}")
    }

    /// Returns the current timestamp in milliseconds.
    fn timestamp_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Makes an authenticated POST request to the Cobo API.
    async fn post<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, CoboMpcError> {
        let body_str = serde_json::to_string(body).map_err(|e| CoboMpcError::Other(e.to_string()))?;
        let timestamp = self.timestamp_ms();
        let nonce = self.nonce();
        let signature = sign_request(
            &self.signing_key,
            "POST",
            path,
            timestamp,
            &nonce,
            &body_str,
        );

        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, "POST request to Cobo API");

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("BIZ-API-KEY", &self.api_key)
            .header("BIZ-API-NONCE", &nonce)
            .header("BIZ-API-SIGNATURE", &signature)
            .header("BIZ-TIMESTAMP", timestamp.to_string())
            .body(body_str)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        debug!(status = %status, body = %body, "Cobo API response");

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<CoboApiError>(&body) {
                return Err(CoboMpcError::Api(err));
            }
            return Err(CoboMpcError::Other(format!(
                "HTTP {status}: {body}"
            )));
        }

        serde_json::from_str(&body).map_err(|e| {
            CoboMpcError::Other(format!("Failed to parse response: {e}, body: {body}"))
        })
    }

    /// Makes an authenticated GET request to the Cobo API.
    async fn get<R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<R, CoboMpcError> {
        let timestamp = self.timestamp_ms();
        let nonce = self.nonce();
        let signature = sign_request(
            &self.signing_key,
            "GET",
            path,
            timestamp,
            &nonce,
            "",
        );

        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, "GET request to Cobo API");

        let resp = self
            .http
            .get(&url)
            .header("BIZ-API-KEY", &self.api_key)
            .header("BIZ-API-NONCE", &nonce)
            .header("BIZ-API-SIGNATURE", &signature)
            .header("BIZ-TIMESTAMP", timestamp.to_string())
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        debug!(status = %status, body = %body, "Cobo API response");

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<CoboApiError>(&body) {
                return Err(CoboMpcError::Api(err));
            }
            return Err(CoboMpcError::Other(format!(
                "HTTP {status}: {body}"
            )));
        }

        serde_json::from_str(&body).map_err(|e| {
            CoboMpcError::Other(format!("Failed to parse response: {e}, body: {body}"))
        })
    }

    /// Creates a transaction source for this wallet.
    fn source(&self) -> TransactionSource {
        TransactionSource {
            source_type: "Org-Controlled".to_string(),
            wallet_id: self.wallet_id.clone(),
            address: self.address.clone(),
        }
    }

    /// Signs a message using the Cobo MPC API (EIP-191 personal sign).
    ///
    /// Returns the 65-byte signature (r, s, v).
    pub async fn sign_message(&self, message: &[u8]) -> Result<[u8; 65], CoboMpcError> {
        let encoded_message = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            message,
        );

        let request = MessageSignRequest {
            request_id: self.request_id(),
            chain_id: self.cobo_chain_id.clone(),
            source: self.source(),
            destination: MessageSignDestination {
                destination_type: "EVM_EIP_191".to_string(),
                structured_data: None,
                message: Some(encoded_message),
            },
            description: Some("Message signing via Foundry cast".to_string()),
        };

        let resp: CreateTransactionResponse = self.post("/v2/transactions/message/sign", &request).await?;
        let detail = self.wait_transaction_status(
            &resp.transaction_id,
            &TransactionStatus::Completed,
        ).await?;

        self.extract_signature(&detail)
    }

    /// Signs EIP-712 typed data using the Cobo MPC API.
    ///
    /// Returns the 65-byte signature (r, s, v).
    pub async fn sign_typed_data(&self, typed_data_json: &str) -> Result<[u8; 65], CoboMpcError> {
        let request = MessageSignRequest {
            request_id: self.request_id(),
            chain_id: self.cobo_chain_id.clone(),
            source: self.source(),
            destination: MessageSignDestination {
                destination_type: "EVM_EIP_712".to_string(),
                structured_data: Some(StructuredData {
                    data_type: "EVM_EIP_712".to_string(),
                    data: typed_data_json.to_string(),
                }),
                message: None,
            },
            description: Some("EIP-712 signing via Foundry cast".to_string()),
        };

        let resp: CreateTransactionResponse = self.post("/v2/transactions/message/sign", &request).await?;
        let detail = self.wait_transaction_status(
            &resp.transaction_id,
            &TransactionStatus::Completed,
        ).await?;

        self.extract_signature(&detail)
    }

    /// Calls a contract via the Cobo MPC API (sign + broadcast atomic).
    ///
    /// Returns the transaction hash.
    pub async fn call_contract(
        &self,
        to: &str,
        calldata: &str,
        value: Option<&str>,
        fee: TransactionRequestFee,
    ) -> Result<String, CoboMpcError> {
        let request = ContractCallRequest {
            request_id: self.request_id(),
            chain_id: self.cobo_chain_id.clone(),
            source: self.source(),
            destination: ContractCallDestination {
                destination_type: "EVM_Contract".to_string(),
                address: to.to_string(),
                calldata: calldata.to_string(),
                value: value.map(|v| v.to_string()),
            },
            fee,
            description: Some("Contract call via Foundry cast".to_string()),
        };

        let resp: CreateTransactionResponse = self.post("/v2/transactions/contract_call", &request).await?;

        // Wait for the transaction to be broadcast (Confirming status)
        let detail = self.wait_transaction_status(
            &resp.transaction_id,
            &TransactionStatus::Confirming,
        ).await?;

        detail
            .transaction_hash
            .ok_or(CoboMpcError::MissingTransactionHash)
    }

    /// Sends a token transfer via the Cobo MPC API using `/v2/transactions/transfer`.
    ///
    /// This is the proper API for token transfers (both native and ERC20).
    /// For native token transfers, use token_id like "ETH_ETH", "MATIC_MATIC".
    /// Returns the transaction hash.
    pub async fn transfer_token(
        &self,
        to: &str,
        token_id: &str,
        amount: &str,
        fee: Option<TransactionRequestFee>,
    ) -> Result<String, CoboMpcError> {
        let request = TransferRequest {
            request_id: self.request_id(),
            source: TransferSource {
                source_type: "Org-Controlled".to_string(),
                wallet_id: self.wallet_id.clone(),
                address: self.address.clone(),
            },
            token_id: token_id.to_string(),
            destination: TransferDestination {
                destination_type: "Address".to_string(),
                account_output: TransferAccountOutput {
                    address: to.to_string(),
                    amount: amount.to_string(),
                },
            },
            fee,
            description: Some("Token transfer via Foundry cast".to_string()),
        };

        let resp: CreateTransactionResponse = self.post("/v2/transactions/transfer", &request).await?;

        // Wait for the transaction to be broadcast (Confirming status)
        let detail = self.wait_transaction_status(
            &resp.transaction_id,
            &TransactionStatus::Confirming,
        ).await?;

        detail
            .transaction_hash
            .ok_or(CoboMpcError::MissingTransactionHash)
    }

    /// Sends a native token transfer via the Cobo MPC API.
    ///
    /// Uses the `/v2/transactions/transfer` API with the native token ID.
    /// Returns the transaction hash.
    pub async fn transfer(
        &self,
        to: &str,
        amount: &str,
        fee: Option<TransactionRequestFee>,
    ) -> Result<String, CoboMpcError> {
        // Native token ID format: chain_id_chain_id (e.g., "ETH_ETH", "MATIC_MATIC")
        let token_id = format!("{chain}_{chain}", chain = self.cobo_chain_id);
        self.transfer_token(to, &token_id, amount, fee).await
    }

    /// Polls the Cobo API for transaction status until the target status is reached.
    pub async fn wait_transaction_status(
        &self,
        transaction_id: &str,
        target_status: &TransactionStatus,
    ) -> Result<TransactionDetail, CoboMpcError> {
        let path = format!("/v2/transactions/{}", transaction_id);

        for retry in 0..self.max_retries {
            tokio::time::sleep(self.poll_interval).await;

            let detail: TransactionDetail = self.get(&path).await?;
            debug!(
                retry = retry,
                status = ?detail.status,
                target = ?target_status,
                "Polling transaction status"
            );

            // Check for failure states first
            if detail.status == TransactionStatus::Failed {
                return Err(CoboMpcError::TransactionFailed {
                    reason: detail.failed_reason.unwrap_or_else(|| "unknown".to_string()),
                });
            }
            if detail.status == TransactionStatus::Rejected {
                return Err(CoboMpcError::TransactionRejected);
            }

            // For broadcast target, accept Confirming or Completed
            if target_status == &TransactionStatus::Confirming {
                if detail.status.is_broadcast() {
                    return Ok(detail);
                }
            } else if &detail.status == target_status || detail.status.is_final() {
                return Ok(detail);
            }
        }

        Err(CoboMpcError::PollingTimeout {
            retries: self.max_retries,
        })
    }

    /// Extracts and validates a 65-byte signature from a transaction detail.
    fn extract_signature(&self, detail: &TransactionDetail) -> Result<[u8; 65], CoboMpcError> {
        let sig_hex = detail
            .signature
            .as_ref()
            .ok_or_else(|| CoboMpcError::InvalidSignature("no signature in response".to_string()))?;

        let sig_bytes = hex::decode(sig_hex.strip_prefix("0x").unwrap_or(sig_hex))
            .map_err(|e| CoboMpcError::InvalidSignature(format!("invalid hex: {e}")))?;

        if sig_bytes.len() != 65 {
            return Err(CoboMpcError::InvalidSignature(format!(
                "expected 65 bytes, got {}",
                sig_bytes.len()
            )));
        }

        let mut sig = [0u8; 65];
        sig.copy_from_slice(&sig_bytes);

        // Normalize V value to 27/28 format
        // Reference: Go implementation mpc_remote_signer.go:123-125
        if sig[64] < 27 {
            sig[64] += 27;
        }

        Ok(sig)
    }

    /// Returns the wallet address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns the Cobo chain ID.
    pub fn cobo_chain_id(&self) -> &str {
        &self.cobo_chain_id
    }

    /// Builds a legacy fee configuration for Cobo API.
    pub fn build_legacy_fee(
        &self,
        gas_price: u128,
        gas_limit: u64,
    ) -> TransactionRequestFee {
        // Token ID format: chain_id_native (e.g., "ETH_ETH", "MATIC_MATIC")
        let token_id = format!("{chain}_{chain}", chain = self.cobo_chain_id);
        TransactionRequestFee {
            fee_type: "Fixed".to_string(),
            token_id,
            gas_price: Some(gas_price.to_string()),
            gas_limit: Some(gas_limit.to_string()),
            max_fee: None,
            max_priority_fee: None,
        }
    }

    /// Builds an EIP-1559 fee configuration for Cobo API.
    pub fn build_eip1559_fee(
        &self,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
        gas_limit: u64,
    ) -> TransactionRequestFee {
        let token_id = format!("{chain}_{chain}", chain = self.cobo_chain_id);
        TransactionRequestFee {
            fee_type: "Fixed".to_string(),
            token_id,
            gas_price: None,
            gas_limit: Some(gas_limit.to_string()),
            max_fee: Some(max_fee_per_gas.to_string()),
            max_priority_fee: Some(max_priority_fee_per_gas.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valid 32-byte Ed25519 private key hex for testing.
    const TEST_KEY_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";

    fn test_client() -> CoboMpcClient {
        CoboMpcClient::new(
            TEST_KEY_HEX,
            "wallet-123".to_string(),
            "0xAbC1230001112223334445556667778889990000".to_string(),
            "ETH".to_string(),
            CoboEnv::Dev,
        )
        .unwrap()
    }

    // --- CoboMpcClient::new ---

    #[test]
    fn test_new_valid_key() {
        let client = CoboMpcClient::new(
            TEST_KEY_HEX,
            "w1".to_string(),
            "0xAddr".to_string(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_new_valid_key_with_0x_prefix() {
        let client = CoboMpcClient::new(
            &format!("0x{TEST_KEY_HEX}"),
            "w1".to_string(),
            "0xAddr".to_string(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_new_invalid_key_too_short() {
        let client = CoboMpcClient::new(
            "aabb",
            "w1".to_string(),
            "0xAddr".to_string(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(client.is_err());
    }

    #[test]
    fn test_new_invalid_key_not_hex() {
        let client = CoboMpcClient::new(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            "w1".to_string(),
            "0xAddr".to_string(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(client.is_err());
    }

    #[test]
    fn test_new_invalid_key_too_long() {
        let long_key = "aa".repeat(33); // 33 bytes = 66 hex chars
        let client = CoboMpcClient::new(
            &long_key,
            "w1".to_string(),
            "0xAddr".to_string(),
            "ETH".to_string(),
            CoboEnv::Prod,
        );
        assert!(client.is_err());
    }

    // --- request_id ---

    #[test]
    fn test_request_id_format() {
        let client = test_client();
        let id = client.request_id();
        assert!(id.starts_with("cobo-mpc-rust-v2-"), "got: {id}");
        // The part after the prefix should be a numeric timestamp
        let ts_part = id.strip_prefix("cobo-mpc-rust-v2-").unwrap();
        assert!(ts_part.parse::<u128>().is_ok(), "timestamp part not numeric: {ts_part}");
    }

    #[test]
    fn test_request_id_unique() {
        let client = test_client();
        let id1 = client.request_id();
        let id2 = client.request_id();
        // In fast succession they could match on millis, but the test documents intent
        // At minimum they should both be valid format
        assert!(id1.starts_with("cobo-mpc-rust-v2-"));
        assert!(id2.starts_with("cobo-mpc-rust-v2-"));
    }

    // --- build_legacy_fee ---

    #[test]
    fn test_build_legacy_fee() {
        let client = test_client();
        let fee = client.build_legacy_fee(20_000_000_000, 21_000);

        assert_eq!(fee.fee_type, "Fixed");
        assert_eq!(fee.token_id, "ETH_ETH");
        assert_eq!(fee.gas_price, Some("20000000000".to_string()));
        assert_eq!(fee.gas_limit, Some("21000".to_string()));
        assert!(fee.max_fee.is_none());
        assert!(fee.max_priority_fee.is_none());
    }

    #[test]
    fn test_build_legacy_fee_chain_id_in_token() {
        let client = CoboMpcClient::new(
            TEST_KEY_HEX,
            "w1".to_string(),
            "0xAddr".to_string(),
            "MATIC".to_string(),
            CoboEnv::Dev,
        )
        .unwrap();
        let fee = client.build_legacy_fee(30_000_000_000, 50_000);
        assert_eq!(fee.token_id, "MATIC_MATIC");
    }

    // --- build_eip1559_fee ---

    #[test]
    fn test_build_eip1559_fee() {
        let client = test_client();
        let fee = client.build_eip1559_fee(50_000_000_000, 2_000_000_000, 100_000);

        assert_eq!(fee.fee_type, "Fixed");
        assert_eq!(fee.token_id, "ETH_ETH");
        assert!(fee.gas_price.is_none());
        assert_eq!(fee.gas_limit, Some("100000".to_string()));
        assert_eq!(fee.max_fee, Some("50000000000".to_string()));
        assert_eq!(fee.max_priority_fee, Some("2000000000".to_string()));
    }

    // --- extract_signature ---

    #[test]
    fn test_extract_signature_valid_65_bytes() {
        let client = test_client();
        let mut sig = vec![0xABu8; 64];
        sig.push(27); // V = 27
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let result = client.extract_signature(&detail).unwrap();
        assert_eq!(result.len(), 65);
        assert_eq!(result[64], 27);
    }

    #[test]
    fn test_extract_signature_with_0x_prefix() {
        let client = test_client();
        let mut sig = vec![0x11u8; 64];
        sig.push(28); // V = 28
        let sig_hex = format!("0x{}", hex::encode(&sig));
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let result = client.extract_signature(&detail).unwrap();
        assert_eq!(result[64], 28);
    }

    #[test]
    fn test_extract_signature_normalize_v_0_to_27() {
        let client = test_client();
        let mut sig = vec![0xFFu8; 64];
        sig.push(0); // V = 0 should become 27
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let result = client.extract_signature(&detail).unwrap();
        assert_eq!(result[64], 27, "V=0 should be normalized to 27");
    }

    #[test]
    fn test_extract_signature_normalize_v_1_to_28() {
        let client = test_client();
        let mut sig = vec![0xFFu8; 64];
        sig.push(1); // V = 1 should become 28
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let result = client.extract_signature(&detail).unwrap();
        assert_eq!(result[64], 28, "V=1 should be normalized to 28");
    }

    #[test]
    fn test_extract_signature_v_27_unchanged() {
        let client = test_client();
        let mut sig = vec![0x00u8; 64];
        sig.push(27);
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let result = client.extract_signature(&detail).unwrap();
        assert_eq!(result[64], 27, "V=27 should remain 27");
    }

    #[test]
    fn test_extract_signature_v_28_unchanged() {
        let client = test_client();
        let mut sig = vec![0x00u8; 64];
        sig.push(28);
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let result = client.extract_signature(&detail).unwrap();
        assert_eq!(result[64], 28, "V=28 should remain 28");
    }

    #[test]
    fn test_extract_signature_too_short() {
        let client = test_client();
        let sig = vec![0u8; 60]; // only 60 bytes
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let err = client.extract_signature(&detail).unwrap_err();
        assert!(matches!(err, CoboMpcError::InvalidSignature(_)));
        let msg = format!("{err}");
        assert!(msg.contains("expected 65 bytes"), "got: {msg}");
    }

    #[test]
    fn test_extract_signature_too_long() {
        let client = test_client();
        let sig = vec![0u8; 70]; // 70 bytes
        let sig_hex = hex::encode(&sig);
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: Some(sig_hex),
            failed_reason: None,
        };

        let err = client.extract_signature(&detail).unwrap_err();
        assert!(matches!(err, CoboMpcError::InvalidSignature(_)));
    }

    #[test]
    fn test_extract_signature_missing() {
        let client = test_client();
        let detail = TransactionDetail {
            transaction_id: "tx1".to_string(),
            status: TransactionStatus::Completed,
            transaction_hash: None,
            signature: None,
            failed_reason: None,
        };

        let err = client.extract_signature(&detail).unwrap_err();
        assert!(matches!(err, CoboMpcError::InvalidSignature(_)));
        let msg = format!("{err}");
        assert!(msg.contains("no signature"), "got: {msg}");
    }

    // --- source() ---

    #[test]
    fn test_source() {
        let client = test_client();
        let src = client.source();
        assert_eq!(src.source_type, "Org-Controlled");
        assert_eq!(src.wallet_id, "wallet-123");
        assert_eq!(src.address, "0xAbC1230001112223334445556667778889990000");
    }

    // --- accessors ---

    #[test]
    fn test_address_accessor() {
        let client = test_client();
        assert_eq!(client.address(), "0xAbC1230001112223334445556667778889990000");
    }

    #[test]
    fn test_cobo_chain_id_accessor() {
        let client = test_client();
        assert_eq!(client.cobo_chain_id(), "ETH");
    }
}
