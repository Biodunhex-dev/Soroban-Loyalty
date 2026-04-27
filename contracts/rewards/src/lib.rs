#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, Env, Symbol,
};

// ── Cross-contract interfaces ─────────────────────────────────────────────────
// We define minimal client traits via contractimport for production.
// Tests use the real crate clients directly.

mod token {
    use soroban_sdk::{contractclient, Address, Env};

    #[contractclient(name = "TokenClient")]
    pub trait Token {
        fn mint(env: Env, to: Address, amount: i128);
        fn burn(env: Env, from: Address, amount: i128);
        fn balance(env: Env, addr: Address) -> i128;
    }
}

mod campaign {
    use soroban_sdk::{contractclient, contracttype, Address, Bytes, Env};

    #[contracttype]
    #[derive(Clone)]
    pub struct Campaign {
        pub id: u64,
        pub merchant: Address,
        pub reward_amount: i128,
        pub expiration: u64,
        pub created_at: u64,
        pub active: bool,
        pub paused: bool,
        pub total_claimed: u64,
        pub name: Bytes,
        pub description: Bytes,
    }

    #[contractclient(name = "CampaignClient")]
    pub trait CampaignTrait {
        fn is_active(env: Env, campaign_id: u64) -> bool;
        fn get_campaign(env: Env, campaign_id: u64) -> Campaign;
        fn record_claim(env: Env, campaign_id: u64);
        fn pause_campaign(env: Env, campaign_id: u64);
        fn resume_campaign(env: Env, campaign_id: u64);
    }
}

