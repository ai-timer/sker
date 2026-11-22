# Research: Sker Release Preview

## Decisions

### Workspace and dependency boundary

- **Decision:** Implement within the existing Rust 2024 workspace: `sker-schema` owns shared contracts; compiler, analyzer, retriever, MCP, CLI, and ledger remain separate crates/apps as their manifests specify.
- **Rationale:** The repository already has these crate boundaries and dependencies. The Constitution requires cross-crate data to flow through `sker-schema` and forbids C-linked dependencies.
- **Alternatives considered:** A single CLI crate would simplify early wiring but would violate the established workspace boundary and make shared data contracts implicit.

### Rust library selections

- **`ast-grep` 0.51.3:** Use `ast-grep-core`, `ast-grep-language`, and `ast-grep-config` for syntax-aware matching and diff anchors in the supported language set. Keep syntax parsing in `sker-analyzer` and rule compilation validation in `sker-compiler`; do not expose library types outside `sker-schema`. The crate provides parsing, traversal, matching, and replacement over tree-sitter nodes. [Reference](https://docs.rs/ast-grep-core/0.51.3/ast_grep_core/).
- **`rmcp` 3.10.0:** Use the official Rust MCP SDK with `server`, `macros`, `transport-io`, and `schemars` capabilities. MCP tool names and input/output schemas are fixed in `contracts/mcp-tools.json`; schemas use JSON Schema Draft 2020-12 and object-shaped results for compatibility. The current workspace declaration has `server`, `macros`, and `transport-io`; the plan records `schemars` as the feature to add when tool output schemas are implemented. [Tagged 3.10.0 manifest](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.10.0/crates/rmcp/Cargo.toml).
- **`gix` 0.90.0:** Keep `default-features = false` and the current explicit SHA-1, revision, status, gix-status, blob-diff, index, dirwalk, and worktree-mutation feature set. Do not enable `worktree-stream`, `worktree-archive`, or the `extras` feature bundle. `cargo tree -e features -i gix-worktree-stream` currently reports no active dependency path, so the crate in Cargo.lock is not currently enabled. [0.90.0 feature manifest](https://docs.rs/crate/gix/0.90.0/source/Cargo.toml).
- **`tantivy` 0.27.0:** Use as the local lexical retrieval/index candidate in `sker-retriever`; expand synonym metadata before query, return a bounded ranked set, and keep model artifacts out of the package. This implements the behavior-level contract without making its internal ranking algorithm a user-facing requirement. [0.27.0 documentation](https://docs.rs/tantivy/0.27.0/tantivy/).
- **`rkyv` 0.8 with `bytecheck`:** Use for validated archived bundle serialization in `sker-schema`; validate archived bytes before access and retain the stable BLAKE3 content hash as a separate integrity identity. The workspace explicitly enables `bytecheck`. [Feature reference](https://docs.rs/crate/rkyv/0.8.18/features).

### Offline local workflows

- **Decision:** Rule compilation, local consult/check, and CI recomputation consume repository/local artifacts and complete without a remote service. The governance ledger is an optional separate service.
- **Rationale:** This is required by the Constitution and FR-012/FR-017. The repo already isolates the ledger app and declares PostgreSQL through SQLx with rustls.
- **Alternatives considered:** Requiring a hosted rule or telemetry service would make core workflows fail in disconnected or self-hosted environments.

### Rule retrieval behavior

- **Decision:** Preview v0.1 retrieval returns only selected context using lexical relevance and synonym metadata; it does not download or bundle a vector model.
- **Rationale:** This is the accepted spec clarification and keeps offline cold-start and distribution constraints predictable. `tantivy` 0.27.0 is already in the workspace dependency set and is a candidate local full-text engine, but the user-facing contract remains behavior-based.
- **Alternatives considered:** A bundled local embedding model increases package size and startup work; an online embedding service violates offline operation.
- **References:** [Tantivy crate documentation](https://docs.rs/tantivy/0.27.0/tantivy/).

### MCP and CI interchange

- **Decision:** Keep the existing `rmcp` server dependency and its standard-I/O transport for the local MCP process. Emit SARIF 2.1.0 for CI findings.
- **Rationale:** `rmcp` and `transport-io` are already declared; SARIF is explicitly required by the spec and 2.1.0 remains the stable OASIS interchange contract while 2.2 is a newer draft.
- **Alternatives considered:** A custom JSON-RPC tool protocol duplicates MCP; a custom CI findings format would not satisfy the spec's SARIF requirement.
- **References:** [rmcp transport documentation](https://docs.rs/rmcp/3.10.0/rmcp/transport/), [OASIS SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html).

### Source of truth and authority

- **Decision:** Compiled bundles are immutable snapshots. Local Agent/CLI flows accumulate consult trace before/during coding and generate the finalized Agent credential after the diff exists but before commit/PR submission. The credential carries consult trace, semantic verdicts, `bundle_sha256`, `diff_tree_hash`, and a BLAKE3 digest. CI independently recomputes the PR diff and validates credential integrity, binding, and declaration completeness; CI never creates, repairs, or replaces an Agent credential. Agent proposals remain pending until human review and Git merge.
- **Rationale:** A final digest cannot bind the resulting diff until that diff exists; local finalization before submission preserves the requested pre-submit producer/verifier split while allowing consult history to be captured before coding. CI independently validates without taking ownership of the agent's claim.
- **Alternatives considered:** Letting an agent activate its own proposal, or accepting self-reported Agent status without CI verification, conflicts with the Constitution and clarified PR gate.

### Release target matrix

- **Decision:** Preview v0.1 blocking release and acceptance targets are macOS arm64/x86_64 and Linux x86_64/arm64. Windows remains pure-Rust/no-C-dependency source compatibility, but is not a blocking release or acceptance target.
- **Rationale:** This is the accepted platform correction. It keeps release qualification bounded while retaining source-level portability.
- **Alternatives considered:** Windows release qualification in Preview v0.1 expands the release matrix and is explicitly deferred.

### Trusted Agent evidence

- **Decision:** The local Agent/CLI (`MCP`, `Hook`, or `sker attest`) is the sole producer of the Agent credential. CI only independently recomputes the PR diff and verifies the credential fields and BLAKE3 digest; it never generates, repairs, or replaces credentials.
- **Credential payload:** `bundle_sha256`, lowercase-hex BLAKE3 `diff_tree_hash`, ordered consult trace entries, per-rule semantic verdicts, schema version, and creation time. Consult trace entries contain query digests, selected rule IDs, paths, and timestamps rather than raw prompts. Semantic verdicts use the four spec states. The credential digest is BLAKE3 over RFC 8785 canonical JSON UTF-8 bytes, excluding the digest field itself. The diff tree digest uses path-sorted changed entries and length-prefixed status/path/mode/blob fields.
- **Rationale:** This preserves local authorship and keeps CI an independent verifier. CI compares its recomputed bundle/diff identities and required declarations against the submitted credential.
- **Alternatives considered:** CI-issued credentials conflate verifier and issuer, and cannot demonstrate which consultation/verdict history the local Agent had before submission.

## Assumptions to validate during implementation planning

- Release and blocking benchmark results are required only for macOS arm64/x86_64 and Linux x86_64/arm64; Windows is non-blocking source compatibility.
- The final local credential is produced after the candidate diff exists and before commit/PR submission; consult trace is captured before/during coding and included in that final credential.
- The current workspace's rmcp declaration must add its `schemars` feature when output-schema generation is wired; no HTTP transport is required for the local MCP server.
