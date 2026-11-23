#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
action_dir="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$action_dir/.." && pwd)"
sker_bin="${SKER_BIN:-$repo_root/target/release/sker}"
export SKER_BIN="$sker_bin"

if [[ ! -x "$sker_bin" ]]; then
  echo "Building release sker binary..."
  cargo build --release -p sker --manifest-path "$repo_root/Cargo.toml"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git_repo="$tmp/repo"
mkdir -p "$git_repo/.sker/rules" "$git_repo/services/payments"

# Copy sample deterministic rule
cp "$repo_root/examples/sample-playbook/rules/go-no-logging.yaml" "$git_repo/.sker/rules/"

git -C "$git_repo" init -q
git -C "$git_repo" config user.email "ci-contract@example.invalid"
git -C "$git_repo" config user.name "Contract Test"

# Commit base version
cat > "$git_repo/services/payments/main.go" <<'GO'
package payments

func ProcessPayment() string {
	return "ok"
}
GO
# Compile bundle and commit initial baseline state
"$sker_bin" compile --repo "$git_repo" >/dev/null
git -C "$git_repo" add .
git -C "$git_repo" commit -qm "base commit with compiled bundle and projections"
base_rev="$(git -C "$git_repo" rev-parse HEAD)"

echo "=== Testing True Binary Against Frozen cli-ci Contract ==="

# Validate CheckReport and SARIF against contract schema using Node.js
validate_reports() {
  local report_file="$1"
  local sarif_file="$2"
  local expected_code="$3"

  node - "$report_file" "$sarif_file" "$expected_code" <<'NODE'
const fs = require('node:fs');
const report = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const sarif = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
const code = Number(process.argv[4]);

// CheckReport contract validation
const hex = (v, n) => typeof v === 'string' && new RegExp(`^[0-9a-f]{${n}}$`).test(v);
if (report.schema_version !== 'sker.check-report/v1') throw new Error('invalid schema_version');
if (!['passed', 'blocked', 'incomplete'].includes(report.status)) throw new Error('invalid status');
if (!hex(report.bundle_sha256, 64)) throw new Error('invalid bundle_sha256');
if (!hex(report.diff_tree_hash, 64)) throw new Error('invalid diff_tree_hash');
if (!Number.isInteger(report.changed_line_count) || report.changed_line_count < 0) throw new Error('invalid changed_line_count');
if (!Array.isArray(report.findings)) throw new Error('findings not an array');
if (typeof report.analysis.complete !== 'boolean') throw new Error('analysis.complete not boolean');

if (code === 0 && report.status !== 'passed') throw new Error('code 0 must have status passed');
if (code === 1 && report.status !== 'blocked') throw new Error('code 1 must have status blocked');
if (code === 2 && report.status !== 'incomplete') throw new Error('code 2 must have status incomplete');

// SARIF 2.1.0 validation
if (sarif.version !== '2.1.0') throw new Error('invalid sarif version');
if (!Array.isArray(sarif.runs) || sarif.runs.length === 0) throw new Error('sarif missing runs');
NODE
}

# ---------------------------------------------------------------------------
# Test 1: Exit Code 0 (Compliant change, no blocking findings)
# ---------------------------------------------------------------------------
echo "--- Scenario 0: Compliant diff -> Exit code 0 ---"
cat > "$git_repo/services/payments/main.go" <<'GO'
package payments

func ProcessPayment() string {
	const result = "processed"
	return result
}
GO
git -C "$git_repo" add services/payments/main.go
git -C "$git_repo" commit -qm "compliant change"

report0="$tmp/report-0.json"
sarif0="$tmp/sarif-0.sarif"
set +e
"$sker_bin" check --ci --repo "$git_repo" --base "$base_rev" --sarif "$sarif0" --json "$report0"
code0=$?
set -e

[[ "$code0" -eq 0 ]] || { echo "Expected exit code 0, got $code0" >&2; exit 1; }
[[ -f "$report0" ]] || { echo "Report 0 missing" >&2; exit 1; }
[[ -f "$sarif0" ]] || { echo "SARIF 0 missing" >&2; exit 1; }
validate_reports "$report0" "$sarif0" 0

# verify-pr.sh for pure-human PR on exit 0 should pass
SKER_ACTOR=human SKER_EXIT_CODE=0 INPUT_REPORT_FILE="$report0" bash "$action_dir/scripts/verify-pr.sh"

