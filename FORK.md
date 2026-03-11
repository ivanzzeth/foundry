# Fork Documentation

This is a fork of [foundry-rs/foundry](https://github.com/foundry-rs/foundry)
with custom extensions for Cobo MPC, remote-signer, and batch operations.

## Custom Features

| Feature Flag | Crate | Description |
|---|---|---|
| `signer-cobo` | `crates/cobo-mpc/` | Cobo MPC wallet signer + provider |
| `signer-remote` | `crates/remote-signer/` | Remote HTTP signer with ACL |
| `batch` | `crates/batch-ops/` | Distribute/collect batch commands |
| `extra` | (umbrella) | Enables all three above |

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
- `castup/` — installer scripts (castup + install)
- `.github/workflows/fork-release.yml` — release CI
- `FORK.md` (this file)

## Syncing with Upstream

```bash
./scripts/sync-upstream.sh
```

If conflicts occur, they will be in the "Modified Files" listed above.
All custom code uses `#[cfg(feature = "...")]` guards to minimize diff.

## Install

### One-line installer (recommended)

```bash
curl -sSf https://raw.githubusercontent.com/ivanzzeth/foundry/HEAD/castup/install | bash
```

Then run `castup` to install the latest pre-built release:

```bash
castup                          # Install latest release
castup -i v0.1.0           # Install specific version
castup --source                 # Build from source (master)
castup -b feat/my-branch        # Build from specific branch
castup -p ~/code/foundry        # Build from local repo
castup -l                       # List installed versions
castup -u v0.1.0           # Switch to installed version
```

### From source (cargo)

```bash
cargo install --path ./crates/cast --features extra
```

Or from git:

```bash
cargo install --git https://github.com/ivanzzeth/foundry \
  --features extra \
  --bin cast
```

## CI/CD

Release workflow: `.github/workflows/fork-release.yml`

- **Trigger**: Push tags `v*.*.*` or manual dispatch
- **Platforms**: Linux (x86_64, aarch64), macOS (x86_64, aarch64)
- **Features**: All upstream features + `extra` (signer-cobo, signer-remote, batch)
- **Artifacts**: `foundry_{tag}_{platform}_{arch}.tar.gz` on GitHub Releases
