# Smart Contract Audits

This directory contains the public artifacts and operational checklists for third-party audits of the SorobanLoyalty smart contracts.

## Files

- `smart-contract-audit-plan.md` - Audit scope, deliverables, and remediation policy for the token, campaign, and rewards contracts.
- `smart-contract-audit-status.yml` - Machine-readable launch gate status consumed by CI and deployment scripts.
- `reports/` - Publicly published audit reports and remediation notes.

## Operating Rules

1. Every mainnet-bound audit must cover all three contracts in `contracts/`.
2. Audit work must be performed by an independent third party.
3. Critical and high findings must be fixed before mainnet deployment.
4. Medium findings must either be fixed or explicitly accepted with written rationale.
5. The final report and remediation summary must be published in `reports/`.
6. Any significant contract change after an audit must set `significant_changes_since_last_audit: true` in `smart-contract-audit-status.yml` until a re-audit is completed.