# ---------------------------------------------------------------------------
# Test 2: Exit Code 1 (Deterministic BLOCK finding)
# ---------------------------------------------------------------------------
echo "--- Scenario 1: Deterministic BLOCK finding -> Exit code 1 ---"
cat > "$git_repo/services/payments/main.go" <<'GO'
package payments

import "fmt"

func ProcessPayment() string {
	fmt.Printf("logging card %s", "1234")
	return "done"
}
GO
git -C "$git_repo" add services/payments/main.go
git -C "$git_repo" commit -qm "violation change"

report1="$tmp/report-1.json"
sarif1="$tmp/sarif-1.sarif"
set +e
"$sker_bin" check --ci --repo "$git_repo" --base "$base_rev" --sarif "$sarif1" --json "$report1"
code1=$?
set -e

[[ "$code1" -eq 1 ]] || { echo "Expected exit code 1, got $code1" >&2; exit 1; }
[[ -f "$report1" ]] || { echo "Report 1 missing" >&2; exit 1; }
[[ -f "$sarif1" ]] || { echo "SARIF 1 missing" >&2; exit 1; }
validate_reports "$report1" "$sarif1" 1

# verify-pr.sh for human PR on exit 1 must fail (block)
set +e
SKER_ACTOR=human SKER_EXIT_CODE=1 INPUT_REPORT_FILE="$report1" bash "$action_dir/scripts/verify-pr.sh" 2>/dev/null
v_code1=$?
set -e
[[ "$v_code1" -ne 0 ]] || { echo "verify-pr.sh should block on exit code 1" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Test 3: Exit Code 2 (Incomplete analysis due to unsupported path)
# ---------------------------------------------------------------------------
echo "--- Scenario 2: Incomplete analysis -> Exit code 2 ---"
# Only files covered by an active deterministic rule scope may trigger
# incomplete. Add a rule scoping *.bin, then recompile so the bundle sees it.
cat > "$git_repo/.sker/rules/bin-scope.yaml" <<'YAML'
schema_version: sker.entry/v1
rule_id: regression.bin.no-eval
version: 1.0.0
title: Binary artifacts must not embed eval payloads
domain: security
description: Scoped deterministic rule covering binary artifacts.
severity: BLOCK
track: deterministic
scope:
  include_paths: ["**/*.bin"]
signals:
  - signal_id: bin-eval
    languages: [python]
    description: eval call
    pattern: "eval($$$ARGS)"
exemptions: []
overrides: []
fixtures:
  - fixture_id: eval-call
    polarity: positive
    path: examples/sample-playbook/fixtures/py-no-eval/positive/eval.py
    expected: trigger
  - fixture_id: safe-int-call
    polarity: negative
    path: examples/sample-playbook/fixtures/py-no-eval/negative/safe.py
    expected: no_trigger
compliant_alternatives: ["ast.literal_eval(...)"]
lifecycle:
  kind: ordinary
  cooldown_hours: 24
consult:
  synonyms: []
  keywords: []
tags: [binary]
YAML
"$sker_bin" compile --repo "$git_repo" >/dev/null
echo "unsupported format" > "$git_repo/services/payments/unsupported.bin"
git -C "$git_repo" add services/payments/unsupported.bin
git -C "$git_repo" commit -qm "add unsupported file"

report2="$tmp/report-2.json"
sarif2="$tmp/sarif-2.sarif"
set +e
"$sker_bin" check --ci --repo "$git_repo" --base "$base_rev" --sarif "$sarif2" --json "$report2"
code2=$?
set -e

[[ "$code2" -eq 2 ]] || { echo "Expected exit code 2, got $code2" >&2; exit 1; }
[[ -f "$report2" ]] || { echo "Report 2 missing" >&2; exit 1; }
[[ -f "$sarif2" ]] || { echo "SARIF 2 missing" >&2; exit 1; }
validate_reports "$report2" "$sarif2" 2

# verify-pr.sh on exit 2:
# 1) for Agent PR, it must fail (exit non-zero)
set +e
SKER_ACTOR=agent SKER_EXIT_CODE=2 INPUT_REPORT_FILE="$report2" bash "$action_dir/scripts/verify-pr.sh" 2>/dev/null
v_agent_code2=$?
set -e
[[ "$v_agent_code2" -ne 0 ]] || { echo "verify-pr.sh should block Agent PR on exit 2" >&2; exit 1; }

# 2) for Pure-Human PR, it must pass (fail-open)
SKER_ACTOR=human SKER_EXIT_CODE=2 INPUT_REPORT_FILE="$report2" bash "$action_dir/scripts/verify-pr.sh"

# ---------------------------------------------------------------------------
# Test 4: Agent PR Real BLAKE3 Credential Attestation & Verification
# ---------------------------------------------------------------------------
echo "--- Scenario 4: Real BLAKE3 Credential Attestation & Verification ---"
# Create clean state with compliant code
rm -f "$git_repo/services/payments/unsupported.bin"
cat > "$git_repo/services/payments/main.go" <<'GO'
package payments

func CleanFunction() string {
	return "valid"
}
GO
git -C "$git_repo" add .
git -C "$git_repo" commit -qm "clean compliant diff"

report_agent="$tmp/report-agent.json"
sarif_agent="$tmp/sarif-agent.sarif"
set +e
"$sker_bin" check --ci --repo "$git_repo" --base "$base_rev" --sarif "$sarif_agent" --json "$report_agent"
agent_check_code=$?
set -e
if [[ "$agent_check_code" -ne 0 ]]; then
  echo "Agent check failed with code $agent_check_code" >&2
  if [[ -f "$report_agent" ]]; then
    cat "$report_agent" >&2
  fi
  exit 1
fi

# Attest using real sker binary
cred_file="$git_repo/.sker/credential.json"
export GITHUB_BASE_SHA="$base_rev"
"$sker_bin" attest --repo "$git_repo" --semantic "[]" --output "$cred_file"
[[ -f "$cred_file" ]] || { echo "Credential file missing" >&2; exit 1; }

# Verify valid Agent PR passes verify-pr.sh
SKER_ACTOR=agent SKER_EXIT_CODE=0 INPUT_REPORT_FILE="$report_agent" INPUT_CREDENTIAL_FILE="$cred_file" bash "$action_dir/scripts/verify-pr.sh"

# Tamper with credential BLAKE3 digest -> verify-pr.sh must block
tampered_cred="$tmp/tampered_credential.json"
node - "$cred_file" "$tampered_cred" <<'NODE'
const fs = require('node:fs');
const c = JSON.parse(fs.readFileSync(process.argv[2]));
c.credential_blake3 = "0".repeat(64);
fs.writeFileSync(process.argv[3], JSON.stringify(c, null, 2));
NODE

set +e
SKER_ACTOR=agent SKER_EXIT_CODE=0 INPUT_REPORT_FILE="$report_agent" INPUT_CREDENTIAL_FILE="$tampered_cred" bash "$action_dir/scripts/verify-pr.sh" 2>/dev/null
tampered_exit=$?
set -e
[[ "$tampered_exit" -ne 0 ]] || { echo "verify-pr.sh must reject tampered credential" >&2; exit 1; }

# Attest with evidence_path and evidence_line -> verify-pr.sh must pass
cred_evidence="$tmp/evidence_credential.json"
"$sker_bin" attest --repo "$git_repo" --semantic '[{"rule_id":"R-SEM","state":"comply","declaration":"Reviewed logic","evidence_path":"services/payments/main.go","evidence_line":1}]' --output "$cred_evidence"
SKER_ACTOR=agent SKER_EXIT_CODE=0 INPUT_REPORT_FILE="$report_agent" INPUT_CREDENTIAL_FILE="$cred_evidence" bash "$action_dir/scripts/verify-pr.sh"

# Credential with escalate verdict -> verify-pr.sh must block (Audit #6)
cred_escalate="$tmp/escalate_credential.json"
"$sker_bin" attest --repo "$git_repo" --semantic '[{"rule_id":"R-SEM","state":"escalate","declaration":"Needs human lead approval"}]' --output "$cred_escalate"
set +e
SKER_ACTOR=agent SKER_EXIT_CODE=0 INPUT_REPORT_FILE="$report_agent" INPUT_CREDENTIAL_FILE="$cred_escalate" bash "$action_dir/scripts/verify-pr.sh" 2>/dev/null
escalate_exit=$?
set -e
[[ "$escalate_exit" -ne 0 ]] || { echo "verify-pr.sh must reject escalate state" >&2; exit 1; }

echo "=== True Binary CLI Contract Integration: ALL TESTS PASSED ==="
