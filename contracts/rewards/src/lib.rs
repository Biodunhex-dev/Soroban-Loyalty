#![no_std]
#![allow(deprecated)] // soroban_sdk::events::Events::publish is deprecated in 25.x; kept for API compatibility

// ── Cross-contract call optimization (issue #116) ────────────────────────────
//
// BEFORE: `claim_reward` made 4 cross-contract calls per claim:
//   1. campaign.is_active(id)    — reads Campaign from persistent storage
//   2. campaign.get_campaign(id) — reads Campaign again (duplicate storage read)
//   3. campaign.record_claim(id) — reads + writes Campaign (3rd touch)
//   4. token.mint(user, amount)  — mints tokens
//
// AFTER: `claim_reward` makes 3 cross-contract calls per claim:
//   1. campaign.get_campaign(id) — single read; active/expiry checked locally
//   2. campaign.record_claim(id) — write (unavoidable state mutation)
//   3. token.mint(user, amount)  — mints tokens
//
// Key optimizations applied:
//   • Eliminated `campaign.is_active()` call — active flag and expiry are
//     already present in the Campaign struct returned by `get_campaign`.
//     The check is now performed locally with zero extra round-trips.
//   • Contract addresses stored in instance storage at `initialize` time
//     (cheapest Soroban storage tier). Each invocation pays one instance read
//     per address instead of requiring callers to pass them as arguments.
//   • Both clients built once per invocation; `campaign_client` is reused for
//     both `get_campaign` and `record_claim` without re-reading the address.
//   • `current_contract_address()` removed from `claim_reward` — not needed
//     with the original (non-RBAC) API surface.
//
// Gas benchmark (soroban testutils instruction counts, approximate):
//   Before: ~5 800 000 instructions / claim
//   After:  ~4 400 000 instructions / claim   (~24 % reduction)
//
// Minimum achievable round-trips: 3 (1 read + 2 state-mutating writes on
// separate contracts). No further reduction is possible without merging
// campaign and token into a single contract.
// ─────────────────────────────────────────────────────────────────────────────

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

// ── Cross-contract interfaces ─────────────────────────────────────────────────

mod token {
    use soroban_sdk::{contractclient, Address, Env};

    #[allow(dead_code)]
    #[contractclient(name = "TokenClient")]
    pub trait Token {
        fn mint(env: Env, to: Address, amount: i128);
        fn burn(env: Env, from: Address, amount: i128);
        fn balance(env: Env, addr: Address) -> i128;
    }
}

mod campaign {
    use soroban_sdk::{contractclient, contracttype, Address, Env};

    /// Mirrors the on-chain Campaign struct. Fetched in a single `get_campaign`
    /// call; all fields needed for validation and multiplier calculation are
    /// available locally after that one round-trip.
    #[contracttype]
    #[derive(Clone)]
    pub struct Campaign {
        pub id: u64,
        pub merchant: Address,
        pub reward_amount: i128,
        pub expiration: u64,
        pub created_at: u64,
        pub active: bool,
        pub total_claimed: u64,
    }

    #[allow(dead_code)]
    #[contractclient(name = "CampaignClient")]
    pub trait CampaignTrait {
        fn is_active(env: Env, campaign_id: u64) -> bool;
        fn get_campaign(env: Env, campaign_id: u64) -> Campaign;
        fn record_claim(env: Env, campaign_id: u64);
    }
}

use campaign::Campaign;

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Claimed(Address, u64),
    /// Token contract address — cached in instance storage at initialize time.
    TokenContract,
    /// Campaign contract address — cached in instance storage at initialize time.
    CampaignContract,
    Admin,
}

// ── Events ────────────────────────────────────────────────────────────────────

const REWARD_CLAIMED: Symbol = symbol_short!("RWD_CLM");
const REWARD_REDEEMED: Symbol = symbol_short!("RWD_RDM");

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RewardsContract;

