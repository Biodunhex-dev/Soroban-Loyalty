# Storage Migration Guide

## Overview

When a Soroban contract upgrade changes the storage schema (adds fields, changes types, renames keys), existing on-chain data must be migrated to the new format. This guide documents the migration pattern used in SorobanLoyalty contracts and the procedure for executing and rolling back migrations.

## Pattern

### Key Principles

1. **Idempotency guard** — a `MigrationVNDone` flag in instance storage prevents the migration from running twice
2. **Admin-only** — migration requires admin auth; unauthorized callers are rejected
3. **Old key cleanup** — after writing new-format records, old keys are removed
4. **Schema version** — `SchemaVersion` in instance storage tracks the current version
5. **Backward-compatible reads** — during the migration window, reads check both old and new keys so the contract remains functional before migration runs

### Storage Key Versioning

```rust
#[contracttype]
pub enum DataKey {
    // v2 (current)
    Claimed(Address, u64),          // → ClaimRecord { amount, claimed_at }

    // v1 (legacy — present only before migration completes)
    ClaimedV1(Address, u64),        // → bool

    // Migration state
    SchemaVersion,                  // → u32
    MigrationV1Done,                // → bool
}
```

### Migration Function Signature

```rust
pub fn migrate_v1_to_v2(
    env: Env,
    admin: Address,
    entries: Vec<(Address, u64)>,   // (user, campaign_id) pairs to migrate
)
```

### Implementation

```rust
pub fn migrate_v1_to_v2(env: Env, admin: Address, entries: Vec<(Address, u64)>) {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    assert!(admin == stored_admin, "not admin");

    // Idempotency guard
    assert!(
        !env.storage().instance().has(&DataKey::MigrationV1Done),
        "migration already completed"
    );

    let now = env.ledger().timestamp();

    for (user, campaign_id) in entries.iter() {
        let v1_key = DataKey::ClaimedV1(user.clone(), campaign_id);
        let v2_key = DataKey::Claimed(user.clone(), campaign_id);

        if env.storage().persistent().has(&v1_key)
            && !env.storage().persistent().has(&v2_key)
        {
            let record = ClaimRecord { amount: 0, claimed_at: now };
            env.storage().persistent().set(&v2_key, &record);
            env.storage().persistent().remove(&v1_key);
        }
    }

    env.storage().instance().set(&DataKey::MigrationV1Done, &true);
    env.storage().instance().set(&DataKey::SchemaVersion, &2_u32);
    env.events().publish(MIGRATED, 2_u32);
}
```

### Backward-Compatible Read

During the migration window (after upgrade, before migration runs), reads check both keys:

```rust
fn has_claimed(env: &Env, user: &Address, campaign_id: u64) -> bool {
    env.storage().persistent().has(&DataKey::Claimed(user.clone(), campaign_id))
        || env.storage().persistent().has(&DataKey::ClaimedV1(user.clone(), campaign_id))
}
```

## Execution Procedure

### Prerequisites

1. Deploy the upgraded contract wasm (containing both old and new key support)
2. Collect all `(user, campaign_id)` pairs that were claimed under v1 — query the `transactions` table:
   ```sql
   SELECT DISTINCT user_address, campaign_id
   FROM transactions
   WHERE type = 'claim'
   ORDER BY user_address;
   ```

### Step-by-Step

1. **Deploy upgraded contract**
   ```bash
   ./scripts/deploy-contracts.sh testnet $SECRET_KEY
   ```

2. **Verify schema version is still 1**
   ```bash
   stellar contract invoke \
     --id $REWARDS_CONTRACT_ID \
     --source $ADMIN_KEY \
     -- schema_version
   # Expected: 1
   ```

3. **Prepare migration entries** (batch into chunks of ≤100 to stay within gas limits)
   ```bash
   # Query DB for all claimed entries
   psql $DATABASE_URL -c "
     SELECT user_address, campaign_id
     FROM transactions WHERE type = 'claim'
   " --csv > claimed_entries.csv
   ```

4. **Run migration** (repeat for each batch)
   ```bash
   stellar contract invoke \
     --id $REWARDS_CONTRACT_ID \
     --source $ADMIN_KEY \
     -- migrate_v1_to_v2 \
     --admin $ADMIN_ADDRESS \
     --entries '[["USER_ADDR_1", 1], ["USER_ADDR_2", 2]]'
   ```

5. **Verify migration completed**
   ```bash
   stellar contract invoke \
     --id $REWARDS_CONTRACT_ID \
     --source $ADMIN_KEY \
     -- schema_version
   # Expected: 2
   ```

6. **Verify no v1 keys remain** (spot-check a few users)
   ```bash
   stellar contract invoke \
     --id $REWARDS_CONTRACT_ID \
     --source $ADMIN_KEY \
     -- has_claimed_view \
     --user $SAMPLE_USER \
     --campaign_id 1
   # Expected: true (served from v2 key)
   ```

## Rollback Procedure

The migration is designed to be safe to retry but not automatically reversible. If migration fails mid-batch:

### Scenario A: Migration panicked before setting `MigrationV1Done`

The idempotency flag was not set, so the migration can be retried from the beginning. The v1 keys for already-migrated entries are gone, but `has_claimed` still returns `true` via the v2 key.

**Action**: Fix the root cause and re-run `migrate_v1_to_v2` with the full entry list.

### Scenario B: Need to roll back to v1 contract

If the v2 contract has a critical bug and must be rolled back:

1. **Redeploy v1 wasm** (requires the upgrade multi-sig flow)
   ```bash
   stellar contract invoke \
     --id $CAMPAIGN_CONTRACT_ID \
     --source $ADMIN_KEY \
     -- propose_upgrade \
     --admin $ADMIN_ADDRESS \
     --wasm_hash $V1_WASM_HASH
   # ... approve and execute upgrade
   ```

2. **Restore v1 claim records** from the `transactions` table:
   ```bash
   # For each claimed entry, write ClaimedV1 key back
   # (requires a one-off recovery script using the Soroban SDK)
   ```

> **Note**: Rolling back after a completed migration requires a recovery script to re-write `ClaimedV1` keys. Always test migrations on testnet before mainnet.

## Testing

### Unit Tests

```bash
cargo test -p soroban-loyalty-rewards -- migration
```

Tests cover:
- `test_migration_v1_to_v2_idempotent_guard` — second call panics
- `test_migration_converts_v1_records` — v1 keys removed, v2 keys written, `has_claimed` still true
- `test_migration_non_admin_rejected` — non-admin panics

### Realistic Data Volume

The migration function accepts a `Vec<(Address, u64)>` batch. For large datasets, call in batches of 50–100 entries per transaction to stay within Soroban's instruction limits (~100M instructions per transaction).

Estimate batch size:
- Each entry: ~2 storage reads + 1 write + 1 remove ≈ ~500K instructions
- Safe batch: 100 entries ≈ 50M instructions (well within 100M limit)

## Adding Future Migrations

Follow this checklist for each new migration:

- [ ] Add new `DataKey` variants for new format (keep old variants with `V{N}` suffix)
- [ ] Add `MigrationV{N}Done` idempotency key
- [ ] Implement `migrate_v{N}_to_v{N+1}` with admin auth + idempotency guard
- [ ] Update reads to check both old and new keys during migration window
- [ ] Write tests: idempotency, data conversion, admin-only, old key cleanup
- [ ] Document rollback procedure
- [ ] Test on testnet with realistic data volume before mainnet
