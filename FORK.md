# Fork Documentation

This is a fork of [foundry-rs/foundry](https://github.com/foundry-rs/foundry)
with custom extensions for Cobo MPC, remote-signer, and batch operations.

## Custom Features

| Feature Flag | Crate | Description |
|---|---|---|
| `cobo-mpc` | `crates/cobo-mpc/` | Cobo MPC wallet signer + provider |
| `remote-signer` | `crates/remote-signer/` | Remote HTTP signer with ACL |
| `batch-ops` | `crates/batch-ops/` | Distribute/collect batch commands |

## Modified Files (vs upstream)

These files have custom modifications and may conflict during sync:
- `Cargo.toml` (root) — workspace members + features
- `crates/wallets/Cargo.toml` — optional deps
- `crates/wallets/src/signer.rs` — new WalletSigner variants
- `crates/wallets/src/opts.rs` — new CLI flags
- `crates/wallets/src/error.rs` — new error variants
- `crates/cast/Cargo.toml` — feature propagation
- `crates/cast/src/args.rs` — new subcommand match arms
- `crates/cast/src/opts.rs` — new subcommand definitions
- `crates/cast/src/cmd/mod.rs` — new modules

## New Files (zero conflict with upstream)
- `crates/cobo-mpc/` — entire crate
- `crates/remote-signer/` — entire crate
- `crates/batch-ops/` — entire crate
- `crates/cast/src/cmd/distribute.rs`
- `crates/cast/src/cmd/collect.rs`
- `scripts/sync-upstream.sh`
- `FORK.md` (this file)

## Syncing with Upstream

```bash
./scripts/sync-upstream.sh
```

If conflicts occur, they will be in the "Modified Files" listed above.
All custom code uses `#[cfg(feature = "...")]` guards to minimize diff.

## Install

```bash
cargo install --path ./crates/cast --features cobo-mpc,remote-signer,batch-ops
```

Or from git:

```bash
cargo install --git https://github.com/ivanzzeth/foundry \
  --features cobo-mpc,remote-signer,batch-ops \
  --bin cast
```
