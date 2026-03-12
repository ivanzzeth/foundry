//! HTTP client for the remote-signer service.

use crate::{
    auth::{parse_signing_key, sign_request},
    types::*,
};
use ed25519_dalek::SigningKey;
use reqwest::Client;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::debug;

/// Default polling interval (2 seconds).
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Default polling timeout (5 minutes).
const DEFAULT_POLL_TIMEOUT: Duration = Duration::from_secs(300);

/// HTTP client for the remote-signer service.
#[derive(Debug, Clone)]
pub struct RemoteSignerClient {
    http: Client,
    base_url: String,
    api_key_id: String,
    signing_key: SigningKey,
    poll_interval: Duration,
    poll_timeout: Duration,
}

impl RemoteSignerClient {
    /// Creates a new remote signer client.
    pub fn new(
        base_url: &str,
        api_key_id: String,
        api_key_hex: &str,
    ) -> eyre::Result<Self> {
        let signing_key = parse_signing_key(api_key_hex)?;
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key_id,
            signing_key,
            poll_interval: DEFAULT_POLL_INTERVAL,
            poll_timeout: DEFAULT_POLL_TIMEOUT,
        })
    }

    /// Returns the current timestamp in milliseconds (matches Go reference).
    fn timestamp_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    /// Makes an authenticated request to the remote signer.
    ///
    /// Authentication uses Ed25519 signatures matching the Go reference implementation:
    /// - Timestamp in milliseconds
    /// - Signature format: `{timestamp_ms}|{method}|{path}|{sha256(body)}`
    /// - Signature is base64 encoded
    async fn request<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<&T>,
    ) -> Result<R, RemoteSignerError> {
        let body_bytes = if let Some(body) = body {
            serde_json::to_vec(body).map_err(|e| RemoteSignerError::Other(e.to_string()))?
        } else {
            Vec::new()
        };

        let timestamp_ms = self.timestamp_ms();
        let signature = sign_request(
            &self.signing_key,
            method,
            path,
            timestamp_ms,
            &body_bytes,
        );

        let url = format!("{}{}", self.base_url, path);
        debug!(url = %url, method = %method, "Request to remote signer");

        let mut req = match method {
            "GET" => self.http.get(&url),
            "POST" => self.http.post(&url),
            _ => return Err(RemoteSignerError::Other(format!("Unsupported method: {method}"))),
        };

        // Headers match Go reference: X-API-Key-ID, X-Timestamp, X-Signature
        // No X-Nonce header (server doesn't use it)
        req = req
            .header("Content-Type", "application/json")
            .header("X-API-Key-ID", &self.api_key_id)
            .header("X-Timestamp", timestamp_ms.to_string())
            .header("X-Signature", &signature);

        if !body_bytes.is_empty() {
            req = req.body(body_bytes);
        }

        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        debug!(status = %status, body = %body, "Remote signer response");

        if !status.is_success() {
            return Err(RemoteSignerError::ServerError {
                status: status.as_u16(),
                body,
            });
        }

        serde_json::from_str(&body).map_err(|e| {
            RemoteSignerError::Other(format!("Failed to parse response: {e}, body: {body}"))
        })
    }

    /// Health check.
    pub async fn health(&self) -> Result<HealthResponse, RemoteSignerError> {
        self.request::<(), HealthResponse>("GET", "/health", None).await
    }

    /// Submits a sign request and polls until completion.
    pub async fn sign(&self, req: &SignRequest) -> Result<SignResponse, RemoteSignerError> {
        let initial: SignResponse = self.request("POST", "/api/v1/evm/sign", Some(req)).await?;

        if initial.status == RequestStatus::Completed {
            return Ok(initial);
        }

        if initial.status.is_final() {
            return match initial.status {
                RequestStatus::Rejected => Err(RemoteSignerError::Rejected {
                    reason: initial.error.unwrap_or_else(|| "no reason".to_string()),
                }),
                RequestStatus::Failed => Err(RemoteSignerError::Failed {
                    reason: initial.error.unwrap_or_else(|| "no reason".to_string()),
                }),
                _ => Ok(initial),
            };
        }

        // Poll for completion
        self.poll_request(&initial.request_id).await
    }

    /// Exposed timestamp for testing.
    #[cfg(test)]
    pub(crate) fn test_timestamp_ms(&self) -> i64 {
        self.timestamp_ms()
    }

    /// Polls a sign request until it reaches a final state.
    async fn poll_request(&self, request_id: &str) -> Result<SignResponse, RemoteSignerError> {
        let path = format!("/api/v1/evm/requests/{}", request_id);
        let start = Instant::now();

        loop {
            if start.elapsed() > self.poll_timeout {
                return Err(RemoteSignerError::PollingTimeout {
                    elapsed_secs: self.poll_timeout.as_secs(),
                });
            }

            tokio::time::sleep(self.poll_interval).await;

            let resp: SignResponse = self.request("GET", &path, None::<&()>).await?;
            debug!(
                request_id = %request_id,
                status = ?resp.status,
                "Polling sign request"
            );

            match resp.status {
                RequestStatus::Completed => return Ok(resp),
                RequestStatus::Rejected => {
                    return Err(RemoteSignerError::Rejected {
                        reason: resp.error.unwrap_or_else(|| "no reason".to_string()),
                    });
                }
                RequestStatus::Failed => {
                    return Err(RemoteSignerError::Failed {
                        reason: resp.error.unwrap_or_else(|| "no reason".to_string()),
                    });
                }
                _ => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid 32-byte hex key for testing.
    const TEST_KEY_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";

    fn make_client() -> RemoteSignerClient {
        RemoteSignerClient::new("http://localhost:9999", "test-key-id".into(), TEST_KEY_HEX)
            .unwrap()
    }

    // --- new() ---

    #[test]
    fn new_with_valid_key() {
        let client = RemoteSignerClient::new(
            "http://localhost:8080",
            "my-key".into(),
            TEST_KEY_HEX,
        );
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_0x_prefixed_key() {
        let client = RemoteSignerClient::new(
            "http://localhost:8080",
            "my-key".into(),
            &format!("0x{TEST_KEY_HEX}"),
        );
        assert!(client.is_ok());
    }

    #[test]
    fn new_strips_trailing_slash() {
        let client = RemoteSignerClient::new(
            "http://localhost:8080/",
            "my-key".into(),
            TEST_KEY_HEX,
        )
        .unwrap();
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn new_with_invalid_key_too_short() {
        let result = RemoteSignerClient::new(
            "http://localhost:8080",
            "my-key".into(),
            "0011",
        );
        assert!(result.is_err());
    }

    #[test]
    fn new_with_invalid_key_not_hex() {
        let result = RemoteSignerClient::new(
            "http://localhost:8080",
            "my-key".into(),
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        );
        assert!(result.is_err());
    }

    // --- timestamp_ms() ---

    #[test]
    fn timestamp_ms_returns_reasonable_value() {
        let client = make_client();
        let ts = client.test_timestamp_ms();
        // Should be after 2024-01-01 and before 2100-01-01 (in milliseconds)
        assert!(ts > 1_704_067_200_000, "timestamp too small: {ts}");
        assert!(ts < 4_102_444_800_000, "timestamp too large: {ts}");
    }

    #[test]
    fn timestamp_ms_is_monotonic() {
        let client = make_client();
        let t1 = client.test_timestamp_ms();
        let t2 = client.test_timestamp_ms();
        assert!(t2 >= t1, "timestamps should be monotonically non-decreasing");
    }
}
