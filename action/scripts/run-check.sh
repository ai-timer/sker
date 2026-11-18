#!/usr/bin/env bash
set -uo pipefail
sker="${INPUT_SKER_PATH:-sker}"
sarif="${INPUT_SARIF_FILE:-sker-results.sarif}"
report="${INPUT_REPORT_FILE:-sker-check-report.json}"
mkdir -p "$(dirname "$sarif")" "$(dirname "$report")"
set +e
"$sker" check --ci --sarif "$sarif" --json "$report"
code=$?
set -e
if [[ "$code" != 0 && "$code" != 1 && "$code" != 2 ]]; then code=2; fi
printf 'exit_code=%s\n' "$code" >> "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
# Exit codes are consumed by the verifier, which applies actor-specific blocking policy.
exit 0
