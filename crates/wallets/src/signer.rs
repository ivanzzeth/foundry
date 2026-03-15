use crate::error::WalletSignerError;
use alloy_consensus::SignableTransaction;
use alloy_dyn_abi::TypedData;
use alloy_network::TxSigner;
use alloy_primitives::{Address, B256, ChainId, Signature, hex};
use alloy_signer::Signer;
use alloy_signer_ledger::{HDPath as LedgerHDPath, LedgerSigner};
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English};
use alloy_signer_trezor::{HDPath as TrezorHDPath, TrezorSigner};
use alloy_sol_types::{Eip712Domain, SolStruct};
use async_trait::async_trait;
use std::{collections::HashSet, path::PathBuf};
use tracing::warn;

#[cfg(feature = "aws-kms")]
use alloy_signer_aws::{AwsSigner, aws_config::BehaviorVersion, aws_sdk_kms::Client as AwsClient};

#[cfg(feature = "gcp-kms")]
use alloy_signer_gcp::{
    GcpKeyRingRef, GcpSigner, GcpSignerError, KeySpecifier,
    gcloud_sdk::{
        GoogleApi,
        google::cloud::kms::v1::key_management_service_client::KeyManagementServiceClient,
    },
};

#[cfg(feature = "turnkey")]
use alloy_signer_turnkey::TurnkeySigner;

#[cfg(feature = "signer-cobo")]
use foundry_cobo_mpc::CoboMpcSigner;

#[cfg(feature = "signer-remote")]
use foundry_remote_signer::RemoteHttpSigner;

pub type Result<T> = std::result::Result<T, WalletSignerError>;

/// Wrapper enum around different signers.
#[derive(Debug)]
pub enum WalletSigner {
    /// Wrapper around local wallet. e.g. private key, mnemonic
    Local(PrivateKeySigner),
    /// Wrapper around Ledger signer.
    Ledger(LedgerSigner),
    /// Wrapper around Trezor signer.
    Trezor(TrezorSigner),
    /// Wrapper around AWS KMS signer.
    #[cfg(feature = "aws-kms")]
    Aws(AwsSigner),
    /// Wrapper around Google Cloud KMS signer.
    #[cfg(feature = "gcp-kms")]
    Gcp(GcpSigner),
    /// Wrapper around Turnkey signer.
    #[cfg(feature = "turnkey")]
    Turnkey(TurnkeySigner),
    /// Wrapper around Cobo MPC signer.
    #[cfg(feature = "signer-cobo")]
    CoboMpc(CoboMpcSigner),
    /// Wrapper around Remote HTTP signer.
    #[cfg(feature = "signer-remote")]
    Remote(RemoteHttpSigner),
}

impl WalletSigner {
    pub async fn from_ledger_path(path: LedgerHDPath) -> Result<Self> {
        let ledger = LedgerSigner::new(path, None).await?;
        Ok(Self::Ledger(ledger))
    }

    pub async fn from_trezor_path(path: TrezorHDPath) -> Result<Self> {
        let trezor = TrezorSigner::new(path, None).await?;
        Ok(Self::Trezor(trezor))
    }

    pub async fn from_aws(key_id: String) -> Result<Self> {
        #[cfg(feature = "aws-kms")]
        {
            let config =
                alloy_signer_aws::aws_config::load_defaults(BehaviorVersion::latest()).await;
            let client = AwsClient::new(&config);

            Ok(Self::Aws(
                AwsSigner::new(client, key_id, None)
                    .await
                    .map_err(|e| WalletSignerError::Aws(Box::new(e)))?,
            ))
        }

        #[cfg(not(feature = "aws-kms"))]
        {
            let _ = key_id;
            Err(WalletSignerError::aws_unsupported())
        }
    }

    pub async fn from_gcp(
        project_id: String,
        location: String,
        keyring: String,
        key_name: String,
        key_version: u64,
    ) -> Result<Self> {
        #[cfg(feature = "gcp-kms")]
        {
            let keyring = GcpKeyRingRef::new(&project_id, &location, &keyring);
            let client = match GoogleApi::from_function(
                KeyManagementServiceClient::new,
                "https://cloudkms.googleapis.com",
                None,
            )
            .await
            {
                Ok(c) => c,
                Err(e) => {
                    return Err(WalletSignerError::Gcp(Box::new(GcpSignerError::GoogleKmsError(
                        e,
                    ))));
                }
            };

            let specifier = KeySpecifier::new(keyring, &key_name, key_version);

            Ok(Self::Gcp(
                GcpSigner::new(client, specifier, None)
                    .await
                    .map_err(|e| WalletSignerError::Gcp(Box::new(e)))?,
            ))
        }

        #[cfg(not(feature = "gcp-kms"))]
        {
            let _ = project_id;
            let _ = location;
            let _ = keyring;
            let _ = key_name;
            let _ = key_version;
            Err(WalletSignerError::gcp_unsupported())
        }
    }