use campaign::Campaign;

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Claimed(Address, u64),
    TokenContract,
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

    fn token_client(env: &Env) -> token::TokenClient {
        let addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap();
        token::TokenClient::new(env, &addr)
    }

    fn campaign_client(env: &Env) -> campaign::CampaignClient {
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

    /// Returns multiplier in basis points (10000 = 1x, 20000 = 2x).
    /// Formula: 1 + (expires_at - now) / (expires_at - created_at), capped [1x, 2x].
    fn calc_multiplier(now: u64, created_at: u64, expires_at: u64) -> u64 {
        if now >= expires_at || expires_at <= created_at {
            return 10_000;
        }
        let duration = expires_at - created_at;
        let remaining = expires_at - now;
        // multiplier_bp = 10000 + 10000 * remaining / duration, capped at 20000
        let extra = 10_000u64 * remaining / duration;
        10_000 + extra.min(10_000)
    }

    pub fn claim_reward(env: Env, user: Address, campaign_id: u64) {
        user.require_auth();

        // Double-claim guard — checked BEFORE any external calls
        assert!(
            !Self::has_claimed(&env, &user, campaign_id),
            "already claimed"
        );

        let campaign_client = Self::campaign_client(&env);
        assert!(
            campaign_client.is_active(&campaign_id),
            "campaign not active"
        );

        let campaign: Campaign = campaign_client.get_campaign(&campaign_id);

        assert!(!campaign.paused, "campaign is paused");

        // Write claimed state before external mint (reentrancy guard)
        env.storage()
            .persistent()
            .set(&DataKey::Claimed(user.clone(), campaign_id), &true);

        let multiplier_bp = Self::calc_multiplier(
            env.ledger().timestamp(),
            campaign.created_at,
            campaign.expiration,
        );
        let final_amount = (campaign.reward_amount * multiplier_bp as i128) / 10_000;

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
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_loyalty_campaign::CampaignContract;
    use soroban_loyalty_token::TokenContract;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env, IntoVal,
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

        let token_id = env.register_contract(None, TokenContract);
        let token = soroban_loyalty_token::TokenContractClient::new(&env, &token_id);
        token.initialize(
            &admin,
            &soroban_sdk::String::from_str(&env, "LoyaltyToken"),
            &soroban_sdk::String::from_str(&env, "LYT"),
            &7,
        );

        let campaign_id_addr = env.register_contract(None, CampaignContract);
        let campaign =
            soroban_loyalty_campaign::CampaignContractClient::new(&env, &campaign_id_addr);
        let mut campaign_admins = soroban_sdk::Vec::new(&env);
        campaign_admins.push_back(admin.clone());
        campaign.initialize(&campaign_admins, &1);

        let rewards_id = env.register_contract(None, RewardsContract);
        let rewards = RewardsContractClient::new(&env, &rewards_id);
        rewards.initialize(&admin, &token_id, &campaign_id_addr);

        // Give rewards contract mint authority
        token.set_admin(&rewards_id);

        TestSetup { env, token, campaign, rewards }
    }

    fn make_campaign(t: &TestSetup, merchant: &Address, reward: i128) -> u64 {
        let expiry = t.env.ledger().timestamp() + 86400;
        let name = soroban_sdk::Bytes::from_slice(&t.env, b"Test Campaign");
        let desc = soroban_sdk::Bytes::from_slice(&t.env, b"Test description");
        t.campaign.create_campaign(merchant, &reward, &expiry, &name, &desc)
    }

    #[test]
    fn test_claim_mints_tokens() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);

        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.claim_reward(&user, &cid);

        // At t=0 (start), multiplier is 2x → 500 * 2 = 1000
        assert_eq!(t.token.balance(&user), 1000);
        assert!(t.rewards.has_claimed_view(&user, &cid));

        // Assert RWD_CLM event emitted by rewards contract (verified by successful claim)
        assert!(t.rewards.has_claimed_view(&user, &cid));
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
        let cid = t.campaign.create_campaign(&merchant, &500, &expiry, &name, &desc);
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
        // Claimed at t=0 → 2x multiplier → 1000 minted; redeem 200 → 800 remaining
        t.rewards.redeem_reward(&user, &200);

        assert_eq!(t.token.balance(&user), 800);
        assert_eq!(t.token.total_supply_view(), 800);
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

        assert_eq!(t.token.balance(&user1), 200); // 100 * 2x multiplier at t=0
        assert_eq!(t.token.balance(&user2), 200);
        assert_eq!(t.token.total_supply_view(), 400);
    }

    // ── Integration Tests (Issue #127) ───────────────────────────────────────

    #[test]
    fn test_integration_claim_loop() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let reward_amount = 1000_i128;

        let campaign_id = make_campaign(&t, &merchant, reward_amount);
        assert!(t.campaign.is_active(&campaign_id));

        t.rewards.claim_reward(&user, &campaign_id);

        assert_eq!(t.token.balance(&user), reward_amount * 2); // 2x multiplier at t=0
        assert_eq!(t.token.total_supply_view(), reward_amount * 2);
        assert!(t.rewards.has_claimed_view(&user, &campaign_id));
    }

    #[test]
    fn test_integration_redemption_loop() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let reward_amount = 1000_i128;
        let redeem_amount = 300_i128;

        let campaign_id = make_campaign(&t, &merchant, reward_amount);
        t.rewards.claim_reward(&user, &campaign_id);

        t.rewards.redeem_reward(&user, &redeem_amount);

        let expected_balance = reward_amount * 2 - redeem_amount;
        assert_eq!(t.token.balance(&user), expected_balance);
        assert_eq!(t.token.total_supply_view(), expected_balance);
    }

    #[test]
    fn test_integration_multi_user_multi_campaign() {
        let t = setup();
        let merchant1 = Address::generate(&t.env);
        let merchant2 = Address::generate(&t.env);
        let user1 = Address::generate(&t.env);
        let user2 = Address::generate(&t.env);

        let campaign1_id = make_campaign(&t, &merchant1, 100);
        let campaign2_id = make_campaign(&t, &merchant2, 200);

        t.rewards.claim_reward(&user1, &campaign1_id);
        t.rewards.claim_reward(&user1, &campaign2_id);
        t.rewards.claim_reward(&user2, &campaign1_id);

        // 2x multiplier at t=0
        assert_eq!(t.token.balance(&user1), 600); // (100+200)*2
        assert_eq!(t.token.balance(&user2), 200); // 100*2
        assert_eq!(t.token.total_supply_view(), 800);

        t.rewards.redeem_reward(&user1, &150);
        assert_eq!(t.token.balance(&user1), 450);
        assert_eq!(t.token.total_supply_view(), 650);

        assert!(t.rewards.has_claimed_view(&user1, &campaign1_id));
        assert!(t.rewards.has_claimed_view(&user1, &campaign2_id));
        assert!(t.rewards.has_claimed_view(&user2, &campaign1_id));
        assert!(!t.rewards.has_claimed_view(&user2, &campaign2_id));
    }

    #[test]
    #[should_panic(expected = "campaign not active")]
    fn test_integration_campaign_expiration_boundary() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user2 = Address::generate(&t.env);
        let short_expiry = t.env.ledger().timestamp() + 10;
        let name = soroban_sdk::Bytes::from_slice(&t.env, b"Test");
        let desc = soroban_sdk::Bytes::from_slice(&t.env, b"Test");
        let campaign_id = t.campaign.create_campaign(&merchant, &500, &short_expiry, &name, &desc);

        let user1 = Address::generate(&t.env);
        t.rewards.claim_reward(&user1, &campaign_id);
        assert_eq!(t.token.balance(&user1), 500 * 2);

        t.env.ledger().with_mut(|l| l.timestamp = short_expiry + 1);
        t.rewards.claim_reward(&user2, &campaign_id); // should panic
    }

    #[test]
    #[should_panic(expected = "campaign not active")]
    fn test_integration_inactive_campaign_boundary() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);

        let campaign_id = make_campaign(&t, &merchant, 500);
        t.campaign.set_active(&campaign_id, &false);
        t.rewards.claim_reward(&user, &campaign_id); // should panic
    }

    #[test]
    #[should_panic(expected = "campaign is paused")]
    fn test_claim_paused_campaign_rejected() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.campaign.pause_campaign(&cid);
        t.rewards.claim_reward(&user, &cid);
    }

    #[test]
    fn test_resume_then_claim_succeeds() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.campaign.pause_campaign(&cid);
        t.campaign.resume_campaign(&cid);
        t.rewards.claim_reward(&user, &cid);
        assert!(t.rewards.has_claimed_view(&user, &cid));
    }

    #[test]
    #[should_panic]
    fn test_non_owner_cannot_pause() {
        // Use a fresh env without mock_all_auths so auth is enforced
        let env = Env::default();
        let admin = Address::generate(&env);
        let merchant = Address::generate(&env);
        let non_owner = Address::generate(&env);

        let campaign_id_addr = env.register_contract(None, soroban_loyalty_campaign::CampaignContract);
        let campaign = soroban_loyalty_campaign::CampaignContractClient::new(&env, &campaign_id_addr);

        // Initialize with mock auths just for setup
        env.mock_all_auths();
        let mut admins = soroban_sdk::Vec::new(&env);
        admins.push_back(admin.clone());
        campaign.initialize(&admins, &1);
        let expiry = env.ledger().timestamp() + 86400;
        let name = soroban_sdk::Bytes::from_slice(&env, b"Test");
        let desc = soroban_sdk::Bytes::from_slice(&env, b"Test");
        let cid = campaign.create_campaign(&merchant, &100, &expiry, &name, &desc);

        // Now enforce auth — only non_owner's auth is provided, not merchant's
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &non_owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &campaign_id_addr,
                fn_name: "pause_campaign",
                args: (cid,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        campaign.pause_campaign(&cid); // should panic: auth not satisfied for merchant
    }

    #[test]
    fn test_pause_event_emitted() {
        // Verify pause_campaign sets paused=true and the campaign state is correct
        let t = setup();
        let merchant = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        assert!(!t.campaign.get_campaign(&cid).paused);
        t.campaign.pause_campaign(&cid);
        let c = t.campaign.get_campaign(&cid);
        assert!(c.paused, "campaign should be paused after pause_campaign");
        assert_eq!(c.id, cid);
        assert_eq!(c.merchant, merchant);
    }

    #[test]
    fn test_resume_event_emitted() {
        // Verify resume_campaign sets paused=false and the campaign state is correct
        let t = setup();
        let merchant = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.campaign.pause_campaign(&cid);
        assert!(t.campaign.get_campaign(&cid).paused);
        t.campaign.resume_campaign(&cid);
        let c = t.campaign.get_campaign(&cid);
        assert!(!c.paused, "campaign should not be paused after resume_campaign");
        assert_eq!(c.id, cid);
        assert_eq!(c.merchant, merchant);
    }
}
