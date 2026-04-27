#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

// ── Cross-contract interfaces ─────────────────────────────────────────────────

mod token {
    use soroban_sdk::{contractclient, Address, Env};

    #[contractclient(name = "TokenClient")]
    pub trait Token {
        fn mint(env: Env, minter: Address, to: Address, amount: i128);
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
        pub total_claimed: u64,
        pub name: Bytes,
        pub description: Bytes,
    }

    #[contractclient(name = "CampaignClient")]
    pub trait CampaignTrait {
        fn is_active(env: Env, campaign_id: u64) -> bool;
        fn get_campaign(env: Env, campaign_id: u64) -> Campaign;
        fn record_claim(env: Env, recorder: Address, campaign_id: u64);
    }
}

use campaign::Campaign;

// ── Roles ─────────────────────────────────────────────────────────────────────

/// Role identifiers for the rewards contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Admin,
    Pauser,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Claimed(Address, u64),
    TokenContract,
    CampaignContract,
    /// Role membership: (role, address) → bool
    RoleMember(Role, Address),
    Paused,
}

// ── Events ────────────────────────────────────────────────────────────────────

const REWARD_CLAIMED: Symbol = symbol_short!("RWD_CLM");
const REWARD_REDEEMED: Symbol = symbol_short!("RWD_RDM");
const ROLE_GRANTED: Symbol = symbol_short!("ROLE_GRT");
const ROLE_REVOKED: Symbol = symbol_short!("ROLE_REV");
const PAUSED: Symbol = symbol_short!("PAUSED");
const UNPAUSED: Symbol = symbol_short!("UNPAUSED");

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RewardsContract;

#[contractimpl]
impl RewardsContract {
    /// Initialize the rewards contract. `admin` receives ADMIN and PAUSER roles.
    pub fn initialize(
        env: Env,
        admin: Address,
        token_contract: Address,
        campaign_contract: Address,
    ) {
        if env.storage().instance().has(&DataKey::Paused) {
            panic!("already initialized");
        }
        Self::_grant_role(&env, &Role::Admin, &admin);
        Self::_grant_role(&env, &Role::Pauser, &admin);

        env.storage()
            .instance()
            .set(&DataKey::TokenContract, &token_contract);
        env.storage()
            .instance()
            .set(&DataKey::CampaignContract, &campaign_contract);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    // ── Role helpers ──────────────────────────────────────────────────────────

    fn _grant_role(env: &Env, role: &Role, account: &Address) {
        env.storage()
            .instance()
            .set(&DataKey::RoleMember(role.clone(), account.clone()), &true);
    }

    fn _revoke_role(env: &Env, role: &Role, account: &Address) {
        env.storage()
            .instance()
            .remove(&DataKey::RoleMember(role.clone(), account.clone()));
    }

    fn has_role(env: &Env, role: &Role, account: &Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::RoleMember(role.clone(), account.clone()))
            .unwrap_or(false)
    }

    fn require_role(env: &Env, role: &Role, account: &Address) {
        account.require_auth();
        if !Self::has_role(env, role, account) {
            panic!("missing role");
        }
    }

    // ── Role management (ADMIN only) ──────────────────────────────────────────

    /// Grant `role` to `account`. Caller must have ADMIN role.
    pub fn grant_role(env: Env, admin: Address, role: Role, account: Address) {
        Self::require_role(&env, &Role::Admin, &admin);
        Self::_grant_role(&env, &role, &account);
        env.events()
            .publish((ROLE_GRANTED, role), (admin, account));
    }

    /// Revoke `role` from `account`. Caller must have ADMIN role.
    pub fn revoke_role(env: Env, admin: Address, role: Role, account: Address) {
        Self::require_role(&env, &Role::Admin, &admin);
        Self::_revoke_role(&env, &role, &account);
        env.events()
            .publish((ROLE_REVOKED, role), (admin, account));
    }

    /// Returns true if `account` has `role`.
    pub fn has_role_view(env: Env, role: Role, account: Address) -> bool {
        Self::has_role(&env, &role, &account)
    }

    // ── Pause (PAUSER role) ───────────────────────────────────────────────────

    pub fn pause(env: Env, pauser: Address) {
        Self::require_role(&env, &Role::Pauser, &pauser);
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish(PAUSED, pauser);
    }

