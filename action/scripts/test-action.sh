#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="$(cd "$root/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

for script in "$root"/scripts/*.sh; do bash -n "$script"; done
for script in "$root"/scripts/*.js; do node --check "$script"; done
node -e 'JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"))' "$root/tests/classification-fixtures.json"
ruby -e 'require "yaml"; d=YAML.load_file(ARGV[0]); abort "composite action must use composite" unless d.dig("runs","using")=="composite"; abort "missing steps" unless d.dig("runs","steps").is_a?(Array)' "$root/action.yml"
node -e 'const s=JSON.parse(require("node:fs").readFileSync(process.argv[1])); if(s.version!=="2.1.0"||!Array.isArray(s.runs))process.exit(1)' "$root/tests/sarif-2.1.0.json"

cat > "$tmp/report.js" <<'NODE'
const fs = require('node:fs');
const code = Number(process.argv[2]);
const deterministic = code === 1 ? [{rule_id:'R-BLOCK',path:'src/a.rs',start_line:4,end_line:4,track:'deterministic',severity:'BLOCK',disposition:'blocked',message:'Deterministic violation',alternatives:['Use safe API']}]:[];
const findings = [...deterministic,{rule_id:'R-SEM',path:'src/a.rs',start_line:8,end_line:8,track:'semantic',severity:'WARN',disposition:'pending_semantic',message:'Review high-risk semantic behavior',alternatives:[]}];
fs.writeFileSync(process.argv[3], JSON.stringify({schema_version:'sker.check-report/v1',status:code===1?'blocked':code===2?'incomplete':'passed',bundle_sha256:'a'.repeat(64),diff_tree_hash:'b'.repeat(64),changed_line_count:3,findings,analysis:{complete:code!==2,unsupported_paths:[],warnings:[]}}));
NODE
cat > "$tmp/mock-sker" <<'MOCK'
#!/usr/bin/env bash
set -e
if [[ "$1" == "compile" ]]; then exit 0; fi
[[ "$1 $2 $3" == "check --ci --sarif" ]]
node "$REPORT_FACTORY" "$MOCK_CODE" "$MOCK_REPORT"
exit "$MOCK_CODE"
MOCK
chmod +x "$tmp/mock-sker"
mkdir -p "$tmp/bin"
cat > "$tmp/bin/b3sum" <<'B3'
#!/usr/bin/env bash
cat >/dev/null
printf '%s  -\n' "${TEST_DIGEST:-$(printf '0%.0s' {1..64})}"
B3
chmod +x "$tmp/bin/b3sum"

# Every exit-code/actor pair is exercised with a contract-shaped mock CheckReport.
while read -r code actor expected; do
  report="$tmp/report-$code-$actor.json"
  export REPORT_FACTORY="$tmp/report.js" MOCK_CODE="$code" MOCK_REPORT="$report"
  export INPUT_SKER_PATH="$tmp/mock-sker" INPUT_SARIF_FILE="$tmp/out.sarif" INPUT_REPORT_FILE="$report" GITHUB_OUTPUT="$tmp/outputs"
  bash "$root/scripts/run-check.sh"
  credential="$tmp/credential.json"
  if [[ "$actor" == agent ]]; then
    node - "$report" "$credential" <<'NODE'
const fs=require('node:fs'); const r=JSON.parse(fs.readFileSync(process.argv[2]));
fs.writeFileSync(process.argv[3],JSON.stringify({schema_version:'sker.agent-credential/v1',bundle_sha256:r.bundle_sha256,diff_tree_hash:r.diff_tree_hash,consult_trace:[],semantic_verdicts:[{rule_id:'R-SEM',state:'comply',declaration:'Reviewed'}],issuer:'local_agent',created_at:'2026-11-18T00:00:00Z',credential_blake3:'c'.repeat(64)}));
NODE
  else rm -f "$credential"; fi
  export SKER_ACTOR="$actor" SKER_EXIT_CODE="$code" INPUT_CREDENTIAL_FILE="$credential" INPUT_REPORT_FILE="$report" TEST_DIGEST="$(printf 'c%.0s' {1..64})" PATH="$tmp/bin:$PATH"
  set +e
  bash "$root/scripts/verify-pr.sh" >"$tmp/verify.out" 2>&1
  actual=$?
  set -e
  if [[ "$expected" == true && $actual -eq 0 ]]; then echo "expected $actor exit $code to block" >&2; exit 1; fi
  if [[ "$expected" == false && $actual -ne 0 ]]; then cat "$tmp/verify.out" >&2; echo "expected $actor exit $code to pass" >&2; exit 1; fi
done < <(node -e 'for(const x of require(process.argv[1]).check_matrix) console.log(x.exit_code,x.actor,x.expected_block)' "$root/tests/classification-fixtures.json")

# Invalid credentials (stale binding, absent triggered declaration, and digest tampering) fail closed.
for fault in stale missing_declaration tampered_digest; do
  report="$tmp/report-0-agent.json"
  export REPORT_FACTORY="$tmp/report.js" MOCK_CODE=0 MOCK_REPORT="$report"
  "$tmp/mock-sker" check --ci --sarif "$tmp/x.sarif" --json "$report" || true
  credential="$tmp/bad-$fault.json"
  node - "$report" "$credential" "$fault" <<'NODE'
const fs=require('node:fs');const r=JSON.parse(fs.readFileSync(process.argv[2]));const fault=process.argv[4];
const c={schema_version:'sker.agent-credential/v1',bundle_sha256:r.bundle_sha256,diff_tree_hash:r.diff_tree_hash,consult_trace:[],semantic_verdicts:[{rule_id:'R-SEM',state:'comply',declaration:'Reviewed'}],issuer:'local_agent',created_at:'2026-11-18T00:00:00Z',credential_blake3:'c'.repeat(64)};
if(fault==='stale')c.diff_tree_hash='d'.repeat(64);
if(fault==='missing_declaration')c.semantic_verdicts=[];
if(fault==='tampered_digest')c.credential_blake3='d'.repeat(64);
fs.writeFileSync(process.argv[3],JSON.stringify(c));
NODE
  export SKER_ACTOR=agent SKER_EXIT_CODE=0 INPUT_CREDENTIAL_FILE="$credential" INPUT_REPORT_FILE="$report" TEST_DIGEST="$(printf 'c%.0s' {1..64})" PATH="$tmp/bin:$PATH"
  set +e; bash "$root/scripts/verify-pr.sh" >/dev/null 2>&1; actual=$?; set -e
  [[ $actual -ne 0 ]] || { echo "$fault credential unexpectedly passed" >&2; exit 1; }
done

# PR comment contains the idempotency marker, compliance matrix, source excerpt, and semantic card.
mkdir -p "$tmp/bin"
cat > "$tmp/bin/gh" <<'GH'
#!/usr/bin/env bash
if [[ "$*" == *"comments?per_page=100"* ]]; then printf '[]'; exit 0; fi
cat > "$CAPTURE_COMMENT"
printf '{"id":1}'
GH
chmod +x "$tmp/bin/gh"
node - "$tmp/comment-report.json" <<'NODE'
const fs=require('node:fs');fs.writeFileSync(process.argv[2],JSON.stringify({schema_version:'sker.check-report/v1',status:'passed',bundle_sha256:'a'.repeat(64),diff_tree_hash:'b'.repeat(64),changed_line_count:1,findings:[{rule_id:'R-SEM',path:'action/tests/sarif-2.1.0.json',start_line:3,end_line:3,track:'semantic',severity:'WARN',disposition:'pending_semantic',message:'Review unsafe semantic path',alternatives:[]}],analysis:{complete:true,unsupported_paths:[],warnings:[]}}));
NODE
export CAPTURE_COMMENT="$tmp/comment.json" GITHUB_REPOSITORY=owner/repo GITHUB_EVENT_PATH="$tmp/event-comment.json" GH_TOKEN=fake PATH="$tmp/bin:$PATH" INPUT_REPORT_FILE="$tmp/comment-report.json"
printf '{"pull_request":{"number":12}}' > "$GITHUB_EVENT_PATH"
bash "$root/scripts/report-pr.sh"
node - "$CAPTURE_COMMENT" <<'NODE'
const fs=require('node:fs');const p=JSON.parse(fs.readFileSync(process.argv[2]));const b=p.body;
for(const s of ['<!-- sker-compliance-matrix -->','代码位置','R-SEM','高危语义自查卡片','sarif-2.1.0.json']) if(!b.includes(s)) throw Error(`comment missing ${s}`);
NODE

# An unavailable Ledger is fail-open and cannot alter the command status.
INPUT_LEDGER_URL='http://127.0.0.1:1/api/v1/events' INPUT_REPORT_FILE="$tmp/comment-report.json" bash "$root/scripts/post-ledger-event.sh"

# Human-only declarations do not suppress an Agent marker or credential evidence.
for case in label credential human; do
  ev="$tmp/event.json"; cred="$tmp/classify-credential"
  labels='[]'; body='Sker-Actor: human-only'; rm -f "$cred"
  if [[ "$case" == label ]]; then labels='[{"name":"agent-authored"}]'; fi
  if [[ "$case" == credential ]]; then touch "$cred"; fi
  if [[ "$case" == human ]]; then labels='[]'; fi
  node - "$ev" "$labels" "$body" <<'NODE'
const fs=require('node:fs');fs.writeFileSync(process.argv[2],JSON.stringify({pull_request:{labels:JSON.parse(process.argv[3]),body:process.argv[4]}}));
NODE
  export GH_EVENT_PATH="$ev" INPUT_CREDENTIAL_FILE="$cred" INPUT_AGENT_LABEL=agent-authored GITHUB_OUTPUT="$tmp/classify-out"
  bash "$root/scripts/classify-pr.sh"
  got="$(sed -n 's/^actor=//p' "$tmp/classify-out" | tail -1)"
  if [[ "$case" == human && "$got" != human ]]; then exit 1; fi
  if [[ "$case" != human && "$got" != agent ]]; then exit 1; fi
done

# Adversarial test: no agent-authored label, no credential, but commit has Co-authored-by: Codex ...
# Must classify as agent and then verify-pr.sh must block.
adv_repo="$tmp/adv-repo"
mkdir -p "$adv_repo"
git -C "$adv_repo" init -q -b main
git -C "$adv_repo" config user.email "test@example.com"
git -C "$adv_repo" config user.name "Tester"
echo "hello" > "$adv_repo/file.txt"
git -C "$adv_repo" add .
git -C "$adv_repo" commit -qm "feat: some changes

Co-authored-by: Codex <codex@openai.com>"

ev_adv="$tmp/event-adv.json"
node - "$ev_adv" <<'NODE'
const fs=require('node:fs');
fs.writeFileSync(process.argv[2], JSON.stringify({pull_request:{labels:[],body:''}}));
NODE

(
  cd "$adv_repo"
  export GH_EVENT_PATH="$ev_adv" INPUT_CREDENTIAL_FILE="$adv_repo/.sker/credential.json" INPUT_AGENT_LABEL=agent-authored GITHUB_OUTPUT="$tmp/classify-adv-out"
  bash "$root/scripts/classify-pr.sh"
)
adv_actor="$(sed -n 's/^actor=//p' "$tmp/classify-adv-out" | tail -1)"
[[ "$adv_actor" == "agent" ]] || { echo "Adversarial PR with Co-authored-by was not classified as agent (got $adv_actor)" >&2; exit 1; }

# verify-pr.sh without credential must block for this classified agent PR
set +e
SKER_ACTOR="$adv_actor" SKER_EXIT_CODE=0 INPUT_CREDENTIAL_FILE="$adv_repo/.sker/credential.json" INPUT_REPORT_FILE="$tmp/comment-report.json" bash "$root/scripts/verify-pr.sh" >/dev/null 2>&1
adv_verify_code=$?
set -e
[[ "$adv_verify_code" -ne 0 ]] || { echo "Adversarial Agent PR unexpectedly passed verify-pr.sh without credential" >&2; exit 1; }

echo "Action syntax and 0/1/2 × Agent/human gate matrix passed."
