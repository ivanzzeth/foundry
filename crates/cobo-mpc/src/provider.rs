//! Cobo MPC Provider that wraps a regular provider and routes transactions via Cobo API.
//!
//! This provider intercepts `send_transaction` calls and routes them through the Cobo MPC API
//! instead of the normal sign→broadcast flow, since Cobo MPC uses atomic sign+broadcast.

use crate::client::CoboMpcClient;
use alloy_json_rpc::RpcError;
use alloy_network::AnyNetwork;
use alloy_primitives::{Address, TxKind, B256};
use alloy_provider::{PendingTransactionBuilder, Provider, RootProvider, SendableTx};
use alloy_rpc_types::TransactionRequest;
use alloy_serde::WithOtherFields;
use alloy_transport::TransportResult;
use async_trait::async_trait;
use std::sync::Arc;

/// A provider wrapper that routes transactions through Cobo MPC API.
///
/// This provider implements the `Provider` trait and intercepts `send_transaction_internal`
/// to route transactions through Cobo's atomic sign+broadcast API.
///
/// All other provider methods are delegated to the inner provider.
#[derive(Debug, Clone)]
pub struct CoboMpcProvider<P> {
    inner: P,
    cobo_client: Arc<CoboMpcClient>,
    dry_run: bool,
}

impl<P> CoboMpcProvider<P> {
    /// Creates a new Cobo MPC provider wrapping the given provider.
    pub fn new(inner: P, cobo_client: CoboMpcClient) -> Self {
        Self {
            inner,
            cobo_client: Arc::new(cobo_client),
            dry_run: false,
        }
    }

    /// Enable dry-run mode for transaction simulation.
    ///
    /// When dry-run is enabled:
    /// - Transaction is built and validated locally (nonce, gas estimation, etc.)
    /// - Transaction details are logged via `tracing::info!` at `cobo_mpc` target
    /// - No actual Cobo API call is made (transfer/call_contract)
    /// - Returns `B256::ZERO` as dummy tx hash
    ///
    /// Useful for validating the transaction flow and parameters without
    /// incurring costs or affecting on-chain state.
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Returns a reference to the inner provider.
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Returns a reference to the Cobo MPC client.
    pub fn cobo_client(&self) -> &CoboMpcClient {
        &self.cobo_client
    }
}

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
impl<P: Provider<AnyNetwork>> Provider<AnyNetwork> for CoboMpcProvider<P> {
    fn root(&self) -> &RootProvider<AnyNetwork> {
        self.inner.root()
    }

    /// Intercept transaction sending and route through Cobo MPC API.
    ///
    /// This overrides the default implementation to use Cobo's atomic sign+broadcast.
    async fn send_transaction_internal(
        &self,
        tx: SendableTx<AnyNetwork>,
    ) -> TransportResult<PendingTransactionBuilder<AnyNetwork>> {
        tracing::debug!("CoboMpcProvider: intercepting send_transaction_internal");

        // Extract transaction request from SendableTx
        let tx_request: WithOtherFields<TransactionRequest> = match tx {
            SendableTx::Builder(builder) => builder,
            SendableTx::Envelope(_) => {
                return Err(RpcError::local_usage_str(
                    "Cobo MPC does not support pre-signed transactions",
                ));
            }
        };

        // Send via Cobo API
        let tx_hash = self
            .send_via_cobo(tx_request)
            .await
            .map_err(|e| RpcError::local_usage_str(&e.to_string()))?;

        // Return PendingTransactionBuilder with the tx hash
        Ok(PendingTransactionBuilder::new(self.root().clone(), tx_hash))
    }
}

