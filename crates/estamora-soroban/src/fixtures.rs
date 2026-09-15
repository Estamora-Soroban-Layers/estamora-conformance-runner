//! Fixture contracts compiled into the test binary.
//!
//! These are not examples and are not a reference implementation. They exist so
//! that the runner's execution path can be tested against contracts whose
//! behaviour is known exactly, including the behaviours it must detect: a missing
//! authorization check, an event emitted before a check, a mutation after a
//! refusal. Each one is deliberately wrong in exactly one way, so a test that
//! fails points at one requirement rather than at a fixture.

// The signatures below take `Env` and `Address` by value because the Soroban
// contract ABI requires it: an exported contract function's parameters *are* the
// wire format of the call, and the host constructs them from the arguments it
// received. Clippy's advice to borrow them would produce a function the host
// cannot call, so the lint is disabled here with that reason rather than worked
// around with a signature that would change the contract's interface.
#![allow(
    clippy::needless_pass_by_value,
    clippy::must_use_candidate,
    reason = "contract function signatures are the Soroban ABI, not a choice"
)]

use soroban_sdk::{Address, Env, contract, contractevent, contractimpl, contracttype};

/// Keys of the fixture's ledger storage.
#[derive(Debug, Clone)]
#[contracttype]
pub enum Key {
    /// The balance held by an account.
    Balance(Address),
}

/// A token-shaped fixture that behaves as a profile would require.
#[derive(Debug, Clone)]
#[contract]
pub struct ConformingToken;

/// An event a non-conforming contract emits before checking anything.
///
/// The fixed topic is declared explicitly for the same reason as
/// [`TransferEvent`]'s: the macro otherwise derives the name from the struct,
/// which produces `sneaky_event` and would not be the name a profile names.
#[derive(Debug, Clone)]
#[contractevent(topics = ["sneaky"])]
pub struct SneakyEvent {
    /// The account named in the premature event.
    #[topic]
    pub who: Address,
}

/// The event a conforming transfer emits.
///
/// The event name is declared rather than derived. The `#[contractevent]` macro
/// names an event after its struct — `TransferEvent` becomes `transfer_event` —
/// and SEP-41 requires the name in the first topic to be exactly `transfer`.
/// A profile can only state a name-based event requirement because a conforming
/// contract declares the name; the runner must therefore match the declared
/// topic and never the Rust type name.
#[derive(Debug, Clone)]
#[contractevent(topics = ["transfer"])]
pub struct TransferEvent {
    /// The account debited.
    #[topic]
    pub from: Address,
    /// The account credited.
    #[topic]
    pub to: Address,
    /// The amount moved.
    pub amount: i128,
}

fn balance_of(env: &Env, who: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&Key::Balance(who.clone()))
        .unwrap_or(0)
}

fn set_balance(env: &Env, who: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&Key::Balance(who.clone()), &amount);
}

#[contractimpl]
impl ConformingToken {
    /// Moves `amount` from `from` to `to`, requiring `from`'s authorization.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient. Aborting is the
    /// documented idiom for rejection in Soroban, and it is why the runner must
    /// read an abort as a refusal rather than as an environment failure.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let available = balance_of(&env, &from);
        assert!(available >= amount, "insufficient balance");
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
        TransferEvent { from, to, amount }.publish(&env);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Credits `who` without requiring authorization. Used to set a fixture up.
    pub fn mint(env: Env, who: Address, amount: i128) {
        set_balance(&env, &who, balance_of(&env, &who) + amount);
    }

    /// Returns a fixed value, so that a test can prove a value round-trips.
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    /// Always aborts, to prove a bare panic is observed as a refusal.
    ///
    /// # Panics
    ///
    /// Always.
    pub fn always_refuses(_env: Env) -> i128 {
        panic!("refused")
    }

    /// Emits an event and then aborts, which is the defect a "must not emit on
    /// failure" requirement exists to catch.
    ///
    /// # Panics
    ///
    /// Always, after emitting.
    pub fn emits_then_refuses(env: Env, who: Address) {
        SneakyEvent { who }.publish(&env);
        panic!("refused after emitting");
    }

    /// Mutates the caller's balance before checking anything, then aborts when
    /// `should_fail` is set. The defect a "refusal must not mutate state"
    /// requirement exists to catch.
    ///
    /// # Panics
    ///
    /// Aborts when `should_fail` is true, after mutating.
    pub fn mutates_then_refuses(env: Env, who: Address, amount: i128, should_fail: bool) {
        set_balance(&env, &who, balance_of(&env, &who) + amount);
        assert!(!should_fail, "refused after mutating");
    }

    /// Returns without requiring the authorization the profile requires. The
    /// defect a "must require authorization" requirement exists to catch.
    pub fn transfer_unchecked(env: Env, from: Address, to: Address, amount: i128) {
        let available = balance_of(&env, &from);
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
    }
}
