# Data Model: Sker Release Preview

## Design boundary

All cross-crate structures are defined in `sker-schema` and serialized through its versioned schema. The compiler creates immutable local rule bundles; the analyzer and retriever consume those bundles without requiring the optional governance ledger. The ledger stores only sanitized events and derived aggregates.

## Entities

### Rule source and lock

Identifies one input to a compiled bundle.

| Field | Meaning | Validation |
|---|---|---|
| `kind` | Central Playbook, business repository lock, or repository-local source | Required enum |
| `source_id` | Stable source identity | Non-empty |
| `version` | Pinned source revision | Exact revision required for remote or dependent sources |
| `content_hash` | Integrity digest of the source snapshot | Must match content at compile time |

### Rule

An independently identifiable policy item.

| Field | Meaning | Validation |
|---|---|---|
| `rule_id` | Stable identifier | Unique within the compiled source set |
| `domain`, `paths` | Subject area and path scope | Path patterns validate before publication |
| `track` | `deterministic` or `semantic` | Required; result reports the selected track |
| `severity` | Enforcement level, including central `BLOCK` | A repository rule cannot weaken or exempt a central `BLOCK` |
| `signals` | Diff code signals that activate the rule | Semantic rules require at least one signal |
| `exemptions` | Explicitly scoped exceptions | Expired or conflicting exemptions fail closed |
| `overrides` | Explicit precedence edges | Graph must be acyclic; precedence must be unambiguous |
| `fixtures` | Positive and negative examples for deterministic rules | At least one passing fixture of each polarity before publish |
| `lifecycle` | Ordinary cooldown or hotfix expiry | Hotfix expiry must be present and valid |
| `synonyms` | Search expansion labels for consult | Metadata only; no model artifact is required |

### Rule bundle

Immutable, reproducible snapshot consumed by check, consult, CI, and attest.

| Field | Meaning | Validation |
|---|---|---|
| `bundle_id`, `bundle_sha256` | Identity and stable content digest | SHA-256 covers the canonical logical representation of normalized rules and source locks |
| `sources` | Source and lock snapshots | Every source hash matches |
| `rules` | Compiled rules | Schema, graph, conflict, and fixture validation pass |
| `created_at`, `compiler_version` | Reproduction metadata | Required in artifact metadata |

### Baseline item

Tracks one existing finding so unchanged debt does not fail every incremental check.

| Field | Meaning | Validation |
|---|---|---|
| `repository_path`, `rule_id` | Stable logical identity components | Repository-relative path and known rule |
| `syntax_anchor` / `symbol` | Syntax node or enclosing symbol when available | Used as a stable match anchor, not a line-number identity |
| `normalized_fragment` | Normalized offending code fragment | Formatting/comment-only changes do not alter matching identity |
| `line_hint` | Last known location for display | Advisory only; never the sole identity key |
| `exception`, `audit` | Baseline rationale and history | Changes are reviewable and auditable |

Re-evaluate a baseline item when the relevant syntax node or normalized problem fragment materially changes. If a language/file has no syntax anchor, use the normalized problem fragment with path and rule identity; unresolved ambiguity must be surfaced rather than silently exempted.

### Check result and attestation

A check result records findings for one diff. The local Agent/CLI process accumulates consult trace before and during coding, then finalizes an attestation once the candidate diff exists and before commit/PR submission. CI is verifier-only.

| Field | Meaning | Validation |
|---|---|---|
| `bundle_sha256` | Identity of the rule bundle used by the local Agent | Required; CI recomputes/loads the PR bundle identity and compares it |
| `diff_tree_hash` | Lowercase-hex BLAKE3 hash of the canonical candidate diff tree | Required; CI independently recomputes and compares it |
| `consult_trace[]` | Ordered consult records: query digest, selected rule IDs, scoped paths, timestamp, and result status | Captured locally before/during coding; raw prompts are excluded |
| `semantic_verdicts[]` | Per-rule semantic declaration and verdict | Required for triggered semantic rules; state is one of the four allowed states |
| `credential_blake3` | Lowercase-hex BLAKE3 digest of the RFC 8785 canonical JSON credential payload, excluding this field | Recomputed by CI; malformed or mismatched digest is rejected |
| `findings` | Rule, path, range, track, severity, disposition | Deterministic and semantic tracks remain distinguishable |
| `issuer` | Local Agent/CLI identity and tool (`MCP`, `Hook`, or `sker attest`) | Present in the payload covered by the digest |
| `created_at`, `schema_version` | Credential versioning and local creation time | Present in the payload covered by the digest |

The credential is generated locally; CI MUST NOT create, repair, refresh, or replace it. `bundle_sha256` is SHA-256 over the canonical logical bundle representation. `diff_tree_hash` is BLAKE3 over a deterministic, path-sorted encoding of changed paths, change status, old/new file modes, and old/new blob bytes with length-prefixed fields. `credential_blake3` is BLAKE3 over the RFC 8785 JSON Canonicalization Scheme UTF-8 encoding of the credential object without the `credential_blake3` field. CI independently recomputes the PR diff, verifies the digest and binding fields, checks the semantic declaration set for completeness, and rejects stale/replayed credentials. The BLAKE3 digest proves payload consistency with the submitted credential; it is not a digital signature or proof of issuer identity.

### Consult trace entry

One local retrieval operation included in the final credential.

| Field | Meaning | Validation |
|---|---|---|
| `consult_id`, `occurred_at` | Unique operation reference and time | Required |
| `query_digest` | Lowercase-hex BLAKE3 digest of normalized query intent | Required; raw prompt text is not serialized |
| `paths[]` | Repository-relative target paths | Normalized and repository-relative |
| `matched_rule_ids[]` | Rules returned to the Agent | Stable IDs from the bound bundle |
| `result_status` | `matched`, `no_match`, or `error` | Required; errors cannot be represented as no match |

### Proposal

Agent-authored candidate rule change.

State transitions: `pending` → human review → merged by the normal Git process. An agent cannot approve, activate, or publish a proposal. The proposal carries authoring context and a diff but does not alter the active bundle.

### Governance event and metric

An optional, sanitized event supports asynchronous aggregate governance reporting.

| Field | Meaning | Validation |
|---|---|---|
| `event_id`, `occurred_at`, `event_type` | Idempotent event identity and kind | Duplicate IDs are counted once |
| `rule_id`, `bundle_sha256`, `track`, `state` | Rule/result context | No source code or prompt content |
| `window`, `numerator`, `denominator`, `sample_count` | Metric provenance | Zero denominator is `no_data`, not zero |
| `freshness` | Ingestion delay or outage state | Missing/stale data is visible |

The ledger derives the seven specified team metrics and rule health notices. Noise ratio uses the spec's SNR formula; the ledger never changes, deletes, or activates rules.

## Relationships

- A rule bundle contains source locks and compiled rules.
- A rule may have multiple scopes, signals, exemptions, fixtures, and lifecycle metadata.
- A baseline item references one rule and one repository location/anchor.
- A check result is computed from one diff and one rule bundle; the local Agent/CLI credential binds `bundle_sha256` and `diff_tree_hash`, includes its consult trace and semantic verdicts, and is only verified by CI.
- A final credential is created after the candidate diff exists and before commit/PR submission; its consult trace can contain operations that occurred before/during coding.
- Governance events refer to rule/result identities and are aggregated into metrics over explicit windows.
- Proposals are reviewed through Git and only affect a future compiled bundle after human approval and merge.
