// Minimal env vars required for module-level initialization in tests
process.env.SOROBAN_RPC_URL = process.env.SOROBAN_RPC_URL ?? "http://localhost:8000";
process.env.NETWORK_PASSPHRASE = process.env.NETWORK_PASSPHRASE ?? "Test SDF Network ; September 2015";
process.env.DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://loyalty:loyalty@localhost:5432/loyalty";
process.env.TEST_DATABASE_URL = process.env.TEST_DATABASE_URL ?? process.env.DATABASE_URL;
process.env.NODE_ENV = "test";
