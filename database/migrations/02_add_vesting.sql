-- Migration: Add vesting support for issue #128
-- Adds vesting_period_days to campaigns and a vesting_schedules table

ALTER TABLE campaigns
  ADD COLUMN IF NOT EXISTS vesting_period_days INT NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS vesting_schedules (
    id                   UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_address         VARCHAR(56) NOT NULL REFERENCES users(address) ON DELETE CASCADE,
    campaign_id          BIGINT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    total_amount         BIGINT NOT NULL,
    claimed_amount       BIGINT NOT NULL DEFAULT 0,
    start_time           BIGINT NOT NULL,   -- unix timestamp
    vesting_duration_secs BIGINT NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_address, campaign_id)
);

CREATE INDEX IF NOT EXISTS idx_vesting_user ON vesting_schedules(user_address);
