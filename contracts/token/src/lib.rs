#![no_std]
#![allow(deprecated)] // soroban_sdk::events::Events::publish is deprecated in 25.x; kept for API compatibility

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
};

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    Balance(Address),
    Allowance(Address, Address), // (owner, spender)
    TotalSupply,
    Name,
    Symbol,
    Decimals,
}

// ── Events ────────────────────────────────────────────────────────────────────

const MINT: Symbol = symbol_short!("MINT");
const TRANSFER: Symbol = symbol_short!("TRANSFER");
const BURN: Symbol = symbol_short!("BURN");
const APPROVAL: Symbol = symbol_short!("APPROVAL");

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    /// Initialize the token. Can only be called once.
    pub fn initialize(env: Env, admin: Address, name: String, symbol: String, decimals: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::TotalSupply, &0_i128);
    }

    // ── Admin helpers ─────────────────────────────────────────────────────────

    fn admin(env: &Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    fn require_admin(env: &Env) {
        Self::admin(env).require_auth();
    }

    // ── Balance helpers ───────────────────────────────────────────────────────

    // Read balance for a pre-built key, avoiding a second key construction.
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
        env.storage().instance().set(&DataKey::TotalSupply, &supply);
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

    pub fn mint(env: Env, to: Address, amount: i128) {
        Self::require_admin(&env);
        assert!(amount > 0, "amount must be positive");

        // Build key once; reuse for both read and write — avoids a second
        // Address clone that the old balance_of/set_balance pair incurred.
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
        assert!(amount > 0, "amount must be positive");

        let key = DataKey::Balance(from.clone());
        let bal = Self::read_balance(&env, &key);
        assert!(bal >= amount, "insufficient balance");
        Self::write_balance(&env, &key, bal - amount);

        // Use checked_sub to guard against total_supply underflow.
        let new_supply = Self::total_supply(&env)
            .checked_sub(amount)
            .expect("underflow");
        Self::set_total_supply(&env, new_supply);

        env.events()
            .publish((BURN, symbol_short!("from"), from), (amount, new_supply));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        assert!(amount > 0, "amount must be positive");

        // Build both keys up front so each address is cloned exactly once.
        let from_key = DataKey::Balance(from.clone());
        let to_key = DataKey::Balance(to.clone());

        // Read both balances before writing either — keeps reads and writes
        // clearly separated and avoids any accidental double-read.
        let from_bal = Self::read_balance(&env, &from_key);
        assert!(from_bal >= amount, "insufficient balance");
        let to_bal = Self::read_balance(&env, &to_key);

        Self::write_balance(&env, &from_key, from_bal - amount);
        Self::write_balance(&env, &to_key, to_bal.checked_add(amount).expect("overflow"));

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

    pub fn admin_address(env: Env) -> Address {
        Self::admin(&env)
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

    pub fn set_admin(env: Env, new_admin: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, Env, IntoVal,
    };

    fn setup() -> (Env, Address, TokenContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(TokenContract, ());
        let client = TokenContractClient::new(&env, &contract_id);
        client.initialize(
            &admin,
            &String::from_str(&env, "LoyaltyToken"),
            &String::from_str(&env, "LYT"),
            &7,
        );
        (env, admin, client)
    }

    #[test]
    fn test_mint_and_balance() {
        let (env, _admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&user, &1000);
        assert_eq!(client.balance(&user), 1000);
        assert_eq!(client.total_supply_view(), 1000);

        let events = env.events().all();
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
    fn test_transfer() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &500);
        client.transfer(&alice, &bob, &200);
        assert_eq!(client.balance(&alice), 300);
        assert_eq!(client.balance(&bob), 200);
    }

    #[test]
    fn test_burn() {
        let (env, _admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&user, &300);
        client.burn(&user, &100);
        assert_eq!(client.balance(&user), 200);
        assert_eq!(client.total_supply_view(), 200);

        let events = env.events().all();
        // events[0] = mint, events[1] = burn — compare the full list
        assert_eq!(
            events,
            vec![
                &env,
                (
                    client.address.clone(),
                    (MINT, symbol_short!("to"), user.clone()).into_val(&env),
                    (300_i128, 300_i128).into_val(&env),
                ),
                (
                    client.address.clone(),
                    (BURN, symbol_short!("from"), user).into_val(&env),
                    (100_i128, 200_i128).into_val(&env),
                ),
            ]
        );
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_burn_insufficient() {
        let (env, _admin, client) = setup();
        let user = Address::generate(&env);
        client.mint(&user, &50);
        client.burn(&user, &100);
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_transfer_insufficient() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &50);
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
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let spender = Address::generate(&env);
        client.mint(&alice, &1000);
        client.approve(&alice, &spender, &300);
        client.transfer_from(&spender, &alice, &bob, &200);
        assert_eq!(client.balance(&alice), 800);
        assert_eq!(client.balance(&bob), 200);
        assert_eq!(client.allowance(&alice, &spender), 100);
    }

    #[test]
    #[should_panic(expected = "allowance exceeded")]
    fn test_transfer_from_exceeds_allowance() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let spender = Address::generate(&env);
        client.mint(&alice, &1000);
        client.approve(&alice, &spender, &100);
        client.transfer_from(&spender, &alice, &bob, &200);
    }

    #[test]
    fn test_approve_overwrite() {
        let (env, _admin, client) = setup();
        let alice = Address::generate(&env);
        let spender = Address::generate(&env);
        client.approve(&alice, &spender, &500);
        client.approve(&alice, &spender, &100);
        assert_eq!(client.allowance(&alice, &spender), 100);
    }
}
