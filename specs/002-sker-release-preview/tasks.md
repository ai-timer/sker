# Tasks: Sker Release Preview (v0.1.0-preview)

**Input**: `specs/002-sker-release-preview/spec.md`, `plan.md`, `data-model.md`, `contracts/`

**Execution topology**: Phase 1 is serial and blocks all implementation lanes. In Phase 2, lanes A–D are parallel work units with disjoint write roots. In Phase 3, lanes E–F are parallel work units with disjoint write roots. A task marked `[P]` may run concurrently with other `[P]` tasks in its wave; tasks within one lane follow their listed dependency order. Every task names its owned paths and an acceptance command.

**Credential authority**: The local Agent/CLI/MCP/Hook/`sker attest` produces the final credential after the candidate diff exists and before commit/submission. CI never creates, repairs, refreshes, or replaces it; CI independently recomputes the PR diff and verifies the submitted credential and declarations.

**Release targets**: Blocking Preview v0.1 acceptance is macOS arm64/x86_64 and Linux x86_64/arm64. Windows remains pure Rust/no-C-dependency source-compatible, outside blocking acceptance.

## Phase 1: Foundational — serial schema and sample rules

**Purpose**: Establish the shared versioned types and representative fixtures before crate lanes begin. Complete all tasks in order.

- [x] T001 [US1] Implement versioned shared source-lock, rule, override, bundle, and archived-bundle types with validation and `rkyv` byte checking in `crates/sker-schema/src/bundle.rs` and `crates/sker-schema/src/rule.rs`; acceptance: `cargo test -p sker-schema`
- [x] T002 [US1] Implement baseline identity types in `crates/sker-schema/src/analysis.rs` using repository-relative path + rule ID + syntax node/symbol + normalized problem fragment, with line number as display hint only; acceptance: `cargo test -p sker-schema`
- [x] T003 [US1] Implement diff, finding, deterministic/semantic result, SARIF input, replay result types, and the path-sorted, length-prefixed BLAKE3 diff-tree hash in `crates/sker-schema/src/analysis.rs`; acceptance: `cargo test -p sker-schema`
- [x] T004 [US3] Implement consult trace, four-state semantic verdict, proposal, and RFC 8785/BLAKE3 credential payload types in `crates/sker-schema/src/credential.rs`; acceptance: `cargo test -p sker-schema`
- [x] T005 [US6] Implement sanitized governance-event, seven-metric, health-notice, and lifecycle types in `crates/sker-schema/src/ledger.rs`; acceptance: `cargo test -p sker-schema`
- [x] T006 [US1] Implement and re-export from `crates/sker-schema/src/lib.rs` every request/response type (`CompileRequest`, `AnalysisRequest`, `CheckReport`, `ConsultQuery`, `ConsultResponse`, `SearchQuery`, `SearchResponse`), shared Trait (`BundleCompiler`, `DiffAnalyzer`, `IntentRetriever`), and error enum (`CompileError`, `AnalysisError`, `RetrievalError`) referenced by `specs/002-sker-release-preview/contracts/crate-traits.rs`; acceptance: `cargo test -p sker-schema && cargo check --workspace`
- [x] T007 [US1] Add schema unit, boundary, and public-contract compile/export tests for `specs/002-sker-release-preview/contracts/entry-schema.json`, `specs/002-sker-release-preview/contracts/crate-traits.rs`, serialization round trips, invalid enums/digests, central `BLOCK` invariants, and archived-byte validation in `crates/sker-schema/tests/schema_contracts.rs` and `crates/sker-schema/tests/contract_exports.rs`; acceptance: `cargo test -p sker-schema && cargo check --workspace`
- [x] T008 [US1] Add the first deterministic Go rule and positive/negative code fixtures in `examples/sample-playbook/rules/go-no-logging.yaml`, `examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go`, and `examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go`; acceptance: `cargo test -p sker-schema --test sample_playbook`
- [x] T009 [US1] Add the Go ast-grep detector and sample Playbook source/lock in `examples/sample-playbook/rules/go-no-logging.yml` and `examples/sample-playbook/sker.yaml`; acceptance: `cargo test -p sker-schema --test sample_playbook`
- [x] T010 [US2] Add one semantic cross-service notification rule with explicit activation signals and signal-present/signal-absent examples in `examples/sample-playbook/rules/cross-service-notification.yaml`, `examples/sample-playbook/fixtures/cross-service-notification/signal_present.yaml`, and `examples/sample-playbook/fixtures/cross-service-notification/signal_absent.yaml`; acceptance: `cargo test -p sker-schema --test sample_playbook`