    pub fn from_turnkey(
        api_private_key: String,
        organization_id: String,
        address: Address,
    ) -> Result<Self> {
        #[cfg(feature = "turnkey")]
        {
            Ok(Self::Turnkey(TurnkeySigner::from_api_key(
                &api_private_key,
                organization_id,
                address,
                None,
            )?))
        }

        #[cfg(not(feature = "turnkey"))]
        {
            let _ = api_private_key;
            let _ = organization_id;
            let _ = address;
            Err(WalletSignerError::turnkey_unsupported())
        }
    }

    pub fn from_cobo_mpc(
        api_key_hex: &str,
        wallet_id: String,
        address: Address,
        cobo_chain_id: String,
        env_str: &str,
    ) -> Result<Self> {
        #[cfg(feature = "signer-cobo")]
        {
            let env: foundry_cobo_mpc::CoboEnv = env_str
                .parse()
                .map_err(|e: eyre::Report| WalletSignerError::CoboMpc(e.to_string()))?;
            let signer = CoboMpcSigner::new(api_key_hex, wallet_id, address, cobo_chain_id, env)
                .map_err(|e| WalletSignerError::CoboMpc(e.to_string()))?;
            Ok(Self::CoboMpc(signer))
        }

        #[cfg(not(feature = "signer-cobo"))]
        {
            let _ = (api_key_hex, wallet_id, address, cobo_chain_id, env_str);
            Err(WalletSignerError::cobo_mpc_unsupported())
        }
    }

    /// TLS paths: (ca_file, cert_file, key_file) for mTLS.
    /// When `skip_verify` is true, server certificate verification is skipped (insecure).
    /// Exactly one of `api_key_hex` or `api_key_file` must be set.
    pub fn from_remote_signer(
        url: &str,
        api_key_id: String,
        api_key_hex: Option<&str>,
        api_key_file: Option<&str>,
        address: Address,
        tls_paths: Option<(String, String, String)>,
        skip_verify: bool,
    ) -> Result<Self> {
        #[cfg(feature = "signer-remote")]
        {
            let tls = tls_paths.map(|(ca, cert, key)| foundry_remote_signer::TlsConfig {
                ca_file: Some(ca),
                cert_file: Some(cert),
                key_file: Some(key),
                skip_verify,
            });
            let signer = RemoteHttpSigner::new(
                url,
                api_key_id,
                api_key_hex,
                api_key_file,
                address,
                tls,
            )
            .map_err(|e| WalletSignerError::RemoteSigner(e.to_string()))?;
            Ok(Self::Remote(signer))
        }

        #[cfg(not(feature = "signer-remote"))]
        {
            let _ = (url, api_key_id, api_key_hex, api_key_file, address, tls_paths, skip_verify);
            Err(WalletSignerError::remote_signer_unsupported())
        }
    }

    /// Returns true if this is a Cobo MPC signer.
    #[cfg(feature = "signer-cobo")]
    pub fn is_cobo_mpc(&self) -> bool {
        matches!(self, Self::CoboMpc(_))
    }

    /// Returns true if this is a Cobo MPC signer (always false when feature disabled).
    #[cfg(not(feature = "signer-cobo"))]
    pub fn is_cobo_mpc(&self) -> bool {
        false
    }

    /// Returns a reference to the Cobo MPC client, if this is a Cobo MPC signer.
    #[cfg(feature = "signer-cobo")]
    pub fn cobo_mpc_client(&self) -> Option<&foundry_cobo_mpc::CoboMpcClient> {
        match self {
            Self::CoboMpc(signer) => Some(signer.client()),
            _ => None,
        }
    }

    pub fn from_private_key(private_key: &B256) -> Result<Self> {
        Ok(Self::Local(PrivateKeySigner::from_bytes(private_key)?))
    }

    /// Returns a list of addresses available to use with current signer
    ///
    /// - for Ledger and Trezor signers the number of addresses to retrieve is specified as argument
    /// - the result for Ledger signers includes addresses available for both LedgerLive and Legacy
    ///   derivation paths
    /// - for Local and AWS signers the result contains a single address
    /// - errors when retrieving addresses are logged but do not prevent returning available
    ///   addresses
    pub async fn available_senders(&self, max: usize) -> Result<Vec<Address>> {
        let mut senders = HashSet::new();

        match self {
            Self::Local(local) => {
                senders.insert(local.address());
            }
            Self::Ledger(ledger) => {
                // Try LedgerLive derivation path
                for i in 0..max {
                    match ledger.get_address_with_path(&LedgerHDPath::LedgerLive(i)).await {
                        Ok(address) => {
                            senders.insert(address);
                        }
                        Err(e) => {
                            warn!("Failed to get Ledger address at index {i} (LedgerLive): {e}");
                        }
                    }
                }
                // Try Legacy derivation path
                for i in 0..max {
                    match ledger.get_address_with_path(&LedgerHDPath::Legacy(i)).await {
                        Ok(address) => {
                            senders.insert(address);
                        }
                        Err(e) => {
                            warn!("Failed to get Ledger address at index {i} (Legacy): {e}");
                        }
                    }
                }
            }
            Self::Trezor(trezor) => {
                for i in 0..max {
                    match trezor.get_address_with_path(&TrezorHDPath::TrezorLive(i)).await {
                        Ok(address) => {
                            senders.insert(address);
                        }
                        Err(e) => {
                            warn!("Failed to get Trezor address at index {i} (TrezorLive): {e}",);
                        }
                    }
                }
            }
            #[cfg(feature = "aws-kms")]
            Self::Aws(aws) => {
                senders.insert(alloy_signer::Signer::address(aws));
            }
            #[cfg(feature = "gcp-kms")]
            Self::Gcp(gcp) => {
                senders.insert(alloy_signer::Signer::address(gcp));
            }
            #[cfg(feature = "turnkey")]
            Self::Turnkey(turnkey) => {
                senders.insert(alloy_signer::Signer::address(turnkey));
            }
            #[cfg(feature = "signer-cobo")]
            Self::CoboMpc(cobo) => {
                senders.insert(alloy_signer::Signer::address(cobo));
            }
            #[cfg(feature = "signer-remote")]
            Self::Remote(remote) => {
                senders.insert(alloy_signer::Signer::address(remote));
            }
        }
        Ok(senders.into_iter().collect())
    }

