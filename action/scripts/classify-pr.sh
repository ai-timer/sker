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

# Check Git commit logs / trailers for Agent co-authorship or Sker-Agent trailers
if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  # In shallow clones, attempt to fetch recent commit history if possible
  if git rev-parse --is-shallow-repository 2>/dev/null | grep -q true; then
    if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
      git fetch origin "$GITHUB_BASE_REF" --depth=50 --no-tags 2>/dev/null || true
    fi
    git fetch --depth=50 --no-tags 2>/dev/null || true
  fi
  commit_trailers="$(git log -n 50 --format="%B" 2>/dev/null || git log -1 --format="%B" 2>/dev/null || true)"
  if [[ -n "$commit_trailers" ]]; then
    if echo "$commit_trailers" | grep -Ei "(Co-authored-by|Sker-Agent):.*(claude|codex|copilot|antigravity|cursor|gemini|openai|anthropic|sker-agent)" >/dev/null; then
      agent_evidence=true
      reason=commit_trailer
    fi
  fi
fi

# Also check commits array in event_path if present (e.g. push event)
if [[ "$agent_evidence" == false && -n "$event_path" && -f "$event_path" ]]; then
  if node - "$event_path" <<'NODE'
const fs = require('node:fs');
const e = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const commits = e.commits || [];
const pattern = /(?:co-authored-by|sker-agent):.*(?:claude|codex|copilot|antigravity|cursor|gemini|openai|anthropic|sker-agent)/i;
const matched = commits.some(c => pattern.test(c.message || ''));
process.exit(matched ? 0 : 1);
NODE
  then agent_evidence=true; reason=commit_trailer; fi
fi

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
