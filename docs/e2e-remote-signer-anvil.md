# Manual E2E: cast + remote-signer (Anvil / Polygon fork)

This describes a **manual** end-to-end test: remote-signer (already running) + cast using the remote signer to perform an ERC20 transfer. **Recommended:** Polygon fork with **USDC.e**; alternative: plain Anvil with any ERC20.

**Constraints:**

- **No changes** to remote-signer repo (no config, no code). Use the service as-is.
- **Only API** is used to add/unlock a test signer (create signer + unlock).
- All edits and builds are in the **foundry** repo only.

---

## 0. Recommended: Polygon fork + USDC.e

- **RPC:** Anvil forking Polygon, e.g. `anvil --fork-url https://polygon-rpc.com` (chain id **137**).
- **Token:** **USDC.e** on Polygon: `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`.
- Flow: start fork → create/unlock signer via API → give signer MATIC (gas) and USDC.e on the fork (e.g. impersonate whale) → run `cast send` for USDC.e `transfer(...)` with remote signer.

Sections 1–7 below apply; where the doc says “Anvil” with chain id 31337, use **Polygon fork** (chain 137) and the USDC.e address above. Step “Start Anvil” becomes “Start Anvil with Polygon fork” (see §3).

---

## 1. Prerequisites

- **remote-signer** already running (you start it yourself; no config changes).
- **Anvil** available (e.g. from this repo’s `forge`/foundry install).
- Foundry and remote-signer repos side-by-side so the path dep resolves:
  - `remote-signer-client` in `crates/remote-signer/Cargo.toml` uses  
    `path = "../../../remote-signer/pkg/rs-client"`  
  - So layout must be: `some_parent/{foundry,remote-signer}`.

You need:

- remote-signer **base URL** (e.g. `https://127.0.0.1:8548` when TLS is enabled)
- An **API key** for remote-signer: **id** + either **Ed25519 private key (hex)** via `REMOTE_SIGNER_API_KEY` or **path to PEM file** via `REMOTE_SIGNER_API_KEY_FILE` (e.g. `data/admin_private.pem`)
- (Optional) RPC URL and chain id: default `http://127.0.0.1:8545`; chain id `137` for Polygon fork, `31337` for plain Anvil

**TLS/mTLS:** If remote-signer uses HTTPS and client auth (mTLS), set:

- `REMOTE_SIGNER_CA_FILE` — path to CA cert (e.g. `remote-signer/certs/ca.crt`) to verify the server
- `REMOTE_SIGNER_CLIENT_CERT_FILE` — path to client cert (e.g. `remote-signer/certs/client.crt`)
- `REMOTE_SIGNER_CLIENT_KEY_FILE` — path to client key (e.g. `remote-signer/certs/client.key`)

Optional: `REMOTE_SIGNER_TLS_INSECURE_SKIP_VERIFY` set to `true` or `1` to skip server certificate verification (insecure; testing only). Default is to verify.

Certs under `remote-signer/certs/` (e.g. `ca.crt`, `client.crt`, `client.key`) match the server’s TLS config.

**Note:** If remote-signer has ERC20/transfer rules with budget metering, ensure the signer or rule has a budget (or a rule allows the test transfer). Otherwise the sign request may be rejected with a “blocked by rule” / “no budget record” error even though the cast ↔ remote-signer integration is working.

---

## 2. Add test signer via API (no config edits)

Use the running remote-signer **only via API**. No changes to its config or code.

### Option A: Create keystore (new key)

1. Create a signer (returns a new address):

   ```bash
   curl -s -X POST "${REMOTE_SIGNER_URL}/api/v1/evm/signers" \
     -H "Content-Type: application/json" \
     -H "X-API-Key-ID: YOUR_API_KEY_ID" \
     -H "X-Timestamp: $(date +%s)000" \
     -H "X-Nonce: $(openssl rand -hex 16)" \
     -H "X-Signature: YOUR_ED25519_SIGNATURE_FOR_THIS_REQUEST" \
     -d '{"type":"keystore","keystore":{"password":"test-password"}}'
   ```

   Authentication uses Ed25519: the server expects headers `X-API-Key-ID`, `X-Timestamp`, `X-Nonce`, and `X-Signature` (signature of `timestamp|nonce|METHOD|path|sha256(body)`). For manual testing you can use the **remote-signer Go client** or **rs-client** (e.g. a small script) to create/unlock signers instead of raw curl, or generate the signature for each request.

2. From the response, note **address**.

3. Unlock the signer:

   ```bash
   curl -s -X POST "${REMOTE_SIGNER_URL}/api/v1/evm/signers/ADDRESS/unlock" \
     -H "Content-Type: application/json" \
     -H "X-API-Key-ID: YOUR_API_KEY_ID" \
     ... \
     -d '{"password":"test-password"}'
   ```

4. Fund this address on Anvil (e.g. send ETH from an Anvil default account) so it can pay gas.

### Option B: HD wallet import (e.g. Anvil default address)

If your remote-signer supports HD wallet import and you want to use Anvil’s default account (e.g. `0xf39Fd...`):

- Use the remote-signer API to **import** an HD wallet (e.g. with Anvil’s well-known mnemonic) and **derive** the first address.
- Then call **unlock** for that signer if required.

Exact request bodies and paths depend on your remote-signer version; see its API docs (e.g. `POST /api/v1/evm/hd-wallets` for import, then derive).

---

## 3. Start Anvil (or Polygon fork)