**Checkpoint**: `sker-schema` contract tests pass and the sample Playbook contains one fixture-backed Go deterministic rule plus one signal-triggered semantic rule.

## Phase 2: Core Parallel Lanes — compiler, analyzer, retrieval/MCP, ledger

**Purpose**: Start lanes A–D concurrently after Phase 1. Each lane must write only within its stated directory allowlist.

### Lane A — compiler `[US1]` (write lock: `crates/sker-compiler/**`)

- [x] T011 [P] [US1] Implement central Playbook, business-lock, and repository-local source loading with pinned revision/content-hash verification and schema errors in `crates/sker-compiler/src/sources.rs`; acceptance: `cargo test -p sker-compiler`
- [x] T012 [US1] Implement source merge, override DAG cycle/conflict checks, rejection of local downgrade/exemption of central `BLOCK` rules, and atomic publication that preserves the previous valid bundle on every compile failure in `crates/sker-compiler/src/merge.rs` and `crates/sker-compiler/src/publish.rs`; acceptance: `cargo test -p sker-compiler`
- [x] T013 [US1] Implement deterministic fixture execution and publication rejection when either polarity fails in `crates/sker-compiler/src/fixtures.rs` and `crates/sker-compiler/tests/fixtures.rs`; acceptance: `cargo test -p sker-compiler`
- [x] T014 [US1] Implement canonical bundle hashing, read-only `bundle.rkyv` creation, and validated archive loading in `crates/sker-compiler/src/bundle.rs`; acceptance: `cargo test -p sker-compiler`
- [x] T015 [US1] Implement bounded, synchronized managed-block projections for AGENTS.md/CLAUDE.md and per-domain SKILL.md generation in `crates/sker-compiler/src/projections.rs`; generated repository files are runtime outputs, while compiler source/tests remain under this lane’s write lock; acceptance: `cargo test -p sker-compiler`
- [x] T016 [US1] Add source-lock, DAG, central-BLOCK, fixture, deterministic-output, atomic-failure-preserves-previous-bundle, and projection regression coverage in `crates/sker-compiler/tests/compiler_regressions.rs` and `crates/sker-compiler/fixtures/regression_playbook.yaml`; tests assert no bundle replacement after failed compile and that AGENTS.md/CLAUDE.md contain identical managed blocks with at most 3 `pinned_to_l1` red lines, exactly the two-step protocol, and no more than 300 tokens; acceptance: `cargo test -p sker-compiler`

### Lane B — analyzer `[US2]` (write lock: `crates/sker-analyzer/**`)

