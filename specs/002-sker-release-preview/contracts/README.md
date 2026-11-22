# External Contracts

This document defines provider-neutral observable contracts. Shared Rust structures are owned by `sker-schema`; crate boundaries do not invent local cross-crate payloads. CLI names below follow the spec's user-visible workflows; exact flags remain implementation tasks.

Frozen contract files:

- [crate-traits.rs](crate-traits.rs): cross-crate trait signatures referencing the shared error enums defined and re-exported by `sker-schema`.
- [cli-ci.json](cli-ci.json): frozen CI invocation, exit-code semantics, atomic output rule, and CheckReport JSON Schema.
- [entry-schema.json](entry-schema.json): complete v1 Entry schema for YAML rule entries.
- [mcp-tools.json](mcp-tools.json): `sker_consult`, `sker_search`, `sker_attest`, and `sker_propose` input/output schemas. Its `sker_propose.entry` reference resolves to the co-shipped `entry-schema.json`; implementations use this bundled schema and do not fetch it over the network.
- [ledger-openapi.yaml](ledger-openapi.yaml): governance event ingest and read-only metrics/health API.

## CLI

The CI check invocation and machine-readable result are frozen in [cli-ci.json](cli-ci.json). Lane F MUST implement against that contract and its fixtures without depending on the Lane E binary; Phase 4 performs binary integration. The CI invocation is `sker check --ci --sarif <path> --json <path>`; exit code 0 means complete/no deterministic BLOCK, 1 means complete/blocked, and 2 means incomplete or invocation/output failure.

The `sker` executable exposes these operations:

| Command | Input | Observable result |
|---|---|---|
| `init` | Repository root and detected host/languages | Idempotent project configuration and managed guidance blocks |
| `doctor` | Repository root | Per-check pass/fail status and actionable repair guidance |
| `compile` | Locked central/business sources and local rules | Immutable bundle plus stable hash, or diagnostics without replacing the last good bundle |
| `check` | Repository root and Git diff; CI mode uses `--ci --sarif <path> --json <path>` | Deterministic findings/blocking status, semantic-trigger status, baseline handling, SARIF-capable locations, and CheckReport JSON |
| `replay` | Repository history window and bundle version | Per-rule historical rates and candidate false-positive samples; no gate or history mutation |
| `mcp` | Local bundle and standard input/output | Starts local MCP service without a central-service dependency |

Errors must be machine distinguishable from a clean/no-hit result. Empty diff, unsupported content, incomplete history, unavailable retrieval, and valid empty results are not conflated.

## MCP tools

The local MCP server exposes `sker_consult`, `sker_search`, `sker_attest`, and `sker_propose` over the declared standard-I/O transport. The exact JSON input and output schemas are frozen in [mcp-tools.json](mcp-tools.json) and use the schema dialect supported by rmcp 3.10.0.

| Tool | Request fields | Response fields / behavior |
|---|---|---|
| `sker_consult` | `intent`, `paths[]`, optional result limit | Ranked matching rules and scopes; below-threshold result is explicitly empty. Preview v0.1 uses local lexical relevance plus synonym metadata and runs offline. |
| `sker_search` | `query`, optional filters | Locatable rule IDs, source/version, scope, and query status; empty result is explicit. |
| `sker_attest` | `bundle_sha256`, `diff_tree_hash`, local consult trace, semantic verdicts | Locally creates a credential with a BLAKE3 digest after the candidate diff exists and before commit/PR submission. |
| `sker_propose` | Candidate rule or patch, rationale | A `pending` proposal identifier; never an active rule or approval. |

All tool results identify status and relevant bundle/version. Tool failure is reported as failure, not as a successful empty result. Consult/search return only selected context, not the entire rule library.

## Hook event

The host adapter consumes a pre-tool-use event containing the target tool/action and paths. It returns a delivery status and, when applicable, the high-risk domain warning. A timeout or unavailable host channel must be visible as undelivered; it must not be represented as delivered.

## CI result and Agent gate

CI independently recomputes results from the PR diff and repository bundle. It emits SARIF 2.1.0 with file URI and line/column region for findings. Result properties retain rule ID, deterministic/semantic track, and baseline/exemption disposition.

Credential ownership and PR verification are:

1. Local Agent/CLI flows collect consult trace before/during coding and `sker_attest` finalizes the credential from the candidate diff before commit/PR submission.
2. The credential payload includes the consult trace, per-rule semantic verdicts, `bundle_sha256`, and the lowercase-hex BLAKE3 `diff_tree_hash`. The final `credential_blake3` is BLAKE3 over RFC 8785 canonical JSON UTF-8 bytes, excluding the digest field itself. The canonical diff encoding is path-sorted and length-prefixes changed path, status, old/new mode, and old/new blob bytes.
3. CI independently recomputes the PR diff and validates credential digest, bundle/diff binding, and triggered semantic declarations. CI MUST NOT issue, repair, or replace an Agent credential.
4. Agent evidence present (credential and/or protected Agent marker): invalid/missing required credential or declaration blocks. With no Agent evidence, treat the PR as human for credential/form requirements and publish a high-risk semantic self-check card when applicable.

The CI provider adapter owns diff retrieval and comment/check-run rendering. Provider-specific authentication and protected-marker configuration are deployment bindings, not core offline requirements. CI remains the verifier and gate; it is never the credential issuer.

## Optional governance ledger

An asynchronous event carries `event_id`, timestamp, event type, rule and bundle identifiers, check track/state, and aggregate-safe counters. It MUST NOT contain source code, prompts, secrets, or personal identity. The service acknowledges accepted or duplicate event IDs; outage/timeout does not affect local results or PR gates. Reporting includes metric window, numerator, denominator, sample count, and freshness. Zero denominator is represented as `no_data`.

## SARIF reference

Use the OASIS SARIF 2.1.0 JSON object model for interchange, with Sker-specific rule, track, and disposition details in result properties. The format does not redefine gate policy.
