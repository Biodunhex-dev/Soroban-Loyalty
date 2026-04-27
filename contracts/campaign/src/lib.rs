#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, Env, Symbol,
};

// ── Roles ─────────────────────────────────────────────────────────────────────

/// Role identifiers for the campaign contract.
/// ADMIN can assign/revoke roles and manage upgrades.
/// PAUSER can pause/unpause the contract.
/// RECORDER is granted to the rewards contract so it can call `record_claim`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Admin,
    Pauser,
    Recorder,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct Campaign {
    pub id: u64,
    pub merchant: Address,
    pub reward_amount: i128,
    pub expiration: u64,
    pub created_at: u64,
    pub active: bool,
    pub total_claimed: u64,
    /// Campaign name — max 64 bytes UTF-8
    pub name: Bytes,
    /// Campaign description — max 256 bytes UTF-8
    pub description: Bytes,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct UpgradeProposal {
    pub wasm_hash: soroban_sdk::BytesN<32>,
    pub proposed_at: u64,
    pub signatures: soroban_sdk::Vec<Address>,
}

#[contracttype]
pub enum DataKey {
    Campaign(u64),
    NextId,
    /// Role membership: (role, address) → bool
    RoleMember(Role, Address),
    /// Multi-sig threshold for upgrades
    Threshold,
    UpgradeProposal,
    Paused,
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// 48-hour timelock for upgrades (in seconds)
const TIMELOCK: u64 = 172_800;

// ── Events ────────────────────────────────────────────────────────────────────

const CAMPAIGN_CREATED: Symbol = symbol_short!("CAM_CRT");
const CAMPAIGN_DEACTIVATED: Symbol = symbol_short!("CAM_DEACT");
const ROLE_GRANTED: Symbol = symbol_short!("ROLE_GRT");
const ROLE_REVOKED: Symbol = symbol_short!("ROLE_REV");
const PAUSED: Symbol = symbol_short!("PAUSED");
const UNPAUSED: Symbol = symbol_short!("UNPAUSED");
const UPGRADE_PROPOSED: Symbol = symbol_short!("UPG_PROP");
const UPGRADE_AUTHORIZED: Symbol = symbol_short!("UPG_AUTH");
const UPGRADE_EXECUTED: Symbol = symbol_short!("UPG_EXEC");
const UPGRADE_CANCELLED: Symbol = symbol_short!("UPG_CNCL");

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    /// Initialize the contract. `admins` receive ADMIN and PAUSER roles.
    /// `threshold` is the number of admin signatures required for upgrades.
    pub fn initialize(env: Env, admins: soroban_sdk::Vec<Address>, threshold: u32) {
        if env.storage().instance().has(&DataKey::Paused) {
            panic!("already initialized");
        }
        assert!(threshold > 0, "threshold must be positive");
        assert!(admins.len() >= threshold, "insufficient admins for threshold");

        for admin in admins.iter() {
            Self::_grant_role(&env, &Role::Admin, &admin);
            Self::_grant_role(&env, &Role::Pauser, &admin);
        }

        env.storage().instance().set(&DataKey::Threshold, &threshold);
        env.storage().instance().set(&DataKey::NextId, &1_u64);
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

    // ── ID helpers ────────────────────────────────────────────────────────────

    fn next_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(1)
    }

    fn bump_id(env: &Env) -> u64 {
        let id = Self::next_id(env);
        env.storage()
            .instance()
            .set(&DataKey::NextId, &(id + 1));
        id
    }

    // ── Public interface ──────────────────────────────────────────────────────

    /// Create a new campaign. Only the merchant (caller) can create it.
    pub fn create_campaign(
        env: Env,
        merchant: Address,
        reward_amount: i128,
        expiration: u64,
        name: Bytes,
        description: Bytes,
    ) -> u64 {
        merchant.require_auth();
        Self::require_not_paused(&env);
        assert!(reward_amount > 0, "reward_amount must be positive");
        assert!(
            expiration > env.ledger().timestamp(),
            "expiration must be in the future"
        );
        assert!(name.len() <= 64, "name exceeds 64 bytes");
        assert!(description.len() <= 256, "description exceeds 256 bytes");

        let id = Self::bump_id(&env);
        let campaign = Campaign {
            id,
            merchant: merchant.clone(),
            reward_amount,
            expiration,
            created_at: env.ledger().timestamp(),
            active: true,
            total_claimed: 0,
            name: name.clone(),
            description: description.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(id), &campaign);

        env.events().publish(
            (CAMPAIGN_CREATED, symbol_short!("id"), id),
            (merchant, name, description),
        );

        id
    }

    /// Deactivate / reactivate a campaign. Only the campaign's merchant can do this.
    pub fn set_active(env: Env, campaign_id: u64, active: bool) {
        let mut campaign = Self::get_campaign_internal(&env, campaign_id);
        campaign.merchant.require_auth();
        campaign.active = active;
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &campaign);

        if !active {
            env.events().publish(
                (CAMPAIGN_DEACTIVATED, symbol_short!("id"), campaign_id),
                campaign.merchant,
            );
        }
    }

    /// Increment the claim counter. Caller must have RECORDER role.
    /// This restricts the function to the rewards contract only.
    pub fn record_claim(env: Env, recorder: Address, campaign_id: u64) {
        Self::require_role(&env, &Role::Recorder, &recorder);
        let mut campaign = Self::get_campaign_internal(&env, campaign_id);
        campaign.total_claimed = campaign
            .total_claimed
            .checked_add(1)
            .expect("overflow");
        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &campaign);
    }

    pub fn get_campaign(env: Env, campaign_id: u64) -> Campaign {
        Self::get_campaign_internal(&env, campaign_id)
    }

    pub fn get_campaign_metadata(env: Env, campaign_id: u64) -> (Bytes, Bytes) {
        let c = Self::get_campaign_internal(&env, campaign_id);
        (c.name, c.description)
    }

    fn get_campaign_internal(env: &Env, campaign_id: u64) -> Campaign {
        env.storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .expect("campaign not found")
    }

    pub fn is_active(env: Env, campaign_id: u64) -> bool {
        let c = Self::get_campaign_internal(&env, campaign_id);
        c.active && env.ledger().timestamp() < c.expiration
    }

    // ── Upgrade mechanism (ADMIN multi-sig + timelock) ────────────────────────

    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: soroban_sdk::BytesN<32>) {
        Self::require_role(&env, &Role::Admin, &admin);
        if env.storage().instance().has(&DataKey::UpgradeProposal) {
            panic!("upgrade already proposed");
        }

        let mut signatures = soroban_sdk::Vec::new(&env);
        signatures.push_back(admin.clone());

        let proposal = UpgradeProposal {
            wasm_hash: wasm_hash.clone(),
            proposed_at: env.ledger().timestamp(),
            signatures,
        };

        env.storage().instance().set(&DataKey::UpgradeProposal, &proposal);
        env.events().publish((UPGRADE_PROPOSED, wasm_hash), admin);
    }

    pub fn authorize_upgrade(env: Env, admin: Address) {
        Self::require_role(&env, &Role::Admin, &admin);
        let mut proposal: UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal)
            .expect("no pending proposal");

        for signee in proposal.signatures.iter() {
            if signee == admin {
                panic!("already authorized by this admin");
            }
        }

        proposal.signatures.push_back(admin.clone());
        env.storage().instance().set(&DataKey::UpgradeProposal, &proposal);
        env.events().publish(UPGRADE_AUTHORIZED, admin);
    }

    pub fn execute_upgrade(env: Env, admin: Address) {
        Self::require_role(&env, &Role::Admin, &admin);
        let proposal: UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal)
            .expect("no pending proposal");

        let threshold: u32 = env.storage().instance().get(&DataKey::Threshold).unwrap();
        assert!(
            proposal.signatures.len() >= threshold,
            "insufficient authorizations"
        );
        assert!(
            env.ledger().timestamp() >= proposal.proposed_at + TIMELOCK,
            "timelock not met"
        );

        env.deployer().update_current_contract_wasm(proposal.wasm_hash.clone());
        env.storage().instance().remove(&DataKey::UpgradeProposal);
        env.events().publish(UPGRADE_EXECUTED, proposal.wasm_hash);
    }

    pub fn cancel_upgrade(env: Env, admin: Address) {
        Self::require_role(&env, &Role::Admin, &admin);
        env.storage().instance().remove(&DataKey::UpgradeProposal);
        env.events().publish(UPGRADE_CANCELLED, admin);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Bytes, Env,
    };

    fn setup() -> (Env, Address, Address, CampaignContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);
        let mut admins = soroban_sdk::Vec::new(&env);
        admins.push_back(admin1.clone());
        admins.push_back(admin2.clone());
        client.initialize(&admins, &2);
        (env, admin1, admin2, client)
    }

    fn name(env: &Env) -> Bytes {
        Bytes::from_slice(env, b"Summer Sale")
    }

    fn desc(env: &Env) -> Bytes {
        Bytes::from_slice(env, b"Earn LYT on every purchase this summer")
    }

    // ── Role management tests ─────────────────────────────────────────────────

    #[test]
    fn test_initial_roles_granted_to_admins() {
        let (env, admin1, admin2, client) = setup();
        assert!(client.has_role_view(&Role::Admin, &admin1));
        assert!(client.has_role_view(&Role::Pauser, &admin1));
        assert!(client.has_role_view(&Role::Admin, &admin2));
        assert!(client.has_role_view(&Role::Pauser, &admin2));
    }

    #[test]
    fn test_grant_role_emits_event() {
        let (env, admin1, _admin2, client) = setup();
        let recorder = Address::generate(&env);
        client.grant_role(&admin1, &Role::Recorder, &recorder);
        assert!(client.has_role_view(&Role::Recorder, &recorder));
    }

    #[test]
    fn test_revoke_role_emits_event() {
        let (env, admin1, _admin2, client) = setup();
        let recorder = Address::generate(&env);
        client.grant_role(&admin1, &Role::Recorder, &recorder);
        client.revoke_role(&admin1, &Role::Recorder, &recorder);
        assert!(!client.has_role_view(&Role::Recorder, &recorder));
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_grant_role_requires_admin() {
        let (env, _admin1, _admin2, client) = setup();
        let non_admin = Address::generate(&env);
        let target = Address::generate(&env);
        client.grant_role(&non_admin, &Role::Recorder, &target);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_revoke_role_requires_admin() {
        let (env, admin1, _admin2, client) = setup();
        let non_admin = Address::generate(&env);
        client.revoke_role(&non_admin, &Role::Pauser, &admin1);
    }

    // ── record_claim access control ───────────────────────────────────────────

    #[test]
    fn test_record_claim_requires_recorder_role() {
        let (env, admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let recorder = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));

        client.grant_role(&admin1, &Role::Recorder, &recorder);
        client.record_claim(&recorder, &id);
        assert_eq!(client.get_campaign(&id).total_claimed, 1);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_record_claim_without_recorder_role_rejected() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let non_recorder = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        client.record_claim(&non_recorder, &id);
    }

    // ── Pause tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_pause_blocks_create_campaign() {
        let (env, admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        client.pause(&admin1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_unpause_allows_create_campaign() {
        let (env, admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        client.pause(&admin1);
        client.unpause(&admin1);
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        assert_eq!(id, 1);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_pause_requires_pauser_role() {
        let (env, _admin1, _admin2, client) = setup();
        let non_pauser = Address::generate(&env);
        client.pause(&non_pauser);
    }

    // ── Campaign CRUD tests ───────────────────────────────────────────────────

    #[test]
    fn test_create_campaign() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        assert_eq!(id, 1);
        let c = client.get_campaign(&id);
        assert_eq!(c.merchant, merchant);
        assert_eq!(c.reward_amount, 100);
        assert!(c.active);
        assert_eq!(c.name, name(&env));
        assert_eq!(c.description, desc(&env));
    }

    #[test]
    fn test_get_campaign_metadata() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        let (n, d) = client.get_campaign_metadata(&id);
        assert_eq!(n, name(&env));
        assert_eq!(d, desc(&env));
    }

    #[test]
    #[should_panic(expected = "name exceeds 64 bytes")]
    fn test_name_too_long() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let long_name = Bytes::from_slice(&env, &[b'x'; 65]);
        client.create_campaign(&merchant, &100, &expiry, &long_name, &desc(&env));
    }

    #[test]
    #[should_panic(expected = "description exceeds 256 bytes")]
    fn test_description_too_long() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let long_desc = Bytes::from_slice(&env, &[b'd'; 257]);
        client.create_campaign(&merchant, &100, &expiry, &name(&env), &long_desc);
    }

    #[test]
    #[should_panic(expected = "expiration must be in the future")]
    fn test_expired_campaign_rejected() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        client.create_campaign(&merchant, &100, &0, &name(&env), &desc(&env));
    }

    #[test]
    fn test_set_active_emits_deactivated_event() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86400;
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        client.set_active(&id, &false);
        assert!(!client.get_campaign(&id).active);

        let events = env.events().all();
        // events[0] = CAM_CRT, events[1] = CAM_DEACT
        assert_eq!(events.len(), 2);
        assert_eq!(
            events.get(1).unwrap(),
            (
                client.address.clone(),
                (CAMPAIGN_DEACTIVATED, symbol_short!("id"), id).into_val(&env),
                merchant.into_val(&env),
            )
        );
    }

    #[test]
    fn test_is_active_after_expiry() {
        let (env, _admin1, _admin2, client) = setup();
        let merchant = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 10;
        let id = client.create_campaign(&merchant, &100, &expiry, &name(&env), &desc(&env));
        assert!(client.is_active(&id));

        env.ledger().with_mut(|l| l.timestamp = expiry + 1);
        assert!(!client.is_active(&id));
    }

    // ── Upgrade flow tests ────────────────────────────────────────────────────

    #[test]
    fn test_upgrade_flow() {
        let (env, admin1, admin2, client) = setup();
        let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

        client.propose_upgrade(&admin1, &wasm_hash);
        client.authorize_upgrade(&admin2);

        env.ledger().with_mut(|l| l.timestamp += TIMELOCK + 1);
        client.execute_upgrade(&admin1);
    }

    #[test]
    #[should_panic(expected = "timelock not met")]
    fn test_upgrade_timelock_enforced() {
        let (env, admin1, admin2, client) = setup();
        let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

        client.propose_upgrade(&admin1, &wasm_hash);
        client.authorize_upgrade(&admin2);
        client.execute_upgrade(&admin1);
    }

    #[test]
    #[should_panic(expected = "insufficient authorizations")]
    fn test_upgrade_threshold_enforced() {
        let (env, admin1, _admin2, client) = setup();
        let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

        client.propose_upgrade(&admin1, &wasm_hash);
        env.ledger().with_mut(|l| l.timestamp += TIMELOCK + 1);
        client.execute_upgrade(&admin1);
    }

    #[test]
    fn test_cancel_upgrade() {
        let (env, admin1, _admin2, client) = setup();
        let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

        client.propose_upgrade(&admin1, &wasm_hash);
        client.cancel_upgrade(&admin1);
        // Can propose again after cancel
        client.propose_upgrade(&admin1, &wasm_hash);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_propose_upgrade_requires_admin() {
        let (env, _admin1, _admin2, client) = setup();
        let non_admin = Address::generate(&env);
        let wasm_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
        client.propose_upgrade(&non_admin, &wasm_hash);
    }
}