- [x] T017 [P] [US2] Implement incremental Git diff/tree extraction with pinned `gix` 0.90.0 features and deterministic path/status/mode/blob encoding in `crates/sker-analyzer/src/diff.rs`; acceptance: `cargo test -p sker-analyzer`
- [x] T018 [US2] Implement Go/Python/TypeScript/YAML-Infra deterministic ast-grep matching, added/modified-line filtering, and `scope.paths`/`scope.exempts` evaluation in `crates/sker-analyzer/src/rules.rs`; acceptance: `cargo test -p sker-analyzer`
- [x] T019 [US2] Implement semantic signal extraction that emits “pending semantic judgment” only when a declared signal is in the changed region in `crates/sker-analyzer/src/signals.rs`; acceptance: `cargo test -p sker-analyzer`
- [x] T020 [US2] Implement stable baseline matching by repository path, rule ID, syntax node/symbol, and normalized fragment, treating line as a hint and surfacing ambiguous identity in `crates/sker-analyzer/src/baseline.rs`; acceptance: `cargo test -p sker-analyzer`
- [x] T021 [US2] Implement read-only historical replay over an explicit window/rule-bundle version with partial-history reporting in `crates/sker-analyzer/src/replay.rs`; acceptance: `cargo test -p sker-analyzer`
- [x] T022 [US2] Implement SARIF 2.1.0 results with file/line locations and four-state outcomes in `crates/sker-analyzer/src/sarif.rs`; acceptance: `cargo test -p sker-analyzer`
- [x] T023 [US2] Implement verifier-only credential validation for canonical BLAKE3, bundle hash, independently recomputed diff-tree hash, required semantic declarations, and stale/replayed bindings in `crates/sker-analyzer/src/credential_verify.rs`; this module must not issue or mutate credentials; acceptance: `cargo test -p sker-analyzer`
- [x] T024 [US2] Add positive/negative language fixtures and baseline movement/format-only/material-change, exact `scope.paths` matching/nonmatching and `scope.exempts` exclusion, deterministic-blocking `alternatives` presence, replay, SARIF, and tampered-credential tests in `crates/sker-analyzer/tests/analysis_regressions.rs` and `crates/sker-analyzer/tests/fixtures/sample_sources.rs`; acceptance: `cargo test -p sker-analyzer`

### Lane C — retrieval and MCP `[US3]` (write lock: `crates/sker-retriever/**`, `crates/sker-mcp/**`)

- [x] T025 [P] [US3] Implement local Tantivy 0.27.0 BM25 indexing, synonym-tag expansion, path/intent scoping, bounded results, and threshold-based honest no-hit in `crates/sker-retriever/src/lib.rs`; acceptance: `cargo test -p sker-retriever`
- [x] T026 [US3] Add offline retrieval ranking, synonym, threshold/no-hit, and 500-rule bounded-result tests in `crates/sker-retriever/tests/retrieval.rs`; acceptance: `cargo test -p sker-retriever`
- [x] T027 [US3] Implement stateless rmcp 3.10.0 `#[tool_router]` stdio tools matching `contracts/mcp-tools.json` (`sker_consult`, `sker_search`, `sker_attest`, `sker_propose`) in `crates/sker-mcp/src/tools.rs`; acceptance: `cargo test -p sker-mcp`
- [x] T028 [US3] Implement local credential issuance after diff finalization, including consult trace, semantic verdicts, bundle/diff binding, RFC 8785 canonicalization, and BLAKE3 in `crates/sker-mcp/src/attest.rs`; acceptance: `cargo test -p sker-mcp`
- [x] T029 [US3] Implement PreToolUse host adapter and truthful delivery/failure status in `crates/sker-mcp/src/hook.rs`; acceptance: `cargo test -p sker-mcp`
- [x] T030 [US3] Add MCP JSON-schema compatibility, stdio tool, issuer, and Hook adapter tests in `crates/sker-mcp/tests/tool_contracts.rs`; acceptance: `cargo test -p sker-mcp`

### Lane D — governance ledger `[US6]` (write lock: `apps/sker-ledger/**`)