    pub fn unpause(env: Env, pauser: Address) {
        Self::require_role(&env, &Role::Pauser, &pauser);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish(UNPAUSED, pauser);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn require_not_paused(env: &Env) {
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if paused {
            panic!("contract is paused");
        }
    }

    // ── Cross-contract clients ────────────────────────────────────────────────

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
    fn calc_multiplier(now: u64, created_at: u64, expires_at: u64) -> u64 {
        if now >= expires_at || expires_at <= created_at {
            return 10_000;
        }
        let duration = expires_at - created_at;
        let remaining = expires_at - now;
        let extra = 10_000u64 * remaining / duration;
        10_000 + extra.min(10_000)
    }

    // ── Core user-facing functions ────────────────────────────────────────────

    pub fn claim_reward(env: Env, user: Address, campaign_id: u64) {
        user.require_auth();
        Self::require_not_paused(&env);

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

        // The rewards contract itself is both the recorder and the minter
        let rewards_addr = env.current_contract_address();
        campaign_client.record_claim(&rewards_addr, &campaign_id);
        Self::token_client(&env).mint(&rewards_addr, &user, &final_amount);

        env.events().publish(
            (REWARD_CLAIMED, symbol_short!("user"), user.clone()),
            (campaign_id, final_amount, multiplier_bp),
        );
    }

    pub fn redeem_reward(env: Env, user: Address, amount: i128) {
        user.require_auth();
        Self::require_not_paused(&env);
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
        testutils::{Address as _, Events, Ledger},
        IntoVal, Env,
    };

    struct TestSetup<'a> {
        env: Env,
        admin: Address,
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

        // Grant the rewards contract the MINTER role on the token contract
        token.grant_role(&admin, &soroban_loyalty_token::Role::Minter, &rewards_id);
        // Revoke admin's own minter role so only rewards contract can mint
        token.revoke_role(&admin, &soroban_loyalty_token::Role::Minter, &admin);

        // Grant the rewards contract the RECORDER role on the campaign contract
        campaign.grant_role(&admin, &soroban_loyalty_campaign::Role::Recorder, &rewards_id);

        TestSetup { env, admin, token, campaign, rewards }
    }

    fn make_campaign(t: &TestSetup, merchant: &Address, reward: i128) -> u64 {
        let expiry = t.env.ledger().timestamp() + 86400;
        let name = soroban_sdk::Bytes::from_slice(&t.env, b"Test Campaign");
        let desc = soroban_sdk::Bytes::from_slice(&t.env, b"Test description");
        t.campaign.create_campaign(merchant, &reward, &expiry, &name, &desc)
    }

    // ── Role management tests ─────────────────────────────────────────────────

    #[test]
    fn test_initial_roles_granted_to_admin() {
        let t = setup();
        assert!(t.rewards.has_role_view(&Role::Admin, &t.admin));
        assert!(t.rewards.has_role_view(&Role::Pauser, &t.admin));
    }

    #[test]
    fn test_grant_role_emits_event() {
        let t = setup();
        let pauser = Address::generate(&t.env);
        t.rewards.grant_role(&t.admin, &Role::Pauser, &pauser);
        assert!(t.rewards.has_role_view(&Role::Pauser, &pauser));
    }

    #[test]
    fn test_revoke_role_emits_event() {
        let t = setup();
        let pauser = Address::generate(&t.env);
        t.rewards.grant_role(&t.admin, &Role::Pauser, &pauser);
        t.rewards.revoke_role(&t.admin, &Role::Pauser, &pauser);
        assert!(!t.rewards.has_role_view(&Role::Pauser, &pauser));
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_grant_role_requires_admin() {
        let t = setup();
        let non_admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        t.rewards.grant_role(&non_admin, &Role::Pauser, &target);
    }

    // ── Pause tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_pause_blocks_claim() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.pause(&t.admin);
        // Should panic because paused
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            t.rewards.claim_reward(&user, &cid);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_unpause_allows_claim() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.pause(&t.admin);
        t.rewards.unpause(&t.admin);
        t.rewards.claim_reward(&user, &cid);
        assert!(t.rewards.has_claimed_view(&user, &cid));
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_pause_requires_pauser_role() {
        let t = setup();
        let non_pauser = Address::generate(&t.env);
        t.rewards.pause(&non_pauser);
    }

    // ── Core functionality tests ──────────────────────────────────────────────

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
        // 2x multiplier → 1000 minted; redeem 200 → 800 remaining
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

        assert_eq!(t.token.balance(&user1), 200); // 2x multiplier
        assert_eq!(t.token.balance(&user2), 200);
        assert_eq!(t.token.total_supply_view(), 400);
    }

    #[test]
    fn test_claim_emits_event() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.claim_reward(&user, &cid);

        let events = t.env.events().all();
        let rwd_clm_event = events.iter().find(|(contract, _, _)| {
            *contract == t.rewards.address
        });
        assert!(rwd_clm_event.is_some(), "RWD_CLM event not emitted");
        let (_, topics, _) = rwd_clm_event.unwrap();
        assert_eq!(topics.get(0).unwrap(), REWARD_CLAIMED.into_val(&t.env));
    }

    #[test]
    fn test_redeem_emits_event() {
        let t = setup();
        let merchant = Address::generate(&t.env);
        let user = Address::generate(&t.env);
        let cid = make_campaign(&t, &merchant, 500);
        t.rewards.claim_reward(&user, &cid);
        t.rewards.redeem_reward(&user, &200);

        let events = t.env.events().all();
        let rwd_rdm_event = events.iter().rev().find(|(contract, _, _)| {
            *contract == t.rewards.address
        });
        assert!(rwd_rdm_event.is_some(), "RWD_RDM event not emitted");
        let (_, topics, _) = rwd_rdm_event.unwrap();
        assert_eq!(topics.get(0).unwrap(), REWARD_REDEEMED.into_val(&t.env));
    }
}
