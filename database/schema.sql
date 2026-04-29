-- SorobanLoyalty PostgreSQL Schema

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE IF NOT EXISTS users (
    address         VARCHAR(56) PRIMARY KEY,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS campaigns (
    id              BIGINT PRIMARY KEY,
    merchant        VARCHAR(56) NOT NULL,
    name            VARCHAR(255),
    reward_amount   BIGINT NOT NULL,
    expiration      BIGINT NOT NULL,   -- unix timestamp
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    total_claimed   BIGINT NOT NULL DEFAULT 0,
    display_order   INT NOT NULL DEFAULT 0,
    tx_hash         VARCHAR(64),
    image_url       TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS rewards (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_address    VARCHAR(56) NOT NULL REFERENCES users(address) ON DELETE CASCADE,
    campaign_id     BIGINT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    amount          BIGINT NOT NULL,
    redeemed        BOOLEAN NOT NULL DEFAULT FALSE,
    redeemed_amount BIGINT NOT NULL DEFAULT 0,
    claimed_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    redeemed_at     TIMESTAMPTZ,
    UNIQUE (user_address, campaign_id)
);

CREATE TABLE IF NOT EXISTS transactions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tx_hash         VARCHAR(64) NOT NULL UNIQUE,
    type            VARCHAR(32) NOT NULL,  -- 'claim' | 'redeem' | 'transfer'
    user_address    VARCHAR(56),
    campaign_id     BIGINT,
    amount          BIGINT,
    ledger          BIGINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rewards_user ON rewards(user_address);
CREATE INDEX IF NOT EXISTS idx_rewards_user_claimed_at ON rewards(user_address, claimed_at DESC);
CREATE INDEX IF NOT EXISTS idx_rewards_campaign ON rewards(campaign_id);
CREATE INDEX IF NOT EXISTS idx_transactions_user ON transactions(user_address);
CREATE INDEX IF NOT EXISTS idx_campaigns_merchant ON campaigns(merchant);
CREATE INDEX IF NOT EXISTS idx_campaigns_name ON campaigns(name);
CREATE INDEX IF NOT EXISTS idx_campaigns_active ON campaigns(active);
CREATE INDEX IF NOT EXISTS idx_campaigns_expiration ON campaigns(expiration);