- [x] T031 [P] [US6] Implement Axum routes and request/response validation exactly for POST `/api/v1/events`, GET `/api/v1/metrics/overview`, and GET `/api/v1/rules/health` from `contracts/ledger-openapi.yaml` in `apps/sker-ledger/src/routes.rs`; acceptance: `cargo test -p sker-ledger`
- [x] T032 [US6] Implement SQLx/PostgreSQL migrations, sanitized idempotent event ingestion, and indexes in `apps/sker-ledger/migrations/0001_events.sql`; acceptance: `cargo test -p sker-ledger`
- [x] T033 [US6] Implement the seven metric calculations with explicit windows/numerators/denominators/sample counts and SNR `(not_triggered_in_logic + human_override) / total_triggers` in `apps/sker-ledger/src/metrics.rs`; acceptance: `cargo test -p sker-ledger`
- [x] T034 [US6] Implement materialized metric views, freshness, zero-sample no-data behavior, and refresh/query logic in `apps/sker-ledger/migrations/0002_metric_views.sql` and `apps/sker-ledger/src/metrics_store.rs`; acceptance: `cargo test -p sker-ledger`
- [x] T035 [US6] Implement 180-day suspected-dead, weekly SNR >70%, 24-hour cooldown, and hotfix-expiry notices without mutating rules in `apps/sker-ledger/src/health.rs`; acceptance: `cargo test -p sker-ledger`
- [x] T036 [US6] Implement the stateless single-app ledger container (`apps/sker-ledger/Dockerfile`) configured for an external PostgreSQL 16+ instance, plus a local/Preview `postgres:16` companion service and readiness dependencies in `apps/sker-ledger/docker-compose.yml`; acceptance: `docker compose -f apps/sker-ledger/docker-compose.yml config`
- [x] T037 [US6] Add OpenAPI route conformance, privacy rejection, duplicate ingestion, seven-metric/SNR, and lifecycle tests in `apps/sker-ledger/tests/ledger_contract.rs`; acceptance: `cargo test -p sker-ledger`

**Checkpoint**: Four lanes finish their crate-local tests before Phase 3 consumes their public APIs.

## Phase 3: Assembly & CI — CLI and GitHub Action

**Purpose**: Start lanes E–F concurrently after Phase 2 APIs stabilize. Each lane has a disjoint write root. Lane F is developed and contract-tested against frozen `contracts/cli-ci.json` in this phase; its tests use checked-in contract fixtures and do not depend on Lane E’s in-progress executable. True-binary interoperability is deferred to T052 in Phase 4.

### Lane E — CLI `[US5]` (write lock: `crates/sker-cli/**`)

- [x] T038 [P] [US5] Implement Clap command tree and wiring for compile/check/consult/search/attest/propose/replay/init/doctor/baseline in `crates/sker-cli/src/main.rs` and `crates/sker-cli/src/commands/mod.rs`; acceptance: `cargo test -p sker-cli`
- [x] T039 [US5] Implement idempotent language/host detection and managed configuration initialization in `crates/sker-cli/src/commands/init.rs`; acceptance: `cargo test -p sker-cli`
- [x] T040 [US5] Implement rule-bundle hash, managed-block size, Hook, MCP, and platform checks with actionable results in `crates/sker-cli/src/commands/doctor.rs`; acceptance: `cargo test -p sker-cli`
- [x] T041 [US5] Implement baseline create/update/list operations and auditable stable-anchor serialization in `crates/sker-cli/src/commands/baseline.rs`; acceptance: `cargo test -p sker-cli`
- [x] T042 [US5] Add CLI command and clean-repository integration tests in `crates/sker-cli/tests/commands.rs`; acceptance: `cargo test -p sker-cli`

### Lane F — GitHub Action `[US4]` (write lock: `action/**`)