    pub fn from_mnemonic(
        mnemonic: &str,
        passphrase: Option<&str>,
        derivation_path: Option<&str>,
        index: u32,
    ) -> Result<Self> {
        let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic);

        if let Some(passphrase) = passphrase {
            builder = builder.password(passphrase)
        }

        builder = if let Some(hd_path) = derivation_path {
            builder.derivation_path(hd_path)?
        } else {
            builder.index(index)?
        };

        Ok(Self::Local(builder.build()?))
    }
}

macro_rules! delegate {
    ($s:ident, $inner:ident => $e:expr) => {
        match $s {
            Self::Local($inner) => $e,
            Self::Ledger($inner) => $e,
            Self::Trezor($inner) => $e,
            #[cfg(feature = "aws-kms")]
            Self::Aws($inner) => $e,
            #[cfg(feature = "gcp-kms")]
            Self::Gcp($inner) => $e,
            #[cfg(feature = "turnkey")]
            Self::Turnkey($inner) => $e,
            #[cfg(feature = "signer-cobo")]
            Self::CoboMpc($inner) => $e,
            #[cfg(feature = "signer-remote")]
            Self::Remote($inner) => $e,
        }
    };
}

#[async_trait]
impl Signer for WalletSigner {
    /// Signs the given hash.
    async fn sign_hash(&self, hash: &B256) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => inner.sign_hash(hash)).await
    }

    async fn sign_message(&self, message: &[u8]) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => inner.sign_message(message)).await
    }

    fn address(&self) -> Address {
        delegate!(self, inner => alloy_signer::Signer::address(inner))
    }

    fn chain_id(&self) -> Option<ChainId> {
        delegate!(self, inner => inner.chain_id())
    }

    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        delegate!(self, inner => inner.set_chain_id(chain_id))
    }

    async fn sign_typed_data<T: SolStruct + Send + Sync>(
        &self,
        payload: &T,
        domain: &Eip712Domain,
    ) -> alloy_signer::Result<Signature>
    where
        Self: Sized,
    {
        delegate!(self, inner => inner.sign_typed_data(payload, domain)).await
    }

    async fn sign_dynamic_typed_data(
        &self,
        payload: &TypedData,
    ) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => inner.sign_dynamic_typed_data(payload)).await
    }
}

#[async_trait]
impl TxSigner<Signature> for WalletSigner {
    fn address(&self) -> Address {
        Signer::address(self)
    }

    async fn sign_transaction(
        &self,
        tx: &mut dyn SignableTransaction<Signature>,
    ) -> alloy_signer::Result<Signature> {
        delegate!(self, inner => TxSigner::sign_transaction(inner, tx)).await
    }
}

/// Signers that require user action to be obtained.
#[derive(Debug, Clone)]
pub enum PendingSigner {
    Keystore(PathBuf),
    Interactive,
}

impl PendingSigner {
    pub fn unlock(self) -> Result<WalletSigner> {
        match self {
            Self::Keystore(path) => {
                let password = rpassword::prompt_password("Enter keystore password:")?;
                match PrivateKeySigner::decrypt_keystore(path, password) {
                    Ok(signer) => Ok(WalletSigner::Local(signer)),
                    Err(e) => match e {
                        // Catch the `MacMismatch` error, which indicates an incorrect password and
                        // return a more user-friendly `IncorrectKeystorePassword`.
                        alloy_signer_local::LocalSignerError::EthKeystoreError(
                            eth_keystore::KeystoreError::MacMismatch,
                        ) => Err(WalletSignerError::IncorrectKeystorePassword),
                        _ => Err(WalletSignerError::Local(e)),
                    },
                }
            }
            Self::Interactive => {
                let private_key = rpassword::prompt_password("Enter private key:")?;
                Ok(WalletSigner::from_private_key(&hex::FromHex::from_hex(private_key)?)?)
            }
        }
    }
}
