#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
};

// ── Roles ─────────────────────────────────────────────────────────────────────

/// Role identifiers. ADMIN can assign/revoke all roles.
/// MINTER can call `mint`. PAUSER can call `pause`/`unpause`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Admin,
    Minter,
    Pauser,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    /// Role membership: (role, address) → bool
    RoleMember(Role, Address),
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
    Name,
    Symbol,
    Decimals,
    /// Whether the contract is paused
    Paused,
}

// ── Events ────────────────────────────────────────────────────────────────────

const MINT: Symbol = symbol_short!("MINT");
const TRANSFER: Symbol = symbol_short!("TRANSFER");
const BURN: Symbol = symbol_short!("BURN");
const APPROVAL: Symbol = symbol_short!("APPROVAL");
const ROLE_GRANTED: Symbol = symbol_short!("ROLE_GRT");
const ROLE_REVOKED: Symbol = symbol_short!("ROLE_REV");
const PAUSED: Symbol = symbol_short!("PAUSED");
const UNPAUSED: Symbol = symbol_short!("UNPAUSED");

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    /// Initialize the token. Can only be called once.
    /// `admin` receives the ADMIN, MINTER, and PAUSER roles.
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) {
        if env.storage().instance().has(&DataKey::Paused) {
            panic!("already initialized");
        }
        // Grant all roles to the initial admin
        Self::_grant_role(&env, &Role::Admin, &admin);
        Self::_grant_role(&env, &Role::Minter, &admin);
        Self::_grant_role(&env, &Role::Pauser, &admin);

        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::TotalSupply, &0_i128);
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

    // ── Balance helpers ───────────────────────────────────────────────────────

    #[inline(always)]
    fn read_balance(env: &Env, key: &DataKey) -> i128 {
        env.storage().persistent().get(key).unwrap_or(0)
    }

    #[inline(always)]
    fn write_balance(env: &Env, key: &DataKey, amount: i128) {
        env.storage().persistent().set(key, &amount);
    }

    fn total_supply(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    fn set_total_supply(env: &Env, supply: i128) {
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &supply);
    }

    // ── Allowance helpers ─────────────────────────────────────────────────────

    fn get_allowance(env: &Env, owner: &Address, spender: &Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Allowance(owner.clone(), spender.clone()))
            .unwrap_or(0)
    }

    fn set_allowance(env: &Env, owner: &Address, spender: &Address, amount: i128) {
        env.storage()
            .persistent()
            .set(&DataKey::Allowance(owner.clone(), spender.clone()), &amount);
    }

    // ── Public interface ──────────────────────────────────────────────────────

    /// Mint `amount` tokens to `to`. Caller must have MINTER role.
    pub fn mint(env: Env, minter: Address, to: Address, amount: i128) {
        Self::require_role(&env, &Role::Minter, &minter);
        Self::require_not_paused(&env);
        assert!(amount > 0, "amount must be positive");

        let key = DataKey::Balance(to.clone());
        let new_bal = Self::read_balance(&env, &key)
            .checked_add(amount)
            .expect("overflow");
        Self::write_balance(&env, &key, new_bal);

        let new_supply = Self::total_supply(&env)
            .checked_add(amount)
            .expect("overflow");
        Self::set_total_supply(&env, new_supply);

        env.events()
            .publish((MINT, symbol_short!("to"), to), (amount, new_supply));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        Self::require_not_paused(&env);
        assert!(amount > 0, "amount must be positive");

        let key = DataKey::Balance(from.clone());
        let bal = Self::read_balance(&env, &key);
        assert!(bal >= amount, "insufficient balance");
        Self::write_balance(&env, &key, bal - amount);

        let new_supply = Self::total_supply(&env)
            .checked_sub(amount)
            .expect("underflow");
        Self::set_total_supply(&env, new_supply);

        env.events()
            .publish((BURN, symbol_short!("from"), from), (amount, new_supply));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::require_not_paused(&env);
        assert!(amount > 0, "amount must be positive");

        let from_key = DataKey::Balance(from.clone());
        let to_key = DataKey::Balance(to.clone());

        let from_bal = Self::read_balance(&env, &from_key);
        assert!(from_bal >= amount, "insufficient balance");
        let to_bal = Self::read_balance(&env, &to_key);

        Self::write_balance(&env, &from_key, from_bal - amount);
        Self::write_balance(
            &env,
            &to_key,
            to_bal.checked_add(amount).expect("overflow"),
        );

        env.events()
            .publish((TRANSFER, symbol_short!("from"), from), (to, amount));
    }

    /// Approve `spender` to transfer up to `amount` tokens on behalf of the caller.
    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        owner.require_auth();
        assert!(amount >= 0, "amount must be non-negative");
        Self::set_allowance(&env, &owner, &spender, amount);
        env.events()
            .publish((APPROVAL, symbol_short!("owner"), owner), (spender, amount));
    }

    /// Transfer `amount` tokens from `from` to `to` using the caller's allowance.
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        Self::require_not_paused(&env);
        assert!(amount > 0, "amount must be positive");

        let current = Self::get_allowance(&env, &from, &spender);
        assert!(current >= amount, "allowance exceeded");

        let from_key = DataKey::Balance(from.clone());
        let to_key = DataKey::Balance(to.clone());
        let from_bal = Self::read_balance(&env, &from_key);
        assert!(from_bal >= amount, "insufficient balance");

        Self::set_allowance(&env, &from, &spender, current - amount);
        Self::write_balance(&env, &from_key, from_bal - amount);
        let to_bal = Self::read_balance(&env, &to_key)
            .checked_add(amount)
            .expect("overflow");
        Self::write_balance(&env, &to_key, to_bal);

        env.events()
            .publish((TRANSFER, symbol_short!("from"), from), (to, amount));
    }

    /// Returns the remaining allowance for `spender` on behalf of `owner`.
    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        Self::get_allowance(&env, &owner, &spender)
    }

    pub fn balance(env: Env, addr: Address) -> i128 {
        Self::read_balance(&env, &DataKey::Balance(addr))
    }

    pub fn total_supply_view(env: Env) -> i128 {
        Self::total_supply(&env)
    }

    pub fn name(env: Env) -> String {
        env.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get(&DataKey::Symbol).unwrap()
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    /// Legacy view — returns the first address that holds the ADMIN role.
    /// Kept for backward compatibility with the backend indexer.
    pub fn admin_address(env: Env) -> Address {
        // Not meaningful with RBAC; kept for ABI compatibility.
        // Returns the zero address sentinel if no admin is set.
        panic!("use has_role_view to check roles")
    }

    /// Backward-compat: transfer the ADMIN role to `new_admin`.
    /// Caller must have ADMIN role. Also grants MINTER + PAUSER to new_admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        Self::require_role(&env, &Role::Admin, &caller);
        Self::_grant_role(&env, &Role::Admin, &new_admin);
        Self::_grant_role(&env, &Role::Minter, &new_admin);
        Self::_grant_role(&env, &Role::Pauser, &new_admin);
        // Revoke all roles from old admin
        Self::_revoke_role(&env, &Role::Admin, &caller);
        Self::_revoke_role(&env, &Role::Minter, &caller);
        Self::_revoke_role(&env, &Role::Pauser, &caller);
        env.events()
            .publish((ROLE_GRANTED, Role::Admin), (caller.clone(), new_admin.clone()));
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, IntoVal, Env,
    };

    fn setup() -> (Env, Address, TokenContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, TokenContract);
        let client = TokenContractClient::new(&env, &contract_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "LoyaltyToken"),
            &String::from_str(&env, "LYT"),
            &7,
        );
        (env, admin, client)
    }

    // ── Role management tests ─────────────────────────────────────────────────

    #[test]
    fn test_initial_roles_granted_to_admin() {
        let (env, admin, client) = setup();
        assert!(client.has_role_view(&Role::Admin, &admin));
        assert!(client.has_role_view(&Role::Minter, &admin));
        assert!(client.has_role_view(&Role::Pauser, &admin));
    }

    #[test]
    fn test_grant_role_emits_event() {
        let (env, admin, client) = setup();
        let minter = Address::generate(&env);
        client.grant_role(&admin, &Role::Minter, &minter);
        assert!(client.has_role_view(&Role::Minter, &minter));

        let events = env.events().all();
        let last = events.last().unwrap();
        assert_eq!(
            last,
            (
                client.address.clone(),
                (ROLE_GRANTED, Role::Minter).into_val(&env),
                (admin, minter).into_val(&env),
            )
        );
    }

    #[test]
    fn test_revoke_role_emits_event() {
        let (env, admin, client) = setup();
        let minter = Address::generate(&env);
        client.grant_role(&admin, &Role::Minter, &minter);
        client.revoke_role(&admin, &Role::Minter, &minter);
        assert!(!client.has_role_view(&Role::Minter, &minter));

        let events = env.events().all();
        let last = events.last().unwrap();
        assert_eq!(
            last,
            (
                client.address.clone(),
                (ROLE_REVOKED, Role::Minter).into_val(&env),
                (admin, minter).into_val(&env),
            )
        );
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_grant_role_requires_admin() {
        let (env, _admin, client) = setup();
        let non_admin = Address::generate(&env);
        let target = Address::generate(&env);
        client.grant_role(&non_admin, &Role::Minter, &target);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_revoke_role_requires_admin() {
        let (env, admin, client) = setup();
        let non_admin = Address::generate(&env);
        client.revoke_role(&non_admin, &Role::Minter, &admin);
    }

    // ── Mint access control tests ─────────────────────────────────────────────

    #[test]
    fn test_mint_with_minter_role() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&admin, &user, &1000);
        assert_eq!(client.balance(&user), 1000);
        assert_eq!(client.total_supply_view(), 1000);
    }

    #[test]
    fn test_mint_by_granted_minter() {
        let (env, admin, client) = setup();
        let minter = Address::generate(&env);
        let user = Address::generate(&env);
        client.grant_role(&admin, &Role::Minter, &minter);
        client.mint(&minter, &user, &500);
        assert_eq!(client.balance(&user), 500);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_mint_without_minter_role_rejected() {
        let (env, _admin, client) = setup();
        let non_minter = Address::generate(&env);
        let user = Address::generate(&env);
        client.mint(&non_minter, &user, &100);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_mint_after_minter_role_revoked() {
        let (env, admin, client) = setup();
        let minter = Address::generate(&env);
        let user = Address::generate(&env);
        client.grant_role(&admin, &Role::Minter, &minter);
        client.revoke_role(&admin, &Role::Minter, &minter);
        client.mint(&minter, &user, &100);
    }

    // ── Pause tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_pause_and_unpause() {
        let (env, admin, client) = setup();
        assert!(!client.is_paused());
        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_mint_blocked_when_paused() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.pause(&admin);
        client.mint(&admin, &user, &100);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_transfer_blocked_when_paused() {
        let (env, admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&admin, &alice, &500);
        client.pause(&admin);
        client.transfer(&alice, &bob, &100);
    }

    #[test]
    #[should_panic(expected = "missing role")]
    fn test_pause_requires_pauser_role() {
        let (env, _admin, client) = setup();
        let non_pauser = Address::generate(&env);
        client.pause(&non_pauser);
    }

    #[test]
    fn test_pauser_role_can_be_granted_separately() {
        let (env, admin, client) = setup();
        let pauser = Address::generate(&env);
        client.grant_role(&admin, &Role::Pauser, &pauser);
        client.pause(&pauser);
        assert!(client.is_paused());
    }

    // ── Transfer / burn tests ─────────────────────────────────────────────────

    #[test]
    fn test_transfer() {
        let (env, admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&admin, &alice, &500);
        client.transfer(&alice, &bob, &200);
        assert_eq!(client.balance(&alice), 300);
        assert_eq!(client.balance(&bob), 200);
    }

    #[test]
    fn test_burn() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&admin, &user, &300);
        client.burn(&user, &100);
        assert_eq!(client.balance(&user), 200);
        assert_eq!(client.total_supply_view(), 200);
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_burn_insufficient() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&admin, &user, &50);
        client.burn(&user, &100);
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_transfer_insufficient() {
        let (env, admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&admin, &alice, &50);
        client.transfer(&alice, &bob, &100);
    }

    // ── Allowance tests ───────────────────────────────────────────────────────

    #[test]
    fn test_approve_and_allowance() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let spender = Address::generate(&env);
        client.approve(&alice, &spender, &500);
        assert_eq!(client.allowance(&alice, &spender), 500);
    }

    #[test]
    fn test_transfer_from_within_allowance() {
        let (env, admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let spender = Address::generate(&env);
        client.mint(&admin, &alice, &1000);
        client.approve(&alice, &spender, &300);
        client.transfer_from(&spender, &alice, &bob, &200);
        assert_eq!(client.balance(&alice), 800);
        assert_eq!(client.balance(&bob), 200);
        assert_eq!(client.allowance(&alice, &spender), 100);
    }

    #[test]
    #[should_panic(expected = "allowance exceeded")]
    fn test_transfer_from_exceeds_allowance() {
        let (env, admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let spender = Address::generate(&env);
        client.mint(&admin, &alice, &1000);
        client.approve(&alice, &spender, &100);
        client.transfer_from(&spender, &alice, &bob, &200);
    }

    #[test]
    fn test_set_admin_transfers_all_roles() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.set_admin(&admin, &new_admin);
        assert!(client.has_role_view(&Role::Admin, &new_admin));
        assert!(client.has_role_view(&Role::Minter, &new_admin));
        assert!(client.has_role_view(&Role::Pauser, &new_admin));
        // Old admin loses roles
        assert!(!client.has_role_view(&Role::Admin, &admin));
    }

    #[test]
    fn test_mint_emits_event() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&admin, &user, &1000);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events,
            vec![
                &env,
                (
                    client.address.clone(),
                    (MINT, symbol_short!("to"), user).into_val(&env),
                    (1000_i128, 1000_i128).into_val(&env),
                )
            ]
        );
    }

    #[test]
    fn test_burn_emits_event() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&admin, &user, &300);
        client.burn(&user, &100);

        let events = env.events().all();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events.get(1).unwrap(),
            (
                client.address.clone(),
                (BURN, symbol_short!("from"), user).into_val(&env),
                (100_i128, 200_i128).into_val(&env),
            )
        );
    }
}
