<!--
Sync Impact Report
- Version change: unratified template → 1.0.0 (initial adoption)
- Modified principles: template placeholders → Sker Release Preview principles I–V
- Added sections: Rust Constraints; Quality Gates
- Removed sections: none
- Follow-up TODOs: confirm the original ratification date
-->

# Sker Release Preview Constitution

## Core Principles

### I. Git Is the Sole Source of Truth
Git is the only source of truth for project state and policy. A change becomes effective
only after approval by two human reviewers on its pull request. Agents MUST only propose
candidate pull requests; they MUST NOT approve or make changes effective. This keeps
authority auditable and with human maintainers.

### II. Offline-First Operation
The `consult`, `check`, and CI workflows MUST run to completion without depending on a
remote central service. Remote integrations MAY add convenience, but MUST NOT be required
for these workflows. This keeps core review and validation available in disconnected and
self-hosted environments.

### III. Deterministic and Semantic Checks Are Separate
The system MUST distinguish `deterministic` checks, whose results can be computed
reproducibly, from `semantic` judgments that cannot be decided reliably by static rules.
It MUST NOT present an undecidable or semantic question as statically decidable. Reports
and outcomes MUST identify which track produced them, so users can understand the limits
of each result.

### IV. Context Cost Is Independent of Rule Count
The context cost of `consult`, `check`, and CI MUST be decoupled from the number of rules
(`n`). Adding rules MUST NOT require loading every rule into every operation's context;
the implementation MUST select or retrieve only the context needed for the current task.
This keeps review cost predictable as the rule set grows.

### V. Crates Communicate Through Shared Schema
All workspace Crates MUST exchange data through types defined by `sker-schema`. Crates
MUST NOT establish ad hoc cross-Crate data contracts. Changes to shared structures MUST
be represented in `sker-schema`, making boundaries explicit and compatible across the
workspace.

## Rust Constraints

The workspace MUST NOT introduce dependencies that require C-linked libraries such as
OpenSSL or libgit2. Use `gix` for Git operations, `rustls` for TLS, and `ast-grep-core`
for syntax-aware code analysis. Any exception requires amending this Constitution before
the dependency is introduced.

## Quality Gates

Every Crate MUST have unit tests and boundary tests. Every static rule MUST be validated
with positive and negative fixtures. Before a change is merged, both of the following
commands MUST pass:

```sh
cargo test --workspace && cargo clippy --workspace -- -D warnings
```

Pull request review MUST verify compliance with these gates and the Core Principles.

## Governance

This Constitution is the authoritative governance document for Sker Release Preview.
Amendments MUST be made in Git and require approval from two human reviewers; an agent
may prepare a candidate amendment but cannot approve or enact it. Reviewers MUST assess
the effect of an amendment on existing crates, rules, fixtures, workflows, and dependent
project guidance before approval.

The version follows Semantic Versioning: increment MAJOR for incompatible governance
changes or removals, MINOR for new principles or materially expanded requirements, and
PATCH for clarifications and non-semantic edits. Every amendment MUST update the version
and last-amended date. Compliance MUST be reviewed during pull request review, and
violations MUST be corrected before merge or addressed by an approved Constitution
amendment.

**Version**: 1.0.0 | **Ratified**: 2026-11-17 | **Last Amended**: 2026-11-17
