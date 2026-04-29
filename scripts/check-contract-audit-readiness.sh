#!/usr/bin/env bash

set -euo pipefail

STATUS_FILE="docs/audits/smart-contract-audit-status.yml"

if [[ ! -f "$STATUS_FILE" ]]; then
  echo "Missing audit status file: $STATUS_FILE"
  exit 1
fi

require_line() {
  local expected="$1"
  if ! grep -Eq "^[[:space:]]*${expected}[[:space:]]*$" "$STATUS_FILE"; then
    echo "Audit readiness check failed: expected '$expected' in $STATUS_FILE"
    exit 1
  fi
}

require_line "scope_defined: true"
require_line "auditor_engaged: true"
require_line "critical_open: 0"
require_line "high_open: 0"
require_line "medium_findings_dispositioned: true"
require_line "public_report_published: true"
require_line "significant_changes_since_last_audit: false"
require_line "re_audit_required_after_significant_changes: true"

echo "Smart contract audit readiness check passed."