- [x] T043 [P] [US4] Implement Action inputs, permissions, supported runner invocation, and SARIF upload wiring in `action/action.yml`, consuming the frozen `specs/002-sker-release-preview/contracts/cli-ci.json` argv, exit-code, and CheckReport schema contract; acceptance: `action/scripts/test-action.sh`
- [x] T044 [US4] Implement PR classification using protected Agent marker or a credential bound to the current diff, with Agent evidence overriding a “human-only” declaration, in `action/scripts/classify-pr.sh`; acceptance: `action/scripts/test-action.sh`
- [x] T045 [US4] Implement verifier-only CI flow that recomputes PR diff and validates the submitted local credential/declaration set without creating or repairing credentials in `action/scripts/verify-pr.sh`; acceptance: `action/scripts/test-action.sh`
- [x] T046 [US4] Implement SARIF 2.1.0 upload and Agent/human PR compliance-card behavior in `action/scripts/report-pr.sh`; acceptance: `action/scripts/test-action.sh`
- [x] T047 [US4] Implement sanitized asynchronous fail-open ledger event posting that cannot change gate status or delay completion in `action/scripts/post-ledger-event.sh`; acceptance: `action/scripts/test-action.sh`
- [x] T048 [US4] Add Agent evidence, pure-human, stale/tampered credential, CLI exit-code/report-shape, SARIF, card, and ledger-outage contract fixtures in `action/tests/classification-fixtures.json` and the action test runner in `action/scripts/test-action.sh`; Phase 3 tests against the frozen CLI contract without requiring Lane E’s binary; acceptance: `action/scripts/test-action.sh`

**Checkpoint**: CLI flows work locally; Action independently verifies PR state and remains fail-open for asynchronous telemetry.

## Phase 4: Integration, Dogfooding & Release

**Purpose**: Validate the assembled multi-language product, performance, this repository’s own policy, and blocking release artifacts.

- [ ] T049 [US2] Add end-to-end Go/Python/TypeScript/YAML-Infra changed-line, baseline, semantic-signal, and SARIF scenarios in `crates/sker-cli/tests/end_to_end.rs`; acceptance: `cargo test -p sker-cli --test end_to_end`
- [ ] T050 [US3] Add 500-rule offline consult/search load fixture and benchmark for p95 <200ms, bounded return size, and no model/token-context growth in `crates/sker-retriever/benches/500_rules.rs`; acceptance: `cargo bench -p sker-retriever --bench 500_rules`
- [ ] T051 [US2] Add an Analyzer benchmark using a fixed 10,000-added/modified-line diff and deterministic rule bundle in `crates/sker-analyzer/benches/incremental_10k.rs`; assert deterministic-check p95 ≤200ms on the documented blocking target hardware and report sample count and environment; acceptance: `cargo bench -p sker-analyzer --bench incremental_10k`
- [ ] T052 [US4] Add true-binary Phase 4 integration against the frozen CLI contract, covering `sker check --ci --sarif <path> --json <path>`, exit codes 0/1/2, both output files, and CheckReport JSON validation in `action/scripts/test-cli-contract.sh`; acceptance: `bash action/scripts/test-cli-contract.sh`
- [ ] T053 [US6] Add an executable Compose smoke check for app readiness and live PostgreSQL connectivity in `apps/sker-ledger/scripts/verify-compose.sh`; acceptance: `bash apps/sker-ledger/scripts/verify-compose.sh`
- [ ] T054 [US1] Add Sker’s own architecture red-line Playbook rules and positive/negative fixtures in `.sker/sker.yaml`, `.sker/rules/workspace-boundaries.yaml`, `.sker/fixtures/workspace-boundaries/positive.rs`, and `.sker/fixtures/workspace-boundaries/negative.rs`; acceptance: `cargo run -p sker-cli -- check --repo .`
- [ ] T055 [US5] Add static distribution targets and release settings for macOS arm64/x86_64 and Linux x86_64/arm64 in `dist-workspace.toml` and `Cargo.toml`; acceptance: `cargo dist build --target aarch64-apple-darwin --target x86_64-apple-darwin --target x86_64-unknown-linux-gnu --target aarch64-unknown-linux-gnu`
- [ ] T056 [US5] Add release matrix and non-blocking Windows source-compatibility check documentation in `specs/002-sker-release-preview/quickstart.md`; acceptance: `cargo check --workspace`
- [ ] T057 [US4] Run the full workspace quality gates and Action fixture suite, recording supported-platform results in `specs/002-sker-release-preview/quickstart.md`; acceptance: `cargo test --workspace && cargo clippy --workspace -- -D warnings && action/scripts/test-action.sh`

