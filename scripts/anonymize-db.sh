#!/bin/bash
# Anonymizes production data for staging environment
# Replaces sensitive stellar addresses and transaction hashes with mock data equivalents

set -e

DB_URL=$1

if [ -z "$DB_URL" ]; then
  echo "Error: Database connection URL is required."
  exit 1
fi

echo "Running data anonymization on staging database..."

psql "$DB_URL" <<EOF
-- Shuffle user addresses securely (56 char length, starts with G)
UPDATE users 
SET address = 'G' || upper(substr(md5(random()::text) || md5(random()::text), 1, 55));

-- Anonymize campaign merchants
UPDATE campaigns 
SET merchant = 'G' || upper(substr(md5(random()::text) || md5(random()::text), 1, 55));

-- Anonymize transaction hashes (64 char length)
UPDATE transactions SET tx_hash = substr(md5(random()::text) || md5(random()::text), 1, 64);
UPDATE campaigns SET tx_hash = substr(md5(random()::text) || md5(random()::text), 1, 64);
EOF