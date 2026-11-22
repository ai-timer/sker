#!/usr/bin/env bash
set -uo pipefail
sker="${INPUT_SKER_PATH:-sker}"
sarif="${INPUT_SARIF_FILE:-sker-results.sarif}"
report="${INPUT_REPORT_FILE:-sker-check-report.json}"
mkdir -p "$(dirname "$sarif")" "$(dirname "$report")"
# `.sker/bundle.*` is gitignored, so CI checkouts must rebuild the bundle first.
set +e
"$sker" compile
compile_code=$?
set -e
if [[ "$compile_code" -ne 0 ]]; then
  # Without a bundle the gate cannot analyze; report incomplete (exit 2 policy).
  printf 'exit_code=2\n' >> "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
  exit 0
fi
set +e
"$sker" check --ci --sarif "$sarif" --json "$report"
code=$?
set -e
if [[ "$code" != 0 && "$code" != 1 && "$code" != 2 ]]; then code=2; fi
printf 'exit_code=%s\n' "$code" >> "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
# Exit codes are consumed by the verifier, which applies actor-specific blocking policy.
exit 0
