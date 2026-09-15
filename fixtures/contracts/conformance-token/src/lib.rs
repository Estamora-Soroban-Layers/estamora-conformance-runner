//! The contracts Estamora's own tests and examples measure.
//!
//! These are not examples and are not a reference implementation. They exist so that
//! the runner's execution path can be exercised against contracts whose behaviour is
//! known exactly — including the behaviours it must detect. Every contract after
//! [`ConformingToken`] is deliberately wrong in **exactly one way**, so a test that
//! fails points at one requirement rather than at a fixture, and a report that says
//! `NON_CONFORMANT` can be traced to a single named defect.
//!
//! # Why they are a workspace member rather than a module of the runner
//!
//! `fixtures/` holds contracts and nothing else, and a fixture compiled as its own
//! crate is compiled by the same SDK and the same toolchain a user's contract is. A
//! fixture that lived inside the runner could accidentally depend on the runner and
//! would then exercise a deployment route a real contract never takes.
//!
//! # Selecting one
//!
//! [`Defect`] names what is wrong, and [`deploy`] registers the matching contract.
//! Tests and examples name a defect rather than a type, so that adding a defect
//! cannot silently change which contract an existing test measures.

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
// The Soroban macros generate items — storage accessors, event publishers — that
// cannot carry documentation, and a fixture is not a published API.
#![allow(
    missing_docs,
    reason = "the contract macros generate items that cannot carry documentation"
)]

use soroban_sdk::testutils::Register as _;
use soroban_sdk::{Address, Env, contract, contractevent, contractimpl, contracttype};

/// Keys of a fixture's ledger storage.
#[derive(Debug, Clone)]
#[contracttype]
pub enum Key {
    /// The balance held by an account.
    Balance(Address),
    /// An allowance granted by one account to another.
    Allowance(Address, Address),
}

/// An event a non-conforming contract emits before checking anything.
///
/// The topic is declared explicitly for the same reason as [`TransferEvent`]'s: the
/// macro otherwise derives the name from the struct, producing `sneaky_event`, which
/// is not a name a profile would ever declare.
#[derive(Debug, Clone)]
#[contractevent(topics = ["sneaky"])]
pub struct SneakyEvent {
    /// The account named in the premature event.
    #[topic]
    pub who: Address,
}

/// The event a conforming transfer emits.
///
/// The name is declared rather than derived. The `#[contractevent]` macro names an
/// event after its struct — `TransferEvent` becomes `transfer_event` — and SEP-41
/// requires the first topic to be exactly `transfer`. A profile can only state a
/// name-based requirement because a conforming contract declares the name, so the
/// runner matches the declared topic and never a Rust type name.
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

/// Which way a fixture is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defect {
    /// Nothing. The contract behaves as the profile requires.
    None,
    /// `transfer` never calls `require_auth`, so anyone may move anyone's value.
    SkipsAuthorization,
    /// `transfer` emits its event before checking the balance, so a refused call
    /// leaves a success event behind. The defect a "must not emit on failure"
    /// requirement exists to catch.
    EmitsBeforeChecking,
    /// `transfer` writes to storage before checking the balance. The defect a
    /// "refusal must not mutate state" requirement exists to catch.
    MutatesBeforeRefusing,
    /// `transfer` emits two transfer events for one movement, so a consumer cannot
    /// tell which is real.
    DoubleEmits,
    /// `transfer` emits an amount one unit larger than the movement it performed.
    WrongEventAmount,
    /// `transfer` permits an overdraft, producing a negative balance instead of a
    /// refusal.
    AllowsOverdraft,
    /// The interface omits `decimals`, which SEP-41 requires.
    MissingDecimals,
}

impl Defect {
    /// Every defect, so that a caller can iterate over the fixture set.
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::SkipsAuthorization,
        Self::EmitsBeforeChecking,
        Self::MutatesBeforeRefusing,
        Self::DoubleEmits,
        Self::WrongEventAmount,
        Self::AllowsOverdraft,
        Self::MissingDecimals,
    ];

    /// The stable name a caller selects it by.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SkipsAuthorization => "skips-authorization",
            Self::EmitsBeforeChecking => "emits-before-checking",
            Self::MutatesBeforeRefusing => "mutates-before-refusing",
            Self::DoubleEmits => "double-emits",
            Self::WrongEventAmount => "wrong-event-amount",
            Self::AllowsOverdraft => "allows-overdraft",
            Self::MissingDecimals => "missing-decimals",
        }
    }

    /// The defect with this name, where there is one.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|defect| defect.as_str() == name)
    }

    /// What the defect violates, so that a report and a test can say why a fixture
    /// was chosen.
    #[must_use]
    pub const fn violates(self) -> &'static str {
        match self {
            Self::None => "nothing: this contract is the conforming case",
            Self::SkipsAuthorization => "the requirement that a transfer be authorized",
            Self::EmitsBeforeChecking => {
                "the requirement that a refused call emit no success event"
            },
            Self::MutatesBeforeRefusing => {
                "the requirement that a refused call leave state unchanged"
            },
            Self::DoubleEmits => "the transfer event's cardinality",
            Self::WrongEventAmount => "the requirement that the event's amount equal the movement",
            Self::AllowsOverdraft => "the requirement that a transfer beyond the balance fail",
            Self::MissingDecimals => "the requirement that the interface expose `decimals`",
        }
    }

    /// Whether this fixture publishes the method a profile requires it to.
    #[must_use]
    pub const fn exposes_decimals(self) -> bool {
        !matches!(self, Self::MissingDecimals)
    }
}

