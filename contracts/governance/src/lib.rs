#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Minimum LYT stake required to submit a proposal (in stroops, 7 decimals).
const MIN_STAKE: i128 = 100_000_000; // 10 LYT
/// Voting period: 7 days in seconds.
const VOTING_PERIOD: u64 = 7 * 24 * 60 * 60;
/// Timelock before execution: 48 hours in seconds.
const TIMELOCK: u64 = 48 * 60 * 60;
/// Quorum: 10% of total supply must vote (in basis points).
const QUORUM_BPS: u64 = 1_000; // 10%
/// Passing threshold: >50% of votes in favor (in basis points).
const THRESHOLD_BPS: u64 = 5_000; // 50%

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum ProposalStatus {
    Active,
    Passed,
    Failed,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    /// Short description of the proposal (max 256 bytes).
    pub description: String,
    pub votes_for: u64,
    pub votes_against: u64,
    pub created_at: u64,
    pub voting_ends_at: u64,
    pub executed_at: u64, // 0 = not executed
    pub status: ProposalStatus,
}

#[contracttype]
pub enum DataKey {
    Proposal(u64),
    NextId,
    TokenContract,
    /// Voted(voter, proposal_id) → bool
    Voted(Address, u64),
}

// ── Events ────────────────────────────────────────────────────────────────────

const PROPOSAL_CREATED: Symbol = symbol_short!("GOV_CRT");
const VOTE_CAST: Symbol = symbol_short!("GOV_VOTE");
const PROPOSAL_EXECUTED: Symbol = symbol_short!("GOV_EXEC");
const PROPOSAL_CANCELLED: Symbol = symbol_short!("GOV_CNCL");

// ── Token interface (read-only) ───────────────────────────────────────────────

mod token {
    use soroban_sdk::{contractclient, Address, Env};

    #[contractclient(name = "TokenClient")]
    pub trait Token {
        fn balance(env: Env, addr: Address) -> i128;
        fn total_supply_view(env: Env) -> i128;
    }
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    pub fn initialize(env: Env, token_contract: Address) {
        if env.storage().instance().has(&DataKey::TokenContract) {
            panic!("already initialized");
        }
        env.storage()
            .instance()
            .set(&DataKey::TokenContract, &token_contract);
        env.storage().instance().set(&DataKey::NextId, &1_u64);
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn token_client(env: &Env) -> token::TokenClient {
        let addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap();
        token::TokenClient::new(env, &addr)
    }

    fn next_id(env: &Env) -> u64 {
        env.storage().instance().get(&DataKey::NextId).unwrap_or(1)
    }

    fn bump_id(env: &Env) -> u64 {
        let id = Self::next_id(env);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        id
    }

    fn get_proposal_internal(env: &Env, proposal_id: u64) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("proposal not found")
    }

    // ── Public interface ──────────────────────────────────────────────────────

    /// Submit a new governance proposal.
    /// Proposer must hold at least MIN_STAKE LYT.
    pub fn propose(env: Env, proposer: Address, description: String) -> u64 {
        proposer.require_auth();

        let stake = Self::token_client(&env).balance(&proposer);
        assert!(stake >= MIN_STAKE, "insufficient stake to propose");
        assert!(description.len() <= 256, "description exceeds 256 chars");

        let id = Self::bump_id(&env);
        let now = env.ledger().timestamp();
        let proposal = Proposal {
            id,
            proposer: proposer.clone(),
            description: description.clone(),
            votes_for: 0,
            votes_against: 0,
            created_at: now,
            voting_ends_at: now + VOTING_PERIOD,
            executed_at: 0,
            status: ProposalStatus::Active,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(id), &proposal);

        env.events()
            .publish((PROPOSAL_CREATED, symbol_short!("id"), id), (proposer, description));

        id
    }

