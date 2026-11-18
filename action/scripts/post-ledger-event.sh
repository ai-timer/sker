#!/usr/bin/env bash
# Telemetry is deliberately detached and never changes the CI result.
set +e
url="${INPUT_LEDGER_URL:-}"
report="${INPUT_REPORT_FILE:-sker-check-report.json}"
[[ -n "$url" && -f "$report" ]] || exit 0
body="$(node - "$report" <<'NODE'
const fs = require('node:fs');
const crypto = require('node:crypto');
const r = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const outcome = r.status === 'blocked' ? 'block' : r.status === 'passed' ? 'pass' : 'no_data';
const event = {
  event_id: crypto.randomUUID(), occurred_at: new Date().toISOString(), event_type: 'ci_gate',
  bundle_sha256: r.bundle_sha256,
  counters: { changed_line_count: r.changed_line_count },
  labels: { repository_class: 'unknown', language: 'unknown', outcome }
};
process.stdout.write(JSON.stringify({schema_version:'sker.ledger-events/v1',events:[event]}));
NODE
)"
headers=(-H 'Content-Type: application/json')
[[ -z "${INPUT_LEDGER_TOKEN:-}" ]] || headers+=(-H "Authorization: Bearer ${INPUT_LEDGER_TOKEN}")
# Backgrounded with a short timeout; command failure and outage are ignored.
(curl --silent --show-error --max-time 2 -X POST "$url" "${headers[@]}" --data-binary "$body" >/dev/null 2>&1) >/dev/null 2>&1 &
exit 0
