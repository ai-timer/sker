# Implementation Plan: Sker Release Preview

**Branch**: `002-sker-release-preview` | **Date**: 2026-11-18 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-sker-release-preview/spec.md`

## Summary

Deliver an offline-first, auditable Rust workspace for compiling locked policy sources into immutable rule bundles, checking supported-language diffs, retrieving only relevant rule context, and independently enforcing PR results. Reuse the existing `sker-schema`, compiler, analyzer, retriever, MCP, CLI, and optional ledger boundaries; keep all cross-crate contracts in `sker-schema`. Local workflows use repository artifacts, and optional telemetry remains outside the PR gate.

## Technical Context

**Language/Version**: Rust edition 2024 (workspace manifests)

**Primary Dependencies**: Existing workspace declarations: ast-grep 0.51.3, gix 0.90.0 with explicit non-default features, rmcp 3.10.0 (`server`, `macros`, `transport-io`; add `schemars` for tool output schemas), tantivy 0.27.0, serde/serde_yaml_ng, tokio, clap, blake3, rkyv 0.8 with `bytecheck`, schemars, axum, and SQLx/PostgreSQL with rustls. No C-linked dependencies.

**Storage**: Local repository/config/bundle files for core workflows; optional governance ledger uses PostgreSQL through the existing SQLx dependency.

**Testing**: `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`; unit and boundary tests per crate, positive and negative fixtures per deterministic rule. Commands are documented in quickstart; they are not run during planning.

**Target Platform**: Blocking Preview v0.1 release/acceptance targets: macOS arm64/x86_64 and Linux x86_64/arm64. Windows remains pure-Rust/no-C-dependency source-compatible but is not a blocking Preview v0.1 target.

**Project Type**: Multi-crate Rust CLI/compiler/analyzer/retriever, local MCP server, and optional governance web service.

**Performance Goals**: Local deterministic check p95 ≤200 ms for up to 10,000 changed lines; resident guidance ≤300 tokens with a 500+ rule library; high-risk Hook warning delivered in milliseconds.

**Constraints**: Consult/check/CI run offline; no bundled vector model; no external runtime for core workflows; no C-linked dependencies; shared cross-crate schemas live in `sker-schema`; central `BLOCK` cannot be weakened by repository rules.

**Scale/Scope**: 20-person multilingual team, at least 500 rules, Go/Python/TypeScript/YAML-Infra, six user stories, PR and optional governance-event workflows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Status | Evidence |
|---|---|---|
| Git remains the authority; agents only propose changes | PASS | Rule changes require human review and merge; agent proposals remain pending. |
| Core workflows work offline | PASS | Consult, check, and CI use local bundles; ledger is optional and asynchronous. |
| Deterministic and semantic outcomes remain distinct | PASS | Spec requires track-labelled outcomes and semantic triggers only on declared signals. |
| Context cost is independent of rule count | PASS | Bounded managed guidance plus on-demand retrieval. |
| Crates communicate through `sker-schema` | PASS | Data model and contracts place shared structures in the schema crate. |
| Rust dependency constraints | PASS | Existing dependency declarations use gix, rustls, and ast-grep; no C-linked library is planned. |
| Quality gates | PASS | Plan includes per-crate unit/boundary testing and rule fixtures; merge commands are in quickstart. |
| Release target scope | PASS | Preview v0.1 blocking release/acceptance targets macOS arm64/x86_64 and Linux x86_64/arm64; Windows is source-level compatibility only. |
| Agent evidence trust boundary | PASS | Local Agent/CLI generates the pre-submit credential; CI independently recomputes the diff and validates it without generating or modifying credentials. |

### Post-design Constitution Check

| Principle | Status | Design evidence |
|---|---|---|
| Git is the source of truth | PASS | Proposal lifecycle ends in human review and normal Git merge; CI verifies local credentials and recomputes results. |
| Offline-first | PASS | Bundle, check, consult, and MCP contracts use local artifacts; ledger ingestion is optional and asynchronous. |
| Deterministic/semantic separation | PASS | Check-result schema carries the track and keeps semantic signals distinct from deterministic findings. |
| Bounded context | PASS | MCP contracts return selected rule context; managed guidance remains capped independently of library size. |
| Shared crate schema | PASS | `data-model.md` places cross-crate entities in `sker-schema`. |
| Rust/dependency constraints | PASS | No dependency exception is proposed; CI/ledger providers are adapters. |
| Quality gates | PASS | `quickstart.md` records workspace tests, clippy, boundary tests, and positive/negative rule fixtures for blocking release targets. |

No Constitution violations or dependency exceptions are identified.

## Project Structure

### Documentation (this feature)

```text
specs/002-sker-release-preview/
├── plan.md              # This file ($speckit-plan command output)
├── research.md          # Phase 0 output ($speckit-plan command)
├── data-model.md        # Phase 1 output ($speckit-plan command)
├── quickstart.md        # Phase 1 output ($speckit-plan command)
├── contracts/           # Phase 1 output ($speckit-plan command)
└── tasks.md             # Phase 2 output ($speckit-tasks command - NOT created by $speckit-plan)
```

### Source Code (repository root)
```text
blocking release/acceptance:
  macOS arm64, macOS x86_64
  Linux x86_64, Linux arm64
source compatibility (non-blocking in Preview v0.1):
  Windows x86_64, Windows arm64
crates/
├── sker-schema/       # Shared serialized types and schema validation
├── sker-compiler/     # Source locks, rule validation, bundle generation
├── sker-analyzer/     # Git diff, language analysis, baseline and SARIF
├── sker-retriever/    # Local lexical relevance and scoped rule retrieval
├── sker-mcp/          # MCP tools and host hook adapter
└── sker-cli/          # `sker` user-facing command-line binary
apps/
└── sker-ledger/       # Optional asynchronous governance service
specs/002-sker-release-preview/
├── contracts/         # CLI, MCP, CI/SARIF, hook, and telemetry contracts
├── data-model.md
├── quickstart.md
├── research.md
└── plan.md
tests/                 # Crate unit/boundary tests and rule fixtures (to be added)
```

**Structure Decision**: Extend the existing seven-member Rust workspace in place. Preserve crate responsibilities and put all exchanged domain types in `sker-schema`. Local Agent/CLI flows produce consult traces and finalized attestations before submission. CI only recomputes the PR diff and validates bundle/diff hashes, declarations, and the BLAKE3 credential; CI never issues, repairs, or replaces an Agent credential. Keep provider-specific CI and ledger deployment adapters at their integration boundaries; local core workflows must not import remote-service requirements.