**Option A – Polygon fork (recommended for USDC.e):**

```bash
anvil --fork-url https://polygon-rpc.com
```

Leave it running. RPC: `http://127.0.0.1:8545`, **chain id 137**. Use USDC.e: `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`.

**Option B – local chain only:**

```bash
anvil
```

Default RPC: `http://127.0.0.1:8545`, chain id `31337`. Use any ERC20 you deploy or have.

---

## 4. Build cast with remote signer

From the **foundry** repo root:

```bash
cargo build -p cast --features signer-remote --release
```

Binary: `target/release/cast`.

---

## 5. Gas and token for the signer (Polygon fork: MATIC + USDC.e)

- **Polygon fork:** Signer needs **MATIC** for gas and **USDC.e** to transfer. On the fork you can use `cast rpc anvil_impersonateAccount <WHALE>` then send MATIC and USDC.e from the whale to the signer address, then `anvil_stopImpersonatingAccount`.
- **Plain Anvil:** Deploy an ERC20 or use an existing one; ensure the **signer address** has enough ETH for gas (and optionally that ERC20).

---

## 6. Run ERC20 transfer with cast + remote signer

Use the signer address from step 2 and your API key (id + Ed25519 private key hex). Example: **Polygon fork + USDC.e** (HTTPS + mTLS):

```bash
export REMOTE_SIGNER_URL="https://127.0.0.1:8548"
export REMOTE_SIGNER_CA_FILE="projects/personal/ivanzzeth/remote-signer/certs/ca.crt"
export REMOTE_SIGNER_CLIENT_CERT_FILE="projects/personal/ivanzzeth/remote-signer/certs/client.crt"
export REMOTE_SIGNER_CLIENT_KEY_FILE="projects/personal/ivanzzeth/remote-signer/certs/client.key"
export REMOTE_SIGNER_TLS_INSECURE_SKIP_VERIFY="false"
# API key: either hex or PEM file path (set one)
# export REMOTE_SIGNER_API_KEY="your-ed25519-private-key-hex"
# export REMOTE_SIGNER_API_KEY_FILE="projects/personal/ivanzzeth/remote-signer/data/admin_private.pem"

# USDC.e on Polygon
USDC_E=0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174

cast send "$USDC_E" \
  "transfer(address,uint256)" RECIPIENT_ADDRESS AMOUNT_IN_6_DECIMALS \
  --rpc-url http://127.0.0.1:8545 \
  --chain 137 \
  --remote-signer-url "${REMOTE_SIGNER_URL}" \
  --remote-signer-api-key-id "${REMOTE_SIGNER_API_KEY_ID}" \
  --remote-signer-api-key "${REMOTE_SIGNER_API_KEY}" \
  --remote-signer-address "${REMOTE_SIGNER_ADDRESS}" \
  --remote-signer-ca-file "${REMOTE_SIGNER_CA_FILE}" \
  --remote-signer-client-cert-file "${REMOTE_SIGNER_CLIENT_CERT_FILE}" \
  --remote-signer-client-key-file "${REMOTE_SIGNER_CLIENT_KEY_FILE}"
```

Or use env vars only (no flags): `REMOTE_SIGNER_CA_FILE`, `REMOTE_SIGNER_CLIENT_CERT_FILE`, `REMOTE_SIGNER_CLIENT_KEY_FILE`. Optional: `REMOTE_SIGNER_TLS_INSECURE_SKIP_VERIFY` for testing. For API key you can set `REMOTE_SIGNER_API_KEY` (hex) or `REMOTE_SIGNER_API_KEY_FILE` (path to PEM); set exactly one.

- `REMOTE_SIGNER_URL`: e.g. `https://127.0.0.1:8548` when TLS is on
- `REMOTE_SIGNER_API_KEY_ID`: same as in the API calls above
- `REMOTE_SIGNER_API_KEY` or `REMOTE_SIGNER_API_KEY_FILE`: Ed25519 private key (hex string) or path to PEM file (e.g. `projects/personal/ivanzzeth/remote-signer/data/admin_private.pem`) for request signing; set exactly one
- `REMOTE_SIGNER_ADDRESS`: the address returned when creating/unlocking the signer (step 2)

Cast will send the sign request to remote-signer, get the signature, then broadcast the transaction to the fork. On a Polygon fork, chain id is 137 and no mainnet funds are spent.

---

## 7. Verify

- Check logs or run `cast receipt <TX_HASH> --rpc-url http://127.0.0.1:8545`.
- Optionally query USDC.e balance: `cast call "$USDC_E" "balanceOf(address)(uint256)" SIGNER_OR_RECIPIENT --rpc-url http://127.0.0.1:8545` (USDC.e uses 6 decimals).

---

## Status

**Verified:** This flow has been run end-to-end: cast (built with `--features signer-remote`) successfully sent an ERC20 transfer (USDC.e on Polygon fork) using remote-signer for transaction signing. No changes were made to the remote-signer repo; only the foundry fork (cast + `foundry-remote-signer` crate using `remote-signer-client`) was used.

---

## Dependency layout (path to rs-client)

`foundry-remote-signer` depends on `remote-signer-client` via:

```toml
remote-signer-client = { path = "../../../remote-signer/pkg/rs-client" }
```

So from the **foundry** repo root, `../remote-signer` must be the **remote-signer** repo (sibling directory). If your layout is different, change the path in `crates/remote-signer/Cargo.toml` accordingly (or later use a published crate version).
