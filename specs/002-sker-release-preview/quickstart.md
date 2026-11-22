# Quickstart Validation Guide

This guide validates the Preview v0.1 workflows after implementation. It is a run guide; it does not claim the current skeleton already implements these commands.

## Prerequisites

- Rust toolchain compatible with the workspace's edition 2024 manifests.
- A Git repository containing one supported language or YAML/Infra, plus a pinned rule bundle and a fixture for deterministic findings.
- No central service is required for local compile, consult, check, or CI recomputation.

## Build and local setup

From the repository root:

```sh
cargo build --workspace
cargo run -p sker -- init
cargo run -p sker -- doctor
```

Expected: the workspace builds, `init` reports detected languages/hosts without duplicating managed blocks on a second run, and `doctor` reports each bundle/guidance/hook/MCP check with actionable failure details.

## Compile and inspect a local diff

```sh
cargo run -p sker -- compile
cargo run -p sker -- check --format sarif
```

Expected: compile writes a read-only bundle and stable hash; an override cycle or local downgrade of a central `BLOCK` is rejected without replacing the last valid bundle. `check` considers only added/modified diff lines, distinguishes deterministic findings from semantic triggers, and reports a SARIF location. A baseline item follows its syntax/symbol and normalized problem anchor across line movement; formatting/comment-only changes preserve the match.

## Consult and attest offline

With network access disabled:

```sh
cargo run -p sker -- mcp
```

Connect an MCP client over standard input/output and invoke `consult`, `search`, and `attest` using the request fields in [contracts/README.md](contracts/README.md). Expected: retrieval uses local lexical relevance and synonym metadata, a low-relevance query returns an explicit empty result, and attestation verification fails if either the bundle or diff hash changes.

## Local credential and CI verification

Before committing the candidate change, let the local Agent/CLI collect consult trace before/during coding and finalize the credential:

```sh
cargo run -p sker -- attest
```

Expected: local `sker attest` emits a credential containing consult trace, semantic verdicts, `bundle_sha256`, `diff_tree_hash`, and the BLAKE3 credential digest. CI must independently recompute the PR diff and validate the submitted credential; it must never create, repair, or replace one. Run the CI adapter against fixtures for valid and invalid credentials, a missing triggered semantic verdict, and a human PR with a high-risk semantic trigger. Invalid Agent evidence blocks; a human PR does not require Agent credentials and receives a self-check card. Disable the optional ledger and repeat; the gate result must remain identical.

## Quality gates

Before merge, run the Constitution's workspace gates:

```sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
action/scripts/test-action.sh
```

Expected: unit and boundary tests pass for every crate; each deterministic rule has passing positive and negative fixtures. Blocking build and fast-check p95 acceptance targets are macOS arm64/x86_64 and Linux x86_64/arm64. Windows remains a pure-Rust/no-C-dependency source-compatibility target and is non-blocking for Preview v0.1.

## Release Distribution Matrix and Platform Compatibility

Sker Release Preview v0.1 provides zero-C-dependency static binaries configured via `dist-workspace.toml` and GitHub Actions release pipelines (`.github/workflows/release.yml`).

### Supported Target Platforms

| Platform Target | Tier / Status | Distribution Artifacts | Verification Command |
|---|---|---|---|
| `aarch64-apple-darwin` | Tier 1 (Blocking) | `.tar.xz`, Shell installer, Homebrew Tap | `cargo check --target aarch64-apple-darwin` / `dist plan` |
| `x86_64-apple-darwin` | Tier 1 (Blocking) | `.tar.xz`, Shell installer, Homebrew Tap | `cargo check --target x86_64-apple-darwin` / `dist plan` |
| `x86_64-unknown-linux-gnu` | Tier 1 (Blocking) | `.tar.xz`, Shell installer, Homebrew Tap | `cargo check --target x86_64-unknown-linux-gnu` / `dist plan` |
| `aarch64-unknown-linux-gnu` | Tier 1 (Blocking) | `.tar.xz`, Shell installer, Homebrew Tap | `cargo check --target aarch64-unknown-linux-gnu` / `dist plan` |
| `x86_64-pc-windows-msvc` | Tier 2 (Non-blocking) | Pure-Rust source compatible | `cargo check --workspace` |

### Non-blocking Windows Source-Compatibility Check

Windows is supported as a pure-Rust source-compatible target. Because Sker rejects C-linked dependencies (`openssl-sys`, `libgit2`, etc.) in accordance with Constitution Principle V and Rust Constraints, Windows builds do not require external C compilers, vcpkg, or OpenSSL toolchains:

```sh
# Verify pure-Rust workspace compilation without C-linkage
cargo check --workspace
# Confirm absence of C-linked OpenSSL
cargo tree -i openssl-sys  # Must report empty / no matches
```

### Installation Channels

- **Shell Installer**:
  ```sh
  curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ai-timer/sker/releases/latest/download/sker-installer.sh | sh
  ```
- **Homebrew Tap**:
  ```sh
  brew install ai-timer/tap/sker
  ```
