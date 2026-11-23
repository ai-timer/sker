#!/usr/bin/env bash
set -euo pipefail
actor="${GITHUB_OUTPUT:-}"
# GITHUB_OUTPUT values are not readable; classification is passed through the job environment by the workflow.
actor="${SKER_ACTOR:-human}"
code="${SKER_EXIT_CODE:-2}"
report="${INPUT_REPORT_FILE:-sker-check-report.json}"
credential="${INPUT_CREDENTIAL_FILE:-.sker/credential.json}"
fail() { echo "::error::$1"; exit 1; }

# Validate complete report shape/status according to the frozen cli-ci contract.
if [[ "$code" == 0 || "$code" == 1 ]]; then
  [[ -f "$report" ]] || fail "Sker returned $code without a CheckReport"
  node - "$report" "$code" <<'NODE' || fail "CheckReport is malformed or inconsistent with the CLI exit code"
const fs = require('node:fs');
const r = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const code = Number(process.argv[3]);
const hex = (v, n) => typeof v === 'string' && new RegExp(`^[0-9a-f]{${n}}$`).test(v);
const keys = (obj, required, allowed) => obj && required.every(k => Object.hasOwn(obj,k)) && Object.keys(obj).every(k => allowed.includes(k));
if (!keys(r,['schema_version','status','bundle_sha256','diff_tree_hash','changed_line_count','findings','analysis'],['schema_version','status','bundle_sha256','diff_tree_hash','changed_line_count','findings','analysis']) ||
    r.schema_version !== 'sker.check-report/v1' || !['passed','blocked','incomplete'].includes(r.status) ||
    !hex(r.bundle_sha256, 64) || !hex(r.diff_tree_hash, 64) || !Number.isInteger(r.changed_line_count) || r.changed_line_count < 0 ||
    !Array.isArray(r.findings) || !keys(r.analysis,['complete','unsupported_paths','warnings'],['complete','unsupported_paths','warnings']) || typeof r.analysis.complete !== 'boolean' ||
    !Array.isArray(r.analysis.unsupported_paths) || !r.analysis.unsupported_paths.every(x=>typeof x==='string') || !Array.isArray(r.analysis.warnings) || !r.analysis.warnings.every(x=>typeof x==='string') ||
    r.status === 'incomplete' || !r.analysis.complete || (code === 0 && r.status !== 'passed') || (code === 1 && r.status !== 'blocked') ||
    (code === 0 && r.findings.some(f => f.track === 'deterministic' && f.severity === 'BLOCK' && f.disposition === 'blocked')) ||
    (code === 1 && !r.findings.some(f => f.track === 'deterministic' && f.severity === 'BLOCK' && f.disposition === 'blocked'))) process.exit(1);
for (const f of r.findings) {
  if (!keys(f,['rule_id','path','start_line','end_line','track','severity','disposition','message','alternatives'],['rule_id','path','start_line','end_line','track','severity','disposition','message','alternatives','baseline_anchor']) ||
      typeof f.rule_id !== 'string' || !f.rule_id || typeof f.path !== 'string' || !f.path || !Number.isInteger(f.start_line) || f.start_line < 1 || !Number.isInteger(f.end_line) || f.end_line < 1 ||
      !['deterministic','semantic'].includes(f.track) || !['BLOCK','WARN','INFO'].includes(f.severity) ||
      !['blocked','warning','pending_semantic','baseline_suppressed','exempt'].includes(f.disposition) ||
      typeof f.message !== 'string' || !f.message || !Array.isArray(f.alternatives) || !f.alternatives.every(x=>typeof x==='string'&&x)) process.exit(1);
  if (Object.hasOwn(f,'baseline_anchor') && f.baseline_anchor !== null && typeof f.baseline_anchor !== 'string') process.exit(1);
}
NODE
fi

# Agent evidence always requires an existing local credential; CI never creates or repairs one.
if [[ "$actor" == agent ]]; then
  [[ -f "$credential" ]] || fail "Agent PR is missing its locally issued credential"
  [[ -f "$report" ]] || fail "The recomputed CheckReport is missing"
  sker_bin="${SKER_BIN:-${INPUT_SKER_PATH:-}}"
  # Capability probe by output: mock binaries may exit 0 for unknown subcommands.
  verify_help=""
  if [[ -n "$sker_bin" ]]; then
    verify_help="$("$sker_bin" verify-credential --help 2>/dev/null || true)"
  fi
  if [[ "$verify_help" == *"--credential"* ]]; then
    # Real sker binary: built-in pure-Rust BLAKE3 + RFC 8785 verification, no external b3sum.
    "$sker_bin" verify-credential --credential "$credential" --report "$report" >/dev/null \
      || fail "Agent credential is stale, tampered, malformed, or missing a required semantic declaration"
  else
    # Legacy fallback for mock-binary test environments that lack a real sker executable.
    [[ "$(command -v b3sum || true)" ]] || fail "BLAKE3 verifier is unavailable; refusing to accept Agent credential"
    node - "$credential" "$report" <<'NODE' || fail "Agent credential is stale, malformed, or missing a required semantic declaration"
const fs = require('node:fs');
const c = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const report = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
const diff = report.diff_tree_hash, bundle = report.bundle_sha256;
const states = new Set(['comply','not_triggered_in_logic','exempt','escalate']);
if (c.schema_version !== 'sker.agent-credential/v1' || c.diff_tree_hash !== diff || c.bundle_sha256 !== bundle ||
    !Array.isArray(c.semantic_verdicts) || !Array.isArray(c.consult_trace) || !c.issuer || !c.created_at || !/^[0-9a-f]{64}$/.test(c.credential_blake3 || '')) process.exit(1);
const byRule = new Map();
for (const v of c.semantic_verdicts) {
  if (!v.rule_id || !states.has(v.state) || typeof v.declaration !== 'string' || !v.declaration.trim()) process.exit(1);
  if (v.state === 'escalate') process.exit(1);
  byRule.set(v.rule_id, v);
}
for (const f of report.findings) if (f.track === 'semantic' && f.disposition === 'pending_semantic' && !byRule.has(f.rule_id)) process.exit(1);
NODE
    canonical="$(node - "$credential" <<'NODE'
const fs = require('node:fs');
const c = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
delete c.credential_blake3;
for (const v of c.semantic_verdicts || []) {
  if (v && v.rationale_digest === null) delete v.rationale_digest;
  if (v && v.evidence_path === null) delete v.evidence_path;
  if (v && v.evidence_line === null) delete v.evidence_line;
}
function canon(v) {
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  if (v && typeof v === 'object') return `{${Object.keys(v).sort().map(k => `${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}`;
  return JSON.stringify(v);
}
process.stdout.write(canon(c));
NODE
)" || fail "Unable to canonicalize the submitted credential"
    actual="$(printf '%s' "$canonical" | b3sum | awk '{print $1}')"
    expected="$(node - "$credential" <<'NODE'
const c = require(process.argv[2]); process.stdout.write(c.credential_blake3);
NODE
)"
    [[ "$actual" == "$expected" ]] || fail "Agent credential BLAKE3 digest does not match"
  fi
fi

case "$code" in
  0) exit 0 ;;
  1) fail "Sker found deterministic blocking findings" ;;
  2) if [[ "$actor" == agent ]]; then fail "Sker analysis was incomplete for an Agent PR"; fi ;;
esac
# Exit 2 on a verified pure-human PR is fail-open; report-pr still publishes semantic self-check guidance.
