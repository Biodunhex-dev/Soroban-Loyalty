#!/usr/bin/env bash
# deploy-contracts.sh — Build and deploy all three Soroban contracts.
# Usage: ./scripts/deploy-contracts.sh [network] [admin-secret-key]
#
# network: local | testnet | mainnet (default: local)
# admin-secret-key: Stellar secret key (S...) for the deployer/admin account

set -euo pipefail

NETWORK="${1:-local}"
ADMIN_SECRET="${2:-}"

if [[ -z "$ADMIN_SECRET" ]]; then
  echo "Usage: $0 [local|testnet|mainnet] <ADMIN_SECRET_KEY>"
  exit 1
fi

case "$NETWORK" in
  local)
    RPC_URL="http://localhost:8000/soroban/rpc"
    PASSPHRASE="Standalone Network ; February 2017"
    ;;
  testnet)
    RPC_URL="https://soroban-testnet.stellar.org"
    PASSPHRASE="Test SDF Network ; September 2015"
    ;;
  mainnet)
    RPC_URL="https://mainnet.sorobanrpc.com"
    PASSPHRASE="Public Global Stellar Network ; September 2015"
    ;;
  *)
    echo "Unknown network: $NETWORK"
    exit 1
    ;;
esac

if [[ "$NETWORK" == "mainnet" ]]; then
  echo "==> Verifying third-party audit readiness gate for mainnet..."
  bash scripts/check-contract-audit-readiness.sh
fi

ADMIN_PUBLIC=$(stellar keys address "$ADMIN_SECRET" 2>/dev/null || \
  stellar keys generate --secret-key "$ADMIN_SECRET" --network "$NETWORK" 2>/dev/null || \
  echo "")

echo "==> Building contracts..."
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path contracts/token/Cargo.toml
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path contracts/campaign/Cargo.toml
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path contracts/rewards/Cargo.toml

echo "==> Deploying Token contract..."
TOKEN_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/soroban_loyalty_token.wasm \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE")
echo "TOKEN_CONTRACT_ID=$TOKEN_ID"

echo "==> Deploying Campaign contract..."
CAMPAIGN_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/soroban_loyalty_campaign.wasm \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE")
echo "CAMPAIGN_CONTRACT_ID=$CAMPAIGN_ID"

echo "==> Deploying Rewards contract..."
REWARDS_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/soroban_loyalty_rewards.wasm \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE")
echo "REWARDS_CONTRACT_ID=$REWARDS_ID"

echo "==> Initializing Token contract..."
stellar contract invoke \
  --id "$TOKEN_ID" \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE" \
  -- initialize \
  --admin "$REWARDS_ID" \
  --name "LoyaltyToken" \
  --symbol "LYT" \
  --decimals 7

echo "==> Initializing Campaign contract..."
stellar contract invoke \
  --id "$CAMPAIGN_ID" \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE" \
  -- initialize \
  --admins "[\"$REWARDS_ID\"]" \
  --threshold 1

echo "==> Initializing Rewards contract..."
stellar contract invoke \
  --id "$REWARDS_ID" \
  --source "$ADMIN_SECRET" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE" \
  -- initialize \
  --admin "$REWARDS_ID" \
  --token_contract "$TOKEN_ID" \
  --campaign_contract "$CAMPAIGN_ID"

echo ""
echo "==> Deployment complete. Add these to your .env:"
echo "TOKEN_CONTRACT_ID=$TOKEN_ID"
echo "CAMPAIGN_CONTRACT_ID=$CAMPAIGN_ID"
echo "REWARDS_CONTRACT_ID=$REWARDS_ID"

# Optionally write to .env
if [[ -f .env ]]; then
  sed -i "s|^TOKEN_CONTRACT_ID=.*|TOKEN_CONTRACT_ID=$TOKEN_ID|" .env
  sed -i "s|^CAMPAIGN_CONTRACT_ID=.*|CAMPAIGN_CONTRACT_ID=$CAMPAIGN_ID|" .env
  sed -i "s|^REWARDS_CONTRACT_ID=.*|REWARDS_CONTRACT_ID=$REWARDS_ID|" .env
  echo "==> .env updated."
fi
