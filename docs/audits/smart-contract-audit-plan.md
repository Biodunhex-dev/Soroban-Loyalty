# Smart Contract Audit Plan

## Objective

The `rewards` contract is the highest-risk contract in the SorobanLoyalty system because it is the sole pathway for token minting and burning. A third-party audit is required before any mainnet deployment of the contract set.

This audit program covers:

- [`contracts/token/src/lib.rs`](/c:/Users/HP/Desktop/drips-2/Soroban-Loyalty/contracts/token/src/lib.rs)
- [`contracts/campaign/src/lib.rs`](/c:/Users/HP/Desktop/drips-2/Soroban-Loyalty/contracts/campaign/src/lib.rs)
- [`contracts/rewards/src/lib.rs`](/c:/Users/HP/Desktop/drips-2/Soroban-Loyalty/contracts/rewards/src/lib.rs)

## Scope

### Contracts in scope

| Contract | Risk focus |
|---|---|
| `token` | Mint authority, burn invariants, transfer and allowance correctness, supply accounting |
| `campaign` | Authorization boundaries, campaign lifecycle rules, metadata and counter integrity |
| `rewards` | Claim/redeem flow, cross-contract call ordering, reentrancy resistance, mint/burn safety |

### Security themes

The auditor must explicitly evaluate:

- Reentrancy and cross-contract call ordering
- Authorization bypass and privilege escalation
- Integer overflow, underflow, and supply/accounting invariants
- Logic errors in claim, redeem, expiry, and campaign activation flows
- Storage safety and one-time initialization assumptions
- Upgrade and admin-rotation safety
- Event correctness for downstream indexers and analytics
- Failure-mode handling around partial execution and panics

### Assets and trust boundaries to review

- Token supply and per-user balances
- Rewards contract mint authority
- Campaign activation and expiration state
- Claim uniqueness per user per campaign
- Admin and upgrade controls
- Cross-contract assumptions between token, campaign, and rewards

## Third-Party Audit Requirements

- Auditor must be independent from the implementation team.
- Preferred firms include `OtterSec`, `Halborn`, or another comparable smart-contract security specialist.
- The engagement must include:
  - Manual line-by-line review
  - Architecture and threat-model review
  - Reproduction steps and proof of concept for findings where applicable
  - Severity ratings for all findings
  - Final public report suitable for publication in this repository

## Deliverables

The audit engagement is not complete until all of the following exist:

1. Defined scope covering all three contracts.
2. Signed or otherwise confirmed third-party engagement.
3. Initial report with findings.
4. Remediation PRs linked to findings.
5. Final remediation summary.
6. Public report committed under `docs/audits/reports/`.
7. Re-audit or written addendum after significant contract changes.

## Remediation Policy

### Blocking findings

- `Critical`: must be remediated before mainnet.
- `High`: must be remediated before mainnet.

### Non-blocking with explicit sign-off

- `Medium`: must either be remediated or formally accepted with rationale, owner, and date.
- `Low` and `Informational`: tracked for backlog and best-effort remediation.

## Mainnet Release Gate

Mainnet deployment of the contracts is blocked unless all of the following are true:

- `scope_defined: true`
- `auditor_engaged: true`
- `critical_open: 0`
- `high_open: 0`
- `medium_findings_dispositioned: true`
- `public_report_published: true`
- `significant_changes_since_last_audit: false`

These conditions are enforced by [`scripts/check-contract-audit-readiness.sh`](/c:/Users/HP/Desktop/drips-2/Soroban-Loyalty/scripts/check-contract-audit-readiness.sh) and the contract security workflow.

## Re-Audit Triggers

A re-audit is required after any significant contract change, including:

- Changes to mint, burn, claim, redeem, or transfer logic
- Storage layout changes
- New admin or upgrade flows
- New cross-contract interactions
- Any change to authorization rules or trusted roles
- Compiler, SDK, or dependency upgrades that alter contract behavior in material ways

## Engagement Checklist

- [ ] Scope shared with auditor
- [ ] All three contracts included
- [ ] Threat model shared
- [ ] Test suite and deployment scripts shared
- [ ] Findings triaged by severity
- [ ] Critical findings closed
- [ ] High findings closed
- [ ] Medium findings closed or accepted with rationale
- [ ] Final report published
- [ ] Re-audit status reviewed after latest contract diff