/// A token that behaves as the SEP-41 profile requires.
#[derive(Debug, Clone)]
#[contract]
pub struct ConformingToken;

/// A token that moves value without requiring authorization.
#[derive(Debug, Clone)]
#[contract]
pub struct SkipsAuthorization;

/// A token that emits its success event before checking the balance.
#[derive(Debug, Clone)]
#[contract]
pub struct EmitsBeforeChecking;

/// A token that writes to storage before checking the balance.
#[derive(Debug, Clone)]
#[contract]
pub struct MutatesBeforeRefusing;

/// A token that emits two transfer events for one movement.
#[derive(Debug, Clone)]
#[contract]
pub struct DoubleEmits;

/// A token whose event states an amount larger than the one it moved.
#[derive(Debug, Clone)]
#[contract]
pub struct WrongEventAmount;

/// A token that permits an overdraft.
#[derive(Debug, Clone)]
#[contract]
pub struct AllowsOverdraft;

/// A token whose interface omits `decimals`.
#[derive(Debug, Clone)]
#[contract]
pub struct MissingDecimals;

/// Registers the contract that is wrong in the way `defect` names.
#[must_use]
pub fn deploy(env: &Env, defect: Defect) -> Address {
    match defect {
        Defect::None => ConformingToken.register(env, None, ()),
        Defect::SkipsAuthorization => SkipsAuthorization.register(env, None, ()),
        Defect::EmitsBeforeChecking => EmitsBeforeChecking.register(env, None, ()),
        Defect::MutatesBeforeRefusing => MutatesBeforeRefusing.register(env, None, ()),
        Defect::DoubleEmits => DoubleEmits.register(env, None, ()),
        Defect::WrongEventAmount => WrongEventAmount.register(env, None, ()),
        Defect::AllowsOverdraft => AllowsOverdraft.register(env, None, ()),
        Defect::MissingDecimals => MissingDecimals.register(env, None, ()),
    }
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

fn allowance_of(env: &Env, from: &Address, spender: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&Key::Allowance(from.clone(), spender.clone()))
        .unwrap_or(0)
}

fn set_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&Key::Allowance(from.clone(), spender.clone()), &amount);
}

/// Generates a fixture's setup entry points as a whole `#[contractimpl]` block.
///
/// The block is generated rather than the functions inside one, and that is not a
/// style choice. `#[contractimpl]` is an attribute macro, so it sees its input
/// *before* a macro call inside the block is expanded and would therefore never
/// export the functions. Expanding at the item level is what makes them part of the
/// contract's interface.
///
/// These entry points are not part of SEP-41. A profile must not require them; they
/// exist because the standard exposes no way to mint, and a vector's opening balances
/// have to come from somewhere. A fixture declares them under a `fixture_` prefix so
/// that a profile author cannot mistake them for the interface under test.
macro_rules! fixture_setup {
    ($contract:ident) => {
        #[contractimpl]
        impl $contract {
            /// Credits `who` without requiring authorization.
            pub fn fixture_mint(env: Env, who: Address, amount: i128) {
                set_balance(&env, &who, balance_of(&env, &who) + amount);
            }

            /// Grants `spender` an allowance over `from`'s balance.
            pub fn fixture_approve(env: Env, from: Address, spender: Address, amount: i128) {
                set_allowance(&env, &from, &spender, amount);
            }
        }
    };
}

#[contractimpl]
impl ConformingToken {
    /// Moves `amount` from `from` to `to`, requiring `from`'s authorization.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient. Aborting is the documented
    /// idiom for rejection in Soroban, and it is why the runner must read an abort as
    /// a refusal rather than as an environment failure.
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

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    /// Returns the total supply, which the runner reads as a cross-check.
    pub fn total_supply(env: Env) -> i128 {
        // A fixture has no supply register, so the value returned here is a constant
        // and the conservation requirements are checked over the balance set instead.
        // Stated rather than left to a reader to discover.
        let _ = &env;
        0
    }

    /// Always aborts, to prove a bare panic is observed as a refusal.
    ///
    /// # Panics
    ///
    /// Always.
    pub fn always_refuses(_env: Env) -> i128 {
        panic!("refused")
    }
}

fixture_setup!(ConformingToken);