    /// Cast a vote on an active proposal.
    /// Each address can vote once per proposal.
    /// Vote weight = voter's LYT balance at time of vote.
    pub fn vote(env: Env, voter: Address, proposal_id: u64, support: bool) {
        voter.require_auth();

        let voted_key = DataKey::Voted(voter.clone(), proposal_id);
        assert!(
            !env.storage().persistent().has(&voted_key),
            "already voted"
        );

        let mut proposal = Self::get_proposal_internal(&env, proposal_id);
        assert!(
            proposal.status == ProposalStatus::Active,
            "proposal not active"
        );
        assert!(
            env.ledger().timestamp() <= proposal.voting_ends_at,
            "voting period ended"
        );

        let weight = Self::token_client(&env).balance(&voter);
        assert!(weight > 0, "no voting power");

        let weight_u64 = weight as u64;
        if support {
            proposal.votes_for = proposal.votes_for.checked_add(weight_u64).expect("overflow");
        } else {
            proposal.votes_against = proposal
                .votes_against
                .checked_add(weight_u64)
                .expect("overflow");
        }

        env.storage().persistent().set(&voted_key, &true);
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (VOTE_CAST, symbol_short!("id"), proposal_id),
            (voter, support, weight_u64),
        );
    }

    /// Finalize a proposal after voting ends and apply quorum/threshold checks.
    /// Must be called before execute to transition status to Passed/Failed.
    pub fn finalize(env: Env, proposal_id: u64) {
        let mut proposal = Self::get_proposal_internal(&env, proposal_id);
        assert!(
            proposal.status == ProposalStatus::Active,
            "proposal not active"
        );
        assert!(
            env.ledger().timestamp() > proposal.voting_ends_at,
            "voting period not ended"
        );

        let total_supply = Self::token_client(&env).total_supply_view();
        let total_votes = proposal.votes_for + proposal.votes_against;

        // Quorum: total votes >= 10% of supply
        let quorum_required = (total_supply as u64) * QUORUM_BPS / 10_000;
        let passed = total_votes >= quorum_required
            && total_votes > 0
            && proposal.votes_for * 10_000 / total_votes > THRESHOLD_BPS;

        proposal.status = if passed {
            ProposalStatus::Passed
        } else {
            ProposalStatus::Failed
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
    }

    /// Execute a passed proposal after the 48-hour timelock.
    /// Execution is on-chain record only; off-chain systems act on the event.
    pub fn execute(env: Env, executor: Address, proposal_id: u64) {
        executor.require_auth();

        let mut proposal = Self::get_proposal_internal(&env, proposal_id);
        assert!(
            proposal.status == ProposalStatus::Passed,
            "proposal not passed"
        );
        assert!(
            env.ledger().timestamp() >= proposal.voting_ends_at + TIMELOCK,
            "timelock not met"
        );

        proposal.status = ProposalStatus::Executed;
        proposal.executed_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((PROPOSAL_EXECUTED, symbol_short!("id"), proposal_id), executor);
    }

    /// Cancel an active proposal. Only the proposer can cancel.
    pub fn cancel(env: Env, proposer: Address, proposal_id: u64) {
        proposer.require_auth();

        let mut proposal = Self::get_proposal_internal(&env, proposal_id);
        assert!(proposal.proposer == proposer, "not proposer");
        assert!(
            proposal.status == ProposalStatus::Active,
            "proposal not active"
        );

        proposal.status = ProposalStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        env.events()
            .publish((PROPOSAL_CANCELLED, symbol_short!("id"), proposal_id), proposer);
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        Self::get_proposal_internal(&env, proposal_id)
    }

    pub fn has_voted(env: Env, voter: Address, proposal_id: u64) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Voted(voter, proposal_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env,
    };

    // Minimal mock token contract for testing
    mod mock_token {
        use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

        #[contracttype]
        pub enum DataKey {
            Balance(Address),
            TotalSupply,
        }

        #[contract]
        pub struct MockToken;

        #[contractimpl]
        impl MockToken {
            pub fn set_balance(env: Env, addr: Address, amount: i128) {
                env.storage()
                    .persistent()
                    .set(&DataKey::Balance(addr), &amount);
            }
            pub fn set_total_supply(env: Env, amount: i128) {
                env.storage()
                    .persistent()
                    .set(&DataKey::TotalSupply, &amount);
            }
            pub fn balance(env: Env, addr: Address) -> i128 {
                env.storage()
                    .persistent()
                    .get(&DataKey::Balance(addr))
                    .unwrap_or(0)
            }
            pub fn total_supply_view(env: Env) -> i128 {
                env.storage()
                    .persistent()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0)
            }
        }
    }

    struct TestSetup<'a> {
        env: Env,
        token: mock_token::MockTokenClient<'a>,
        gov: GovernanceContractClient<'a>,
    }

    fn setup() -> TestSetup<'static> {
        let env = Env::default();
        env.mock_all_auths();

        let token_id = env.register_contract(None, mock_token::MockToken);
        let token = mock_token::MockTokenClient::new(&env, &token_id);

        let gov_id = env.register_contract(None, GovernanceContract);
        let gov = GovernanceContractClient::new(&env, &gov_id);
        gov.initialize(&token_id);

        TestSetup { env, token, gov }
    }

    fn description(env: &Env) -> String {
        String::from_str(env, "Increase reward multiplier cap to 3x")
    }

    #[test]
    fn test_propose_and_vote_and_execute() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        let voter1 = Address::generate(&t.env);
        let voter2 = Address::generate(&t.env);

        // Give proposer enough stake
        t.token.set_balance(&proposer, &MIN_STAKE);
        // Give voters balances
        t.token.set_balance(&voter1, &600);
        t.token.set_balance(&voter2, &300);
        // Total supply = 1900 (proposer + voters); quorum = 190
        t.token.set_total_supply(&1900);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        assert_eq!(pid, 1);

        // Vote for
        t.gov.vote(&voter1, &pid, &true);
        t.gov.vote(&voter2, &pid, &false);

        let p = t.gov.get_proposal(&pid);
        assert_eq!(p.votes_for, 600);
        assert_eq!(p.votes_against, 300);

        // Advance past voting period
        t.env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
        t.gov.finalize(&pid);

        let p = t.gov.get_proposal(&pid);
        assert_eq!(p.status, ProposalStatus::Passed);

        // Advance past timelock
        t.env.ledger().with_mut(|l| l.timestamp += TIMELOCK + 1);
        t.gov.execute(&proposer, &pid);

        let p = t.gov.get_proposal(&pid);
        assert_eq!(p.status, ProposalStatus::Executed);
        assert!(p.executed_at > 0);
    }

    #[test]
    #[should_panic(expected = "insufficient stake to propose")]
    fn test_propose_requires_min_stake() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        t.token.set_balance(&proposer, &(MIN_STAKE - 1));
        t.gov.propose(&proposer, &description(&t.env));
    }

    #[test]
    #[should_panic(expected = "already voted")]
    fn test_double_vote_rejected() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        let voter = Address::generate(&t.env);
        t.token.set_balance(&proposer, &MIN_STAKE);
        t.token.set_balance(&voter, &500);
        t.token.set_total_supply(&1000);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        t.gov.vote(&voter, &pid, &true);
        t.gov.vote(&voter, &pid, &true);
    }

    #[test]
    fn test_proposal_fails_quorum_not_met() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        let voter = Address::generate(&t.env);
        t.token.set_balance(&proposer, &MIN_STAKE);
        // voter has 1 token, total supply 10000 → quorum = 1000, votes = 1 < 1000
        t.token.set_balance(&voter, &1);
        t.token.set_total_supply(&10_000);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        t.gov.vote(&voter, &pid, &true);

        t.env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
        t.gov.finalize(&pid);

        assert_eq!(t.gov.get_proposal(&pid).status, ProposalStatus::Failed);
    }

    #[test]
    fn test_proposal_fails_threshold_not_met() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        let voter_for = Address::generate(&t.env);
        let voter_against = Address::generate(&t.env);
        t.token.set_balance(&proposer, &MIN_STAKE);
        // 400 for, 600 against → 40% for, fails >50% threshold
        t.token.set_balance(&voter_for, &400);
        t.token.set_balance(&voter_against, &600);
        t.token.set_total_supply(&2000);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        t.gov.vote(&voter_for, &pid, &true);
        t.gov.vote(&voter_against, &pid, &false);

        t.env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
        t.gov.finalize(&pid);

        assert_eq!(t.gov.get_proposal(&pid).status, ProposalStatus::Failed);
    }

    #[test]
    #[should_panic(expected = "timelock not met")]
    fn test_execute_before_timelock_rejected() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        let voter = Address::generate(&t.env);
        t.token.set_balance(&proposer, &MIN_STAKE);
        t.token.set_balance(&voter, &600);
        t.token.set_total_supply(&1000);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        t.gov.vote(&voter, &pid, &true);

        t.env.ledger().with_mut(|l| l.timestamp += VOTING_PERIOD + 1);
        t.gov.finalize(&pid);

        // Try to execute immediately (no timelock wait)
        t.gov.execute(&proposer, &pid);
    }

    #[test]
    fn test_cancel_proposal() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        t.token.set_balance(&proposer, &MIN_STAKE);
        t.token.set_total_supply(&MIN_STAKE);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        t.gov.cancel(&proposer, &pid);

        assert_eq!(t.gov.get_proposal(&pid).status, ProposalStatus::Cancelled);
    }

    #[test]
    fn test_events_emitted() {
        let t = setup();
        let proposer = Address::generate(&t.env);
        let voter = Address::generate(&t.env);
        t.token.set_balance(&proposer, &MIN_STAKE);
        t.token.set_balance(&voter, &600);
        t.token.set_total_supply(&1000);

        let pid = t.gov.propose(&proposer, &description(&t.env));
        t.gov.vote(&voter, &pid, &true);

        let events = t.env.events().all();
        // propose event + vote event
        assert!(events.len() >= 2);
    }
}
