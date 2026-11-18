#!/usr/bin/env bash
set -euo pipefail
report="${INPUT_REPORT_FILE:-sker-check-report.json}"
[[ -f "$report" ]] || exit 0
[[ -n "${GITHUB_REPOSITORY:-}" && -n "${GITHUB_EVENT_PATH:-}" && -n "${GH_TOKEN:-}" ]] || exit 0
payload="$(node "$(dirname "${BASH_SOURCE[0]}")/render-comment.js" "$report")"
pr="$(node -p 'String(require(process.argv[1]).pull_request?.number || "")' "$GITHUB_EVENT_PATH")"
[[ -n "$pr" ]] || exit 0
repo="$GITHUB_REPOSITORY"
comments="$(gh api "repos/$repo/issues/$pr/comments?per_page=100")"
comment_id="$(COMMENTS_JSON="$comments" node -e 'const comments=JSON.parse(process.env.COMMENTS_JSON);const marker=String.fromCharCode(60,33,45,45)+" sker-compliance-matrix "+String.fromCharCode(45,45,62);const found=comments.find(comment=>(comment.body||"").includes(marker));if(found)process.stdout.write(String(found.id));')"
if [[ -n "$comment_id" ]]; then
  gh api "repos/$repo/issues/comments/$comment_id" -X PATCH --input - <<<"$payload" >/dev/null
else
  gh api "repos/$repo/issues/$pr/comments" -X POST --input - <<<"$payload" >/dev/null
fi