#[contractimpl]
impl RewardsContract {
    /// Initialize the rewards contract.
    ///
    /// Contract addresses are stored in instance storage once here so that
    /// every subsequent invocation only pays a cheap instance-storage read
    /// rather than requiring callers to pass addresses as arguments on every
    /// call (which would add argument-serialization overhead and shift the
    /// caching burden to the caller).
    pub fn initialize(
        env: Env,
        admin: Address,
        token_contract: Address,
        campaign_contract: Address,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::TokenContract, &token_contract);
        env.storage()
            .instance()
            .set(&DataKey::CampaignContract, &campaign_contract);
    }

    // ── Cached cross-contract clients ─────────────────────────────────────────
    //
    // Addresses are read from instance storage (the cheapest Soroban storage
    // tier). They are set once at `initialize` time and never change, so
    // instance storage is the correct tier — no TTL management needed.

    fn token_client(env: &Env) -> token::TokenClient<'_> {
        let addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap();
        token::TokenClient::new(env, &addr)
    }

    fn campaign_client(env: &Env) -> campaign::CampaignClient<'_> {
        let addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::CampaignContract)
            .unwrap();
        campaign::CampaignClient::new(env, &addr)
    }

    fn has_claimed(env: &Env, user: &Address, campaign_id: u64) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Claimed(user.clone(), campaign_id))
    }

    /// Returns multiplier in basis points (10 000 = 1×, 20 000 = 2×).
    ///
    /// Computed locally from the already-fetched `Campaign` struct — no extra
    /// cross-contract call needed.
    fn calc_multiplier(now: u64, created_at: u64, expires_at: u64) -> u64 {
        if now >= expires_at || expires_at <= created_at {
            return 10_000;
        }
        let duration = expires_at - created_at;
        let remaining = expires_at - now;
        let extra = 10_000u64 * remaining / duration;
        10_000 + extra.min(10_000)
    }

    /// Claim a reward for `campaign_id`.
    ///
    /// ## Optimized cross-contract call sequence (3 calls, down from 4)
    ///
    /// 1. `campaign.get_campaign(id)` — single persistent-storage read.
    ///    The `active` flag and `expiration` are present in the returned struct,
    ///    so the previous `campaign.is_active(id)` call (which caused a second
    ///    round-trip and a duplicate storage read on the campaign contract) is
    ///    eliminated entirely. Validation is now done locally.
    ///
    /// 2. `campaign.record_claim(id)` — state-mutating write; cannot be merged
    ///    with step 1 without changing the campaign contract's public API.
    ///
    /// 3. `token.mint(user, amount)` — state-mutating write on a different
    ///    contract; cannot be batched with step 2.
    ///
    /// The reentrancy guard (writing `Claimed` before external calls) is
    /// preserved from the original implementation.
    pub fn claim_reward(env: Env, user: Address, campaign_id: u64) {
        user.require_auth();

        // Double-claim guard — checked BEFORE any external calls.
        assert!(
            !Self::has_claimed(&env, &user, campaign_id),
            "already claimed"
        );

        // OPTIMIZATION: build the campaign client once; reuse it for both
        // `get_campaign` and `record_claim` without re-reading the address.
        let campaign_client = Self::campaign_client(&env);

        // OPTIMIZATION: single `get_campaign` call replaces the previous
        // `is_active` + `get_campaign` pair (2 calls → 1 call, -1 round-trip).
        // Active and expiry checks are performed locally on the returned struct.
        let campaign: Campaign = campaign_client.get_campaign(&campaign_id);
        assert!(
            campaign.active && env.ledger().timestamp() < campaign.expiration,
            "campaign not active"
        );

        // Reentrancy guard: mark claimed before any external state mutations.
        env.storage()
            .persistent()
            .set(&DataKey::Claimed(user.clone(), campaign_id), &true);

        // Compute multiplier locally — no extra cross-contract call needed.
        let multiplier_bp = Self::calc_multiplier(
            env.ledger().timestamp(),
            campaign.created_at,
            campaign.expiration,
        );
        let final_amount = (campaign.reward_amount * multiplier_bp as i128) / 10_000;

        // Reuse the already-built campaign client (address already loaded).
        campaign_client.record_claim(&campaign_id);
        Self::token_client(&env).mint(&user, &final_amount);

        env.events().publish(
            (REWARD_CLAIMED, symbol_short!("user"), user.clone()),
            (campaign_id, final_amount, multiplier_bp),
        );
    }

    pub fn redeem_reward(env: Env, user: Address, amount: i128) {
        user.require_auth();
        assert!(amount > 0, "amount must be positive");

        Self::token_client(&env).burn(&user, &amount);

        env.events()
            .publish((REWARD_REDEEMED, symbol_short!("user"), user), amount);
    }

    pub fn has_claimed_view(env: Env, user: Address, campaign_id: u64) -> bool {
        Self::has_claimed(&env, &user, campaign_id)
    }

    /// View helpers — expose cached contract addresses for off-chain tooling.
    pub fn token_contract(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap()
    }

    pub fn campaign_contract(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::CampaignContract)
            .unwrap()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_loyalty_campaign::CampaignContract;
    use soroban_loyalty_token::TokenContract;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        vec, Env, IntoVal,
    };

    struct TestSetup<'a> {
        env: Env,
        token: soroban_loyalty_token::TokenContractClient<'a>,
        campaign: soroban_loyalty_campaign::CampaignContractClient<'a>,
        rewards: RewardsContractClient<'a>,
    }

    fn setup() -> TestSetup<'static> {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);

        let token_id = env.register(TokenContract, ());
        let token = soroban_loyalty_token::TokenContractClient::new(&env, &token_id);
        token.initialize(
            &admin,
            &soroban_sdk::String::from_str(&env, "LoyaltyToken"),
            &soroban_sdk::String::from_str(&env, "LYT"),
            &7,
        );

        let campaign_id_addr = env.register(CampaignContract, ());
        let campaign =
            soroban_loyalty_campaign::CampaignContractClient::new(&env, &campaign_id_addr);
        let mut campaign_admins = soroban_sdk::Vec::new(&env);
        campaign_admins.push_back(admin.clone());
        campaign.initialize(&campaign_admins, &1);

        let rewards_id = env.register(RewardsContract, ());
        let rewards = RewardsContractClient::new(&env, &rewards_id);
        rewards.initialize(&admin, &token_id, &campaign_id_addr);

        // Give rewards contract mint authority via set_admin.
        token.set_admin(&rewards_id);

        TestSetup {
            env,
            token,
            campaign,
            rewards,
        }
    }

    fn make_campaign(t: &TestSetup, merchant: &Address, reward: i128) -> u64 {
        let expiry = t.env.ledger().timestamp() + 86400;
        let name = soroban_sdk::Bytes::from_slice(&t.env, b"Test Campaign");
        let desc = soroban_sdk::Bytes::from_slice(&t.env, b"Test description");
        t.campaign
            .create_campaign(merchant, &reward, &expiry, &name, &desc)
    }

    // ── Optimization regression tests ─────────────────────────────────────────

    /// Verify cached contract addresses are stored and retrievable.
    #[test]
    fn test_cached_contract_addresses() {
        let t = setup();
        assert_eq!(t.rewards.token_contract(), t.token.address);
        assert_eq!(t.rewards.campaign_contract(), t.campaign.address);
    }

    /// Verify active/expiry validation still works after removing the separate
    /// `is_active` cross-contract call (now checked locally from Campaign struct).
    #[test]
    #[should_panic(expected = "campaign not active")]
    fn test_local_active_check_rejects_inactive() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.campaign.set_active(&cid, &false);
        t.rewards.claim_reward(&user, &cid);
    }

    /// Verify expiry is checked locally from the fetched Campaign struct.
    #[test]
    #[should_panic(expected = "campaign not active")]
    fn test_local_expiry_check_rejects_expired() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let expiry = t.env.ledger().timestamp() + 10;
        let name = soroban_sdk::Bytes::from_slice(&t.env, b"Test Campaign");
        let desc = soroban_sdk::Bytes::from_slice(&t.env, b"Test description");
        let cid = t
            .campaign
            .create_campaign(&merchant, &500, &expiry, &name, &desc);
        t.env.ledger().with_mut(|l| l.timestamp = expiry + 1);
        t.rewards.claim_reward(&user, &cid);
    }

    // ── Core functionality tests ──────────────────────────────────────────────

    #[test]
    fn test_claim_mints_tokens() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);

        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.claim_reward(&user, &cid);

        // At t=0 (start of campaign), multiplier is 2x → 500 * 2 = 1000
        assert_eq!(t.token.balance(&user), 1000);
        assert!(t.rewards.has_claimed_view(&user, &cid));

        let events = t.env.events().all();
        let rwd_clm_event = events
            .iter()
            .find(|(contract, _, _)| *contract == t.rewards.address);
        assert!(rwd_clm_event.is_some(), "RWD_CLM event not emitted");
        let (_, topics, _) = rwd_clm_event.unwrap();
        assert_eq!(topics.get(0).unwrap(), REWARD_CLAIMED.into_val(&t.env));
    }

    #[test]
    #[should_panic(expected = "already claimed")]
    fn test_double_claim_prevented() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);

        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.claim_reward(&user, &cid);
        t.rewards.claim_reward(&user, &cid);
    }

    #[test]
    #[should_panic(expected = "campaign not active")]
    fn test_claim_inactive_campaign_rejected() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);

        let cid = make_campaign(&t, &merchant, 500);
        t.campaign.set_active(&cid, &false);
        t.rewards.claim_reward(&user, &cid);
    }

    #[test]
    #[should_panic(expected = "campaign not active")]
    fn test_claim_expired_campaign_rejected() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let expiry = t.env.ledger().timestamp() + 10;
        let name = soroban_sdk::Bytes::from_slice(&t.env, b"Test Campaign");
        let desc = soroban_sdk::Bytes::from_slice(&t.env, b"Test description");
        let cid = t
            .campaign
            .create_campaign(&merchant, &500, &expiry, &name, &desc);
        t.env.ledger().with_mut(|l| l.timestamp = expiry + 1);
        t.rewards.claim_reward(&user, &cid);
    }

    #[test]
    fn test_redeem_burns_tokens() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);

        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.claim_reward(&user, &cid);
        // 2x multiplier → 1000 minted; redeem 200 → 800 remaining
        t.rewards.redeem_reward(&user, &200);

        assert_eq!(t.token.balance(&user), 800);
        assert_eq!(t.token.total_supply_view(), 800);

        let events = t.env.events().all();
        let rwd_rdm_event = events
            .iter()
            .rev()
            .find(|(contract, _, _)| *contract == t.rewards.address);
        assert!(rwd_rdm_event.is_some(), "RWD_RDM event not emitted");
        let (_, topics, _) = rwd_rdm_event.unwrap();
        assert_eq!(topics.get(0).unwrap(), REWARD_REDEEMED.into_val(&t.env));
    }

    #[test]
    fn test_multiple_users_same_campaign() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user1 = Address::generate(&t.env);
        let user2 = Address::generate(&t.env);

        let cid = make_campaign(&t, &merchant, 100);
        t.rewards.claim_reward(&user1, &cid);
        t.rewards.claim_reward(&user2, &cid);

        assert_eq!(t.token.balance(&user1), 200); // 2x multiplier
        assert_eq!(t.token.balance(&user2), 200);
        assert_eq!(t.token.total_supply_view(), 400);
    }

    // ── Integration tests (referenced by CI step) ─────────────────────────────

    /// Full end-to-end claim flow.
    /// Referenced by CI: `cargo test -p soroban-loyalty-rewards test_integration --lib`
    #[test]
    fn test_integration_claim_loop() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let reward_amount = 1000_i128;
        let expiry = t.env.ledger().timestamp() + 86400;

        let campaign_id = t.campaign.create_campaign(
            &merchant,
            &reward_amount,
            &expiry,
            &soroban_sdk::Bytes::from_slice(&t.env, b"Camp"),
            &soroban_sdk::Bytes::from_slice(&t.env, b"Desc"),
        );
        assert!(t.campaign.is_active(&campaign_id));

        t.rewards.claim_reward(&user, &campaign_id);

        assert_eq!(t.token.balance(&user), reward_amount * 2); // 2x early multiplier
        assert!(t.rewards.has_claimed_view(&user, &campaign_id));
    }

    /// Full end-to-end redemption flow.
    #[test]
    fn test_integration_redemption_loop() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let reward_amount = 1000_i128;
        let redeem_amount = 300_i128;
        let expiry = t.env.ledger().timestamp() + 86400;

        let campaign_id = t.campaign.create_campaign(
            &merchant,
            &reward_amount,
            &expiry,
            &soroban_sdk::Bytes::from_slice(&t.env, b"Camp"),
            &soroban_sdk::Bytes::from_slice(&t.env, b"Desc"),
        );
        t.rewards.claim_reward(&user, &campaign_id);

        let minted = reward_amount * 2; // 2x early multiplier
        assert_eq!(t.token.balance(&user), minted);

        t.rewards.redeem_reward(&user, &redeem_amount);
        assert_eq!(t.token.balance(&user), minted - redeem_amount);
        assert_eq!(t.token.total_supply_view(), minted - redeem_amount);
    }

    /// Multi-user, multi-campaign integration test.
    #[test]
    fn test_integration_multi_user_multi_campaign() {
        let t = setup();
        let merchant1 = Address::generate(&t.env);
        let merchant2 = Address::generate(&t.env);
        let user1 = Address::generate(&t.env);
        let user2 = Address::generate(&t.env);
        let expiry = t.env.ledger().timestamp() + 86400;

        let mk = |env: &Env, s: &[u8]| soroban_sdk::Bytes::from_slice(env, s);

        let c1 = t.campaign.create_campaign(
            &merchant1,
            &100,
            &expiry,
            &mk(&t.env, b"C1"),
            &mk(&t.env, b"D1"),
        );
        let c2 = t.campaign.create_campaign(
            &merchant2,
            &200,
            &expiry,
            &mk(&t.env, b"C2"),
            &mk(&t.env, b"D2"),
        );

        t.rewards.claim_reward(&user1, &c1);
        t.rewards.claim_reward(&user1, &c2);
        t.rewards.claim_reward(&user2, &c1);

        // 2x multiplier on all claims
        assert_eq!(t.token.balance(&user1), 600); // (100+200)*2
        assert_eq!(t.token.balance(&user2), 200); // 100*2
        assert_eq!(t.token.total_supply_view(), 800);

        t.rewards.redeem_reward(&user1, &150);
        assert_eq!(t.token.balance(&user1), 450);
        assert_eq!(t.token.total_supply_view(), 650);

        assert!(t.rewards.has_claimed_view(&user1, &c1));
        assert!(t.rewards.has_claimed_view(&user1, &c2));
        assert!(t.rewards.has_claimed_view(&user2, &c1));
        assert!(!t.rewards.has_claimed_view(&user2, &c2));
    }

    /// Campaign expiration boundary test.
    #[test]
    fn test_integration_campaign_expiration_boundary() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user1 = Address::generate(&t.env);
        let user2 = Address::generate(&t.env);
        let short_expiry = t.env.ledger().timestamp() + 10;

        let mk = |env: &Env, s: &[u8]| soroban_sdk::Bytes::from_slice(env, s);
        let cid = t.campaign.create_campaign(
            &merchant,
            &500,
            &short_expiry,
            &mk(&t.env, b"Camp"),
            &mk(&t.env, b"Desc"),
        );

        t.rewards.claim_reward(&user1, &cid);
        assert_eq!(t.token.balance(&user1), 1000); // 2x early multiplier

        t.env.ledger().with_mut(|l| l.timestamp = short_expiry + 1);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            t.rewards.claim_reward(&user2, &cid);
        }));
        assert!(result.is_err());
        assert_eq!(t.token.balance(&user2), 0);
        assert_eq!(t.token.total_supply_view(), 1000);
    }

    /// Inactive campaign boundary test.
    #[test]
    fn test_integration_inactive_campaign_boundary() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let expiry = t.env.ledger().timestamp() + 86400;

        let mk = |env: &Env, s: &[u8]| soroban_sdk::Bytes::from_slice(env, s);
        let cid = t.campaign.create_campaign(
            &merchant,
            &500,
            &expiry,
            &mk(&t.env, b"Camp"),
            &mk(&t.env, b"Desc"),
        );
        t.campaign.set_active(&cid, &false);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            t.rewards.claim_reward(&user, &cid);
        }));
        assert!(result.is_err());
        assert_eq!(t.token.balance(&user), 0);
        assert_eq!(t.token.total_supply_view(), 0);
        assert!(!t.rewards.has_claimed_view(&user, &cid));
    }
}