**Release gate**: Blocking acceptance is macOS arm64/x86_64 and Linux x86_64/arm64; Windows is source-compatible but non-blocking. Core workflows must pass offline with the ledger unavailable.

## Dependencies and execution order

### Wave dependencies

- **Phase 1** is serial: T001–T007 establish and test the shared schema, including every public type/Trait/error in `contracts/crate-traits.rs`; T008–T010 add the representative rule and fixture set. Phase 2 waits for all of Phase 1.
- **Phase 2** starts lanes A–D in parallel after Phase 1. Each lane has an exclusive write root: A `crates/sker-compiler/**`; B `crates/sker-analyzer/**`; C `crates/sker-retriever/**` plus `crates/sker-mcp/**`; D `apps/sker-ledger/**`. Do not place shared edits outside these roots during this wave.
- **Phase 3** starts lanes E–F in parallel after Phase 2 APIs stabilize. E writes only `crates/sker-cli/**`; F writes only `action/**` and consumes `contracts/cli-ci.json` plus frozen fixtures, not Lane E’s binary. Phase 4 T052 performs true-binary integration.
- **Phase 4** depends on Phase 3 assembly and runs end-to-end, benchmark, dogfood, release, and workspace gates. It may update integration-owned paths listed on its tasks.

### User-story dependencies

- **US1** schema/source policy and compiler: Phase 1 then Lane A; dogfood uses the completed compiler in Phase 4.
- **US2** sample semantic signal, analyzer, baseline/replay/SARIF: Phase 1 then Lane B; end-to-end validation in Phase 4.
- **US3** shared credential schema, retrieval/MCP/Hook: Phase 1 then Lane C; load/performance validation in Phase 4.
- **US4** PR gate: Phase 3 Lane F consumes the frozen `contracts/cli-ci.json` interface and fixtures in parallel with Lane E; Phase 4 T052 checks the real Lane E binary, then T057 runs the full action gate.
- **US5** CLI and release: Lane E depends on Phase 2 public APIs; distribution qualification is Phase 4.
- **US6** optional ledger: Lane D can proceed in parallel with other Phase 2 lanes and must not become a dependency for local/CI core workflows.

### Parallel execution example

```text
After Phase 1, start together:
  Lane A: T011 → T012 → T013 → T014 → T015 → T016 (crates/sker-compiler/**)
  Lane B: T017 → T018 → T019 → T020 → T021 → T022 → T023 → T024 (crates/sker-analyzer/**)
  Lane C: T025 → T026 → T027 → T028 → T029 → T030 (crates/sker-retriever/**, crates/sker-mcp/**)
  Lane D: T031 → T032 → T033 → T034 → T035 → T036 → T037 (apps/sker-ledger/**)

After Phase 2 APIs stabilize, start together:
  Lane E: T038 → T039 → T040 → T041 → T042 (crates/sker-cli/**)
  Lane F: T043 → T044 → T045 → T046 → T047 → T048 (action/**)
```

## Notes

- `[P]` marks lane entry work that can start concurrently across independent write roots; ordering arrows capture serial dependencies within each lane.
- `[USx]` maps each task to the spec story that owns its user-visible outcome.
- Cross-crate payloads and API boundaries must use `sker-schema` types. Do not add ad hoc crate-to-crate structs.
- Tests are included where required by the Constitution, explicit user scope, or a contract acceptance criterion. No acceptance command in this plan has been executed as part of task generation.
- `contracts/cli-ci.json` freezes `sker check --ci --sarif <path> --json <path>`, exit-code meanings, atomic report behavior, and CheckReport JSON shape before Lane F begins; changes require updating the contract and both lane test suites.
