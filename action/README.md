# Sker GitHub Action

Recompute Sker architectural and negative knowledge policy checks in CI, publish SARIF reports, and render idempotent PR compliance matrices.

## Quickstart

```yaml
name: Sker Governance Gate

on:
  pull_request:
    branches: [main]

permissions:
  contents: read
  pull-requests: write # Required for idempotent compliance matrix comments
  issues: write        # Required for PR comments via GitHub Issues API
  security-events: write # Required for SARIF Code Scanning upload

jobs:
  sker-gate:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code
        uses: actions/checkout@v4
        with:
          fetch-depth: 0 # Full git history required for accurate diff analysis

      - name: Install Sker CLI
        run: |
          curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ai-timer/sker/releases/download/v0.1.0-preview/sker-cli-installer.sh | sh
          echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"

      - name: Run Sker Governance Gate
        uses: ai-timer/sker/action@v0.1.0-preview
        with:
          sker-path: sker
          sarif-file: sker-results.sarif
          report-file: sker-check-report.json
          protected-agent-label: agent-authored
          # Optional Ledger telemetry endpoint:
          # ledger-url: ${{ secrets.SKER_LEDGER_URL }}
          # ledger-token: ${{ secrets.SKER_LEDGER_TOKEN }}
```

## How It Works

1. **PR Classification (`classify-pr.sh`)**: Inspects PR labels, commits, and local credentials (`.sker/credential.json`). If the PR contains `agent-authored` label or a credential file, it is classified as an Agent PR.
2. **Authoritative CI Check (`run-check.sh`)**: Invokes:
   ```text
   sker check --ci --sarif <sarif-file> --json <report-file>
   ```
3. **Agent Credential & Integrity Gate (`verify-pr.sh`)**:
   - For Agent PRs: verifies that `.sker/credential.json` binds the exact git diff BLAKE3 digest and active playbook bundle digest, validates semantic attestations, and enforces exit code rules.
   - For Human PRs: deterministic exit code 1 blocks; non-deterministic findings (code 2) fail-open.
4. **SARIF Upload**: Uploads findings to GitHub Advanced Security Code Scanning.
5. **PR Compliance Matrix (`report-pr.sh`)**: Comments an idempotent compliance matrix and self-check card onto the pull request.
6. **Telemetry Ingestion (`post-ledger-event.sh`)**: (Optional) Sends sanitized telemetry to Sker Ledger (fail-open, asynchronous).

## Action Inputs

| Input | Description | Default |
| --- | --- | --- |
| `sker-path` | Path to the installed `sker` executable | `sker` |
| `sarif-file` | Path to generated SARIF output file | `sker-results.sarif` |
| `report-file` | Path to generated CheckReport JSON output file | `sker-check-report.json` |
| `credential-file` | Path to locally issued Agent credential | `.sker/credential.json` |
| `protected-agent-label` | Protected PR label indicating Agent participation | `agent-authored` |
| `ledger-url` | Optional Sker Ledger event ingestion endpoint | `''` |
| `ledger-token` | Optional Sker Ledger bearer token | `''` |