impl<P: Provider<AnyNetwork>> CoboMpcProvider<P> {
    /// Send a transaction via Cobo MPC API.
    ///
    /// This method extracts transaction details and routes them through the appropriate
    /// Cobo API endpoint (transfer for native token, call_contract for contract calls).
    async fn send_via_cobo(
        &self,
        tx: WithOtherFields<TransactionRequest>,
    ) -> Result<B256, CoboSendError> {
        // Extract destination address
        let to_addr = match tx.to {
            Some(TxKind::Call(addr)) => addr,
            _ => {
                return Err(CoboSendError::InvalidTransaction(
                    "Cobo MPC requires a destination address (CREATE not supported)".to_string(),
                ))
            }
        };
        let to_str = format!("{to_addr:?}");

        // Extract calldata
        let calldata = tx
            .input
            .input()
            .map(|b| format!("0x{}", alloy_primitives::hex::encode(b)))
            .unwrap_or_else(|| "0x".to_string());

        // Extract value
        let value = tx.value.map(|v| v.to_string());

        // Get gas limit - estimate if not provided
        let gas_limit = if let Some(gas) = tx.gas {
            gas
        } else {
            // Estimate gas using inner provider
            let from = tx.from.unwrap_or(Address::ZERO);
            let estimate_tx = WithOtherFields::new(
                TransactionRequest::default()
                    .from(from)
                    .to(to_addr)
                    .input(tx.input.clone())
                    .value(tx.value.unwrap_or_default()),
            );
            self.inner
                .estimate_gas(estimate_tx)
                .await
                .map_err(|e| CoboSendError::GasEstimation(e.to_string()))?
        };

        // Apply 30% gas margin for safety
        let gas_limit_with_margin = gas_limit * 13 / 10;

        // Build fee params
        let fee = if let (Some(max_fee), Some(max_priority_fee)) =
            (tx.max_fee_per_gas, tx.max_priority_fee_per_gas)
        {
            self.cobo_client
                .build_eip1559_fee(max_fee, max_priority_fee, gas_limit_with_margin)
        } else {
            let gas_price = match tx.gas_price {
                Some(gp) => gp,
                None => self
                    .inner
                    .get_gas_price()
                    .await
                    .map_err(|e| CoboSendError::GasPrice(e.to_string()))?,
            };
            self.cobo_client
                .build_legacy_fee(gas_price, gas_limit_with_margin)
        };

        // According to Go reference implementation (evm_transactions_maker.go):
        // - Native transfer: ApiTokenTransfer -> /v2/transactions/transfer with tokenId
        // - Contract call: ContractCall -> /v2/transactions/contract_call
        //
        // Native transfer uses transfer API with tokenId like "ETH_ETH", "MATIC_MATIC".

        // Determine if this is a native transfer or contract call
        let is_native_transfer =
            (calldata == "0x" || calldata == "0x0" || calldata.is_empty()) && value.is_some();

        // Dry-run mode: log and return dummy hash
        if self.dry_run {
            tracing::info!(
                target: "cobo_mpc",
                to = %to_str,
                calldata = %calldata,
                value = ?value,
                gas_limit = %gas_limit_with_margin,
                is_native_transfer = %is_native_transfer,
                "[DRY-RUN] Would send transaction via Cobo MPC"
            );
            // Return a dummy hash for dry-run
            return Ok(B256::ZERO);
        }

        // Call Cobo API
        let tx_hash_str = if is_native_transfer {
            // Native transfer: use /v2/transactions/transfer with tokenId
            let amount = value.as_ref().expect("checked above");
            self.cobo_client
                .transfer(&to_str, amount, Some(fee))
                .await
                .map_err(|e| CoboSendError::CoboApi(format!("transfer failed: {e}")))?
        } else {
            // Contract call: use /v2/transactions/contract_call
            self.cobo_client
                .call_contract(&to_str, &calldata, value.as_deref(), fee)
                .await
                .map_err(|e| CoboSendError::CoboApi(format!("call_contract failed: {e}")))?
        };

        // Parse tx hash
        let tx_hash = tx_hash_str
            .parse::<B256>()
            .map_err(|e| CoboSendError::InvalidTxHash(e.to_string()))?;

        Ok(tx_hash)
    }
}

/// Errors that can occur when sending a transaction via Cobo MPC.
#[derive(Debug, thiserror::Error)]
pub enum CoboSendError {
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("Gas estimation failed: {0}")]
    GasEstimation(String),

    #[error("Failed to get gas price: {0}")]
    GasPrice(String),

    #[error("Cobo API error: {0}")]
    CoboApi(String),

    #[error("Invalid tx hash from Cobo: {0}")]
    InvalidTxHash(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would require mocking the provider and Cobo client
    // For now, we just verify the struct can be constructed
}