#[contractimpl]
impl SkipsAuthorization {
    /// Moves value without requiring authorization: the defect.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let available = balance_of(&env, &from);
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
        TransferEvent { from, to, amount }.publish(&env);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

fixture_setup!(SkipsAuthorization);

#[contractimpl]
impl EmitsBeforeChecking {
    /// Emits the success event, then checks the balance: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient, after emitting.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        TransferEvent {
            from: from.clone(),
            to: to.clone(),
            amount,
        }
        .publish(&env);
        let available = balance_of(&env, &from);
        assert!(available >= amount, "insufficient balance");
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

fixture_setup!(EmitsBeforeChecking);

#[contractimpl]
impl MutatesBeforeRefusing {
    /// Debits the sender before checking that it holds enough: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient, after mutating.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let available = balance_of(&env, &from);
        set_balance(&env, &from, available - amount);
        assert!(available >= amount, "insufficient balance");
        set_balance(&env, &to, balance_of(&env, &to) + amount);
        TransferEvent { from, to, amount }.publish(&env);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

fixture_setup!(MutatesBeforeRefusing);

#[contractimpl]
impl DoubleEmits {
    /// Emits the transfer event twice for one movement: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let available = balance_of(&env, &from);
        assert!(available >= amount, "insufficient balance");
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
        TransferEvent {
            from: from.clone(),
            to: to.clone(),
            amount,
        }
        .publish(&env);
        TransferEvent { from, to, amount }.publish(&env);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

fixture_setup!(DoubleEmits);

#[contractimpl]
impl WrongEventAmount {
    /// Moves `amount` but states `amount + 1` in the event: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let available = balance_of(&env, &from);
        assert!(available >= amount, "insufficient balance");
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
        TransferEvent {
            from,
            to,
            amount: amount + 1,
        }
        .publish(&env);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

fixture_setup!(WrongEventAmount);

#[contractimpl]
impl AllowsOverdraft {
    /// Permits an overdraft instead of refusing: the defect.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let available = balance_of(&env, &from);
        set_balance(&env, &from, available - amount);
        set_balance(&env, &to, balance_of(&env, &to) + amount);
        TransferEvent { from, to, amount }.publish(&env);
    }

    /// Returns the balance held by `who`.
    pub fn balance(env: Env, who: Address) -> i128 {
        balance_of(&env, &who)
    }

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    /// Returns the number of decimals the token uses.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

fixture_setup!(AllowsOverdraft);

#[contractimpl]
impl MissingDecimals {
    /// Moves `amount` from `from` to `to`, requiring `from`'s authorization.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
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

    /// Returns the allowance `from` granted `spender`.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        allowance_of(&env, &from, &spender)
    }

    // `decimals` is deliberately absent. The interface dimension is what notices:
    // nothing about calling a method that does not exist distinguishes it from a
    // contract that refuses every call.
}

fixture_setup!(MissingDecimals);

/// The methods every fixture exposes except where a defect removes one.
///
/// Declared as data rather than derived from the Rust types, because the runner's
/// interface inspection reads a compiled contract's spec section and has no access to
/// these types. A fixture that declared its interface by being inspected would be
/// testing the inspection rather than the profile.
pub const CONFORMING_METHODS: &[(&str, &[&str], Option<&str>)] = &[
    ("transfer", &["address", "address", "i128"], None),
    ("balance", &["address"], Some("i128")),
    ("allowance", &["address", "address"], Some("i128")),
    ("decimals", &[], Some("u32")),
    ("total_supply", &[], Some("i128")),
];

#[cfg(test)]
mod tests {
    use super::{CONFORMING_METHODS, Defect};

    #[test]
    fn every_defect_is_selectable_by_a_name_that_round_trips() {
        // A caller selects a fixture by name from a command line or a test, so a name
        // that did not round-trip would make a fixture unreachable.
        for defect in Defect::ALL {
            assert_eq!(Defect::parse(defect.as_str()), Some(defect));
            assert!(!defect.violates().is_empty());
        }
    }

    #[test]
    fn the_defect_names_are_distinct() {
        let mut names: Vec<&str> = Defect::ALL.iter().map(|defect| defect.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two defects share a name");
    }

    #[test]
    fn an_unknown_defect_name_is_refused_rather_than_defaulted() {
        // Defaulting to the conforming fixture would make a typo look like a passing
        // run against the contract the caller meant to test.
        assert_eq!(Defect::parse("no-such-defect"), None);
    }

    #[test]
    fn only_the_defect_that_removes_decimals_reports_itself_as_doing_so() {
        for defect in Defect::ALL {
            assert_eq!(
                defect.exposes_decimals(),
                defect != Defect::MissingDecimals,
                "{} misreports its interface",
                defect.as_str()
            );
        }
    }

    #[test]
    fn the_conforming_interface_declares_the_methods_sep_41_requires() {
        let names: Vec<&str> = CONFORMING_METHODS
            .iter()
            .map(|(name, _, _)| *name)
            .collect();
        for required in ["transfer", "balance", "allowance", "decimals"] {
            assert!(names.contains(&required), "{required} is missing");
        }
    }
}
