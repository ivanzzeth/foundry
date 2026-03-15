//! Unlock a remote-signer keystore/HD signer via API.
//!
//! Env: REMOTE_SIGNER_URL, REMOTE_SIGNER_API_KEY_ID, one of REMOTE_SIGNER_API_KEY_HEX or REMOTE_SIGNER_API_KEY_FILE,
//!      REMOTE_SIGNER_ADDRESS, REMOTE_SIGNER_PASSWORD.
//! For HTTPS/mTLS: REMOTE_SIGNER_CA_FILE, REMOTE_SIGNER_CLIENT_CERT_FILE, REMOTE_SIGNER_CLIENT_KEY_FILE.
//! Optional: REMOTE_SIGNER_TLS_INSECURE_SKIP_VERIFY=1 to skip server cert verification (testing only).

use remote_signer_client::evm::UnlockSignerRequest;
use remote_signer_client::{Config, TlsConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("REMOTE_SIGNER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8548".to_string());
    let api_key_id = std::env::var("REMOTE_SIGNER_API_KEY_ID")?;
    let address = std::env::var("REMOTE_SIGNER_ADDRESS")?;
    let password = std::env::var("REMOTE_SIGNER_PASSWORD")?;

    let mut cfg = Config::default();
    cfg.base_url = url.trim_end_matches('/').to_string();
    cfg.api_key_id = api_key_id;
    if let Ok(hex) = std::env::var("REMOTE_SIGNER_API_KEY_HEX") {
        cfg.private_key_hex = Some(hex);
    } else if let Ok(path) = std::env::var("REMOTE_SIGNER_API_KEY_FILE") {
        cfg.private_key_file = Some(path);
    } else {
        return Err("REMOTE_SIGNER_API_KEY_HEX or REMOTE_SIGNER_API_KEY_FILE is required".into());
    }

    let skip_verify = std::env::var("REMOTE_SIGNER_TLS_INSECURE_SKIP_VERIFY")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if let (Ok(ca), Ok(cert), Ok(key)) = (
        std::env::var("REMOTE_SIGNER_CA_FILE"),
        std::env::var("REMOTE_SIGNER_CLIENT_CERT_FILE"),
        std::env::var("REMOTE_SIGNER_CLIENT_KEY_FILE"),
    ) {
        cfg.tls = Some(TlsConfig {
            ca_file: Some(ca),
            cert_file: Some(cert),
            key_file: Some(key),
            skip_verify,
        });
    }

    let client = remote_signer_client::Client::new(cfg)?;
    let req = UnlockSignerRequest { password };
    let resp = client.evm.signers.unlock(&address, &req)?;
    println!("Unlocked signer: {} (type: {})", resp.address, resp.signer_type);
    Ok(())
}
