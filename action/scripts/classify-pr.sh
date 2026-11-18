#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
credential="${INPUT_CREDENTIAL_FILE:-.sker/credential.json}"
agent_label="${INPUT_AGENT_LABEL:-agent-authored}"
event_path="${GH_EVENT_PATH:-${GITHUB_EVENT_PATH:-}}"
agent_evidence=false
reason=human

if [[ -n "$event_path" && -f "$event_path" ]]; then
  if node - "$event_path" "$agent_label" <<'NODE'
const fs = require('node:fs');
const e = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const wanted = process.argv[3];
const labels = (e.pull_request?.labels || []).map(x => x.name);
process.exit(labels.includes(wanted) ? 0 : 1);
NODE
  then agent_evidence=true; reason=protected_label; fi
fi
# A credential is evidence even if it is stale or malformed; verification then fails closed.
if [[ -f "$credential" ]]; then agent_evidence=true; reason=credential_file; fi

actor=human
if [[ "$agent_evidence" == true ]]; then actor=agent; fi
# The declaration is informational only. It can never suppress independently observed evidence.
declaration=unspecified
if [[ -n "$event_path" && -f "$event_path" ]]; then
  declaration="$(node - "$event_path" <<'NODE'
const fs = require('node:fs');
const e = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const body = e.pull_request?.body || '';
const m = body.match(/^Sker-Actor:\s*(human-only|agent)\s*$/im);
process.stdout.write(m ? m[1] : 'unspecified');
NODE
)"
fi
if [[ "$declaration" == human-only && "$agent_evidence" == true ]]; then reason=agent_evidence_overrides_human_declaration; fi
{
  printf 'actor=%s\n' "$actor"
  printf 'agent_evidence=%s\n' "$agent_evidence"
  printf 'agent_evidence_reason=%s\n' "$reason"
  printf 'declaration=%s\n' "$declaration"
} >> "$GITHUB_OUTPUT"
