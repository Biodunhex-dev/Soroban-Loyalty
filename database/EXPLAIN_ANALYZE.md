# EXPLAIN ANALYZE — Performance Index Impact

This document shows the expected query plans for the three most impactful queries before and after applying migration `002_add_performance_indexes.sql`. Use these as a baseline when verifying the migration on a real database.

---

## 1. Double-Claim Check

**Query**
```sql
SELECT id
FROM   rewards
WHERE  user_address = 'GABC123...'
AND    campaign_id  = 42;
```

This query is executed on every claim attempt to prevent a user from claiming the same campaign twice.

### Before (no `idx_rewards_user_campaign`)

```
Seq Scan on rewards  (cost=0.00..18450.00 rows=1 width=16)
  Filter: ((user_address = 'GABC123...'::character varying)
           AND (campaign_id = 42))
Planning Time: 0.3 ms
Execution Time: 42.7 ms   -- grows linearly with table size
```

PostgreSQL reads every row in `rewards` and applies both filter predicates. At 500 k rows the scan takes ~40 ms; at 5 M rows it takes ~400 ms.

### After (`idx_rewards_user_campaign` on `rewards(user_address, campaign_id)`)

```
Index Scan using idx_rewards_user_campaign on rewards
                 (cost=0.43..8.45 rows=1 width=16)
  Index Cond: ((user_address = 'GABC123...'::character varying)
               AND (campaign_id = 42))
Planning Time: 0.2 ms
Execution Time: 0.1 ms   -- O(log n), independent of table size
```

PostgreSQL traverses the composite B-tree index directly to the matching row. Execution time is sub-millisecond regardless of table size.

**Why this index**: The existing `idx_rewards_user` covers `user_address` alone. PostgreSQL can use it to narrow the scan to one user's rows, but it still has to filter by `campaign_id` in memory. The composite index satisfies both predicates at the index level.

---

## 2. Expiry Filter

**Query**
```sql
SELECT id, merchant, reward_amount, expiration
FROM   campaigns
WHERE  expiration < EXTRACT(EPOCH FROM NOW())::BIGINT;
```

This query is executed when the indexer or a background job needs to identify expired campaigns.

### Before (no `idx_campaigns_expiration`)

```
Seq Scan on campaigns  (cost=0.00..2340.00 rows=120 width=48)
  Filter: (expiration < 1700000000)
Planning Time: 0.2 ms
Execution Time: 8.3 ms   -- grows linearly with table size
```

PostgreSQL reads every campaign row to evaluate the `expiration` predicate.

### After (`idx_campaigns_expiration` on `campaigns(expiration)`)

```
Index Scan using idx_campaigns_expiration on campaigns
              (cost=0.29..12.50 rows=120 width=48)
  Index Cond: (expiration < 1700000000)
Planning Time: 0.2 ms
Execution Time: 0.4 ms
```

PostgreSQL uses the B-tree index to jump directly to the range of rows where `expiration < threshold`. For range queries (`BETWEEN`, `<`, `>`) a B-tree index is the correct choice.

---

## 3. Active Campaigns Filter

**Query**
```sql
SELECT id, merchant, reward_amount, expiration
FROM   campaigns
WHERE  active = TRUE;
```

This query is executed on every page load of the campaign listing and on every claim eligibility check.

### Before (no `idx_campaigns_active`)

```
Seq Scan on campaigns  (cost=0.00..2340.00 rows=85 width=48)
  Filter: (active = TRUE)
Planning Time: 0.2 ms
Execution Time: 7.9 ms
```

PostgreSQL reads every campaign row. A plain boolean index on `active` would be low-selectivity (only two values) and often ignored by the planner.

### After (partial index `idx_campaigns_active` on `campaigns(active) WHERE active = TRUE`)

```
Index Scan using idx_campaigns_active on campaigns
              (cost=0.15..6.20 rows=85 width=48)
Planning Time: 0.2 ms
Execution Time: 0.2 ms
```

The partial index contains only the rows where `active = TRUE`, so it is small and highly selective. PostgreSQL reads the index directly without touching inactive rows at all.

**Why a partial index**: A full index on a boolean column has poor selectivity — the planner may ignore it. A partial index with `WHERE active = TRUE` is compact (only active campaigns are indexed) and unambiguously useful for this predicate.

---

## Summary

| Query | Before | After | Index Used |
|---|---|---|---|
| Double-claim check | Seq Scan ~40 ms | Index Scan ~0.1 ms | `idx_rewards_user_campaign` |
| Expiry filter | Seq Scan ~8 ms | Index Scan ~0.4 ms | `idx_campaigns_expiration` |
| Active campaigns | Seq Scan ~8 ms | Index Scan ~0.2 ms | `idx_campaigns_active` (partial) |
| Campaign transactions | Seq Scan | Index Scan | `idx_transactions_campaign` |
| Unredeemed rewards | Seq Scan | Index Scan | `idx_rewards_redeemed` (partial) |

> **Note**: Execution times above are illustrative estimates based on typical table sizes (campaigns ~10 k rows, rewards ~500 k rows, transactions ~1 M rows). Actual times will vary with hardware, PostgreSQL version, and data distribution. Run `EXPLAIN (ANALYZE, BUFFERS)` on your target database to get real numbers.

---

## How to Verify

```sql
-- 1. Apply the migration
\i database/migrations/002_add_performance_indexes.sql

-- 2. Confirm indexes exist
SELECT indexname, indexdef
FROM   pg_indexes
WHERE  tablename IN ('rewards', 'campaigns', 'transactions')
ORDER  BY tablename, indexname;

-- 3. Run EXPLAIN ANALYZE for each query
EXPLAIN (ANALYZE, BUFFERS)
SELECT id FROM rewards
WHERE  user_address = 'GABC123...' AND campaign_id = 42;

EXPLAIN (ANALYZE, BUFFERS)
SELECT id FROM campaigns
WHERE  expiration < EXTRACT(EPOCH FROM NOW())::BIGINT;

EXPLAIN (ANALYZE, BUFFERS)
SELECT id FROM campaigns
WHERE  active = TRUE;
```

Look for `Index Scan using idx_rewards_user_campaign`, `idx_campaigns_expiration`, and `idx_campaigns_active` respectively in the output.
