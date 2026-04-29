-- Migration: Add performance indexes for frequently-executed query patterns
-- Safe to run multiple times (IF NOT EXISTS).

-- Composite index on rewards(user_address, campaign_id) for double-claim check.
-- The existing idx_rewards_user covers user_address alone but cannot satisfy
-- the two-column predicate efficiently.
CREATE INDEX IF NOT EXISTS idx_rewards_user_campaign ON rewards(user_address, campaign_id);

-- Index on campaigns.expiration for expiry filtering
-- (e.g. WHERE expiration < now_unix OR expiration BETWEEN $1 AND $2).
CREATE INDEX IF NOT EXISTS idx_campaigns_expiration ON campaigns(expiration);

-- Partial index on campaigns.active — only indexes rows where active = TRUE,
-- keeping the index compact and the scan fast.
CREATE INDEX IF NOT EXISTS idx_campaigns_active ON campaigns(active) WHERE active = TRUE;

-- Index on transactions(campaign_id) for campaign-scoped transaction lookups.
-- The existing idx_transactions_user covers user_address but not campaign_id.
CREATE INDEX IF NOT EXISTS idx_transactions_campaign ON transactions(campaign_id);

-- Partial index on rewards.redeemed — only indexes unredeemed rows (redeemed = FALSE),
-- which are the rows almost always queried.
CREATE INDEX IF NOT EXISTS idx_rewards_redeemed ON rewards(redeemed) WHERE redeemed = FALSE;
