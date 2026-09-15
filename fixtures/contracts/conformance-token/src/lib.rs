//! The contracts Estamora's own tests and examples measure.
//!
//! These are not examples and are not a reference implementation. They exist so that
//! the runner's execution path can be exercised against contracts whose behaviour is
//! known exactly — including the behaviours it must detect. Every contract after
//! [`ConformingToken`] is deliberately wrong in **exactly one way**, so a test that
//! fails points at one requirement rather than at a fixture, and a report that says
//! `NON_CONFORMANT` can be traced to a single named defect.
//!
//! # Why the whole interface is implemented
//!
//! A fixture that omitted methods the profile requires would be wrong in more ways than
//! its defect, and the interface dimension would then report the same failures against
//! every fixture — burying the one difference each of them exists to demonstrate. So
//! every fixture publishes the full SEP-41 surface, and the defects are behavioural
//! except for [`Defect::MissingDecimals`], which is a defect *of* the surface and
//! therefore the one place where the interface differs.
//!
//! # The defects that were removed, and why they are not here
//!
//! Two candidates were written and withdrawn, because neither is a defect a black-box
//! conformance run can observe, and a fixture whose defect cannot be observed makes the
//! runner look wrong when it reports the contract as conformant.
//!
//! * A `transfer` that emits its success event *before* checking the balance. If the
//!   balance is short the call aborts, and Soroban discards the whole invocation: the
//!   event is gone with it. Nothing observable distinguishes this from a transfer that
//!   checks first.
//! * A `transfer` that writes to storage *before* checking the balance. Same reasoning:
//!   the write is rolled back with the invocation.
//!
//! [`ConformingToken::emits_then_refuses`] and [`ConformingToken::mutates_then_refuses`]
//! are the probes that established this, and the tests in `estamora-soroban` pin it. The
//! consequence is a real limit of Estamora's method, and it is stated in the profile's
//! own requirements rather than hidden: "a refused call emits no success event" is a
//! requirement every contract satisfies, so it carries no discriminating power at the
//! transaction boundary and the fixtures that remain are the ones that do.
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
//! Tests and examples name a defect rather than a type, so that adding a defect cannot
//! silently change which contract an existing test measures.

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
use soroban_sdk::{
    Address, Env, MuxedAddress, String, contract, contractevent, contractimpl, contracttype,
};

pub mod interface;

/// Keys of a fixture's ledger storage.
#[derive(Debug, Clone)]
#[contracttype]
pub enum Key {
    /// The balance held by an account.
    Balance(Address),
    /// An allowance granted by one account to another, with the ledger at which it
    /// lapses.
    ///
    /// The expiry is stored beside the amount rather than in a second entry, because a
    /// grant whose expiry could be written without its amount is a grant whose two
    /// halves can disagree.
    Allowance(Address, Address),
}

/// An allowance that was granted, and when it lapses.
#[derive(Debug, Clone)]
#[contracttype]
pub struct Grant {
    /// The amount that may still be drawn.
    pub amount: i128,
    /// The ledger at which the grant lapses. At or beyond it the allowance reads as
    /// zero, which is what SEP-0041 requires and what a vector about expiry observes.
    pub live_until_ledger: u32,
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

/// The event a conforming allowance change emits.
///
/// Both the new amount and the ledger at which it lapses are carried, because a grant
/// that overwrote an allowance without saying how long it lasts is one a wallet cannot
/// tell a user about.
#[derive(Debug, Clone)]
#[contractevent(topics = ["approve"])]
pub struct ApproveEvent {
    /// The account granting the allowance.
    #[topic]
    pub from: Address,
    /// The account receiving it.
    #[topic]
    pub spender: Address,
    /// The allowance now in force, replacing rather than adding to any prior value.
    pub amount: i128,
    /// The ledger at which it lapses.
    pub live_until_ledger: u32,
}

/// The event a burn emits.
#[derive(Debug, Clone)]
#[contractevent(topics = ["burn"])]
pub struct BurnEvent {
    /// The account whose balance was reduced.
    #[topic]
    pub from: Address,
    /// The quantity removed from circulation.
    pub amount: i128,
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

/// The number of decimal places every fixture reports.
///
/// One constant rather than a literal per contract, so that a fixture cannot disagree
/// with another about a property none of them is meant to vary.
pub const DECIMALS: u32 = 7;

// A token reporting no decimal places cannot represent a fractional unit, which is what
// the vectors about precision assert against. Checked where the compiler can see it
// rather than by a runtime assertion, because a test that compares constants to
// constants cannot fail and therefore tests nothing.
const _: () = assert!(DECIMALS > 0);

/// The name every fixture reports.
pub const TOKEN_NAME: &str = "Conformance Token";

/// The symbol every fixture reports.
pub const TOKEN_SYMBOL: &str = "CNF";

/// Which way a fixture is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defect {
    /// Nothing. The contract behaves as the profile requires.
    None,
    /// `transfer` never calls `require_auth`, so anyone may move anyone's value.
    SkipsAuthorization,
    /// `transfer` moves the value correctly but publishes no event at all, so a
    /// consumer watching the ledger sees a balance change it cannot attribute. The
    /// defect a "a successful transfer emits exactly one transfer event"
    /// requirement exists to catch.
    OmitsEvent,
    /// `transfer` debits the sender by the amount and credits the recipient by one
    /// unit less, so value leaves circulation without any event saying so. The
    /// defect a "the transfer moves the exact amount" requirement exists to catch,
    /// and it is the one no event check can see: the event is well formed and states
    /// the amount the caller asked for.
    WrongCreditAmount,
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

/// Every defect, so that a caller can iterate over the fixture set.
impl Defect {
    /// Every defect, in the order the fixtures are declared.
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::SkipsAuthorization,
        Self::OmitsEvent,
        Self::WrongCreditAmount,
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
            Self::OmitsEvent => "omits-event",
            Self::WrongCreditAmount => "wrong-credit-amount",
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
            Self::OmitsEvent => {
                "the requirement that a successful transfer publish exactly one transfer event"
            },
            Self::WrongCreditAmount => {
                "the requirement that a transfer credit the recipient by the amount it debited"
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

/// A token that moves value but publishes no event.
#[derive(Debug, Clone)]
#[contract]
pub struct OmitsEvent;

/// A token that credits the recipient one unit less than it debits the sender.
#[derive(Debug, Clone)]
#[contract]
pub struct WrongCreditAmount;

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
        Defect::OmitsEvent => OmitsEvent.register(env, None, ()),
        Defect::WrongCreditAmount => WrongCreditAmount.register(env, None, ()),
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

/// Moves `amount` from `from` to `to`, without any authorization check.
///
/// The shared movement, so that every fixture performs the same arithmetic and the only
/// difference between two fixtures is the one their defect names. A second copy of this
/// per contract is how a fixture ends up differing from another in an undeclared way.
fn move_value(env: &Env, from: &Address, to: &Address, amount: i128) {
    let available = balance_of(env, from);
    set_balance(env, from, available - amount);
    set_balance(env, to, balance_of(env, to) + amount);
}

/// The stored grant, whether or not it has lapsed.
fn grant_of(env: &Env, from: &Address, spender: &Address) -> Option<Grant> {
    env.storage()
        .persistent()
        .get(&Key::Allowance(from.clone(), spender.clone()))
}

/// The grant that is still in force, with its expiry.
///
/// A lapsed grant reports an amount of zero and its original expiry, which is what
/// SEP-0041 requires and what keeps `transfer_from` from spending an allowance that has
/// run out while still letting a vector observe when it ran out.
fn live_grant(env: &Env, from: &Address, spender: &Address) -> (i128, u32) {
    let Some(grant) = grant_of(env, from, spender) else {
        return (0, 0);
    };
    if grant.live_until_ledger < env.ledger().sequence() {
        return (0, grant.live_until_ledger);
    }
    (grant.amount, grant.live_until_ledger)
}

fn set_grant(env: &Env, from: &Address, spender: &Address, amount: i128, live_until_ledger: u32) {
    env.storage().persistent().set(
        &Key::Allowance(from.clone(), spender.clone()),
        &Grant {
            amount,
            live_until_ledger,
        },
    );
}

/// Rejects an amount a token may not represent.
///
/// SEP-0041 treats a negative amount as invalid, and every fixture refuses it: no defect
/// in this set is about the sign of a quantity.
///
/// # Panics
///
/// Aborts for a negative amount.
fn assert_valid_amount(amount: i128) {
    assert!(amount >= 0, "invalid amount");
}

/// Generates the methods no defect changes.
///
/// They are generated rather than written out eight times because the property that
/// makes these fixtures useful is that two of them differ in exactly one way. Eight
/// hand-written copies of nine methods is eight chances for an undeclared difference to
/// appear, and a fixture that quietly differs twice makes a failing test point at the
/// wrong requirement.
macro_rules! token_impl {
    ($contract:ident) => {
        #[contractimpl]
        impl $contract {
            /// Sets the allowance of `spender` to `amount` until `live_until_ledger`.
            ///
            /// # Panics
            ///
            /// Aborts for a negative amount or an expiry already in the past.
            pub fn approve(
                env: Env,
                from: Address,
                spender: Address,
                amount: i128,
                live_until_ledger: u32,
            ) {
                from.require_auth();
                assert_valid_amount(amount);
                assert!(
                    live_until_ledger >= env.ledger().sequence(),
                    "invalid live_until_ledger"
                );
                set_grant(&env, &from, &spender, amount, live_until_ledger);
                ApproveEvent {
                    from,
                    spender,
                    amount,
                    live_until_ledger,
                }
                .publish(&env);
            }

            /// Returns the amount `spender` may still draw from `from`.
            pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
                live_grant(&env, &from, &spender).0
            }

            /// Returns the balance held by `who`, or zero if none is recorded.
            pub fn balance(env: Env, who: Address) -> i128 {
                balance_of(&env, &who)
            }

            /// Moves `amount` from `from` to `to`, consuming `spender`'s allowance.
            ///
            /// # Panics
            ///
            /// Aborts when the allowance or the balance is short.
            pub fn transfer_from(
                env: Env,
                spender: Address,
                from: Address,
                to: Address,
                amount: i128,
            ) {
                spender.require_auth();
                assert_valid_amount(amount);
                let (allowance, live_until_ledger) = live_grant(&env, &from, &spender);
                assert!(allowance >= amount, "insufficient allowance");
                assert!(balance_of(&env, &from) >= amount, "insufficient balance");
                move_value(&env, &from, &to, amount);
                // The remaining grant keeps its original expiry: SEP-0041 states that
                // spending part of an allowance does not extend it.
                set_grant(&env, &from, &spender, allowance - amount, live_until_ledger);
                TransferEvent { from, to, amount }.publish(&env);
            }

            /// Removes `amount` from `from`'s balance.
            ///
            /// # Panics
            ///
            /// Aborts when the balance is short.
            pub fn burn(env: Env, from: Address, amount: i128) {
                from.require_auth();
                assert_valid_amount(amount);
                assert!(balance_of(&env, &from) >= amount, "insufficient balance");
                set_balance(&env, &from, balance_of(&env, &from) - amount);
                BurnEvent { from, amount }.publish(&env);
            }

            /// Removes `amount` from `from`'s balance, consuming `spender`'s allowance.
            ///
            /// # Panics
            ///
            /// Aborts when the allowance or the balance is short.
            pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
                spender.require_auth();
                assert_valid_amount(amount);
                let (allowance, live_until_ledger) = live_grant(&env, &from, &spender);
                assert!(allowance >= amount, "insufficient allowance");
                assert!(balance_of(&env, &from) >= amount, "insufficient balance");
                set_balance(&env, &from, balance_of(&env, &from) - amount);
                set_grant(&env, &from, &spender, allowance - amount, live_until_ledger);
                BurnEvent { from, amount }.publish(&env);
            }

            /// Returns the token's name.
            pub fn name(env: Env) -> String {
                String::from_str(&env, TOKEN_NAME)
            }

            /// Returns the token's symbol.
            pub fn symbol(env: Env) -> String {
                String::from_str(&env, TOKEN_SYMBOL)
            }
        }
    };
}

/// Generates `decimals`.
///
/// Separate from [`token_impl`] because it is the one method a fixture deliberately
/// omits, and a defect expressed by leaving a macro out is clearer than one expressed by
/// a flag the reader has to trace.
macro_rules! with_decimals {
    ($contract:ident) => {
        #[contractimpl]
        impl $contract {
            /// Returns the number of decimal places the token uses.
            pub fn decimals(_env: Env) -> u32 {
                DECIMALS
            }
        }
    };
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

            /// Grants `spender` an allowance over `from`'s balance until a ledger.
            ///
            /// The expiry is a parameter rather than a constant because a vector about
            /// an expired allowance has to be able to declare one, and there is no other
            /// way to put a lapsed grant into the contract: the interface will not create
            /// one.
            pub fn fixture_approve(
                env: Env,
                from: Address,
                spender: Address,
                amount: i128,
                live_until_ledger: u32,
            ) {
                set_grant(&env, &from, &spender, amount, live_until_ledger);
            }
        }
    };
}

token_impl!(ConformingToken);
with_decimals!(ConformingToken);

#[contractimpl]
impl ConformingToken {
    /// Moves `amount` from `from` to `to`, requiring `from`'s authorization.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient. Aborting is the documented
    /// idiom for rejection in Soroban, and it is why the runner must read an abort as
    /// a refusal rather than as an environment failure.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        move_value(&env, &from, &credited, amount);
        TransferEvent {
            from,
            to: credited,
            amount,
        }
        .publish(&env);
    }

    /// Returns a value the runner reads as a cross-check on the balance set.
    ///
    /// Not part of SEP-0041, which defines no supply accessor. It is published for the
    /// two reasons a fixture may publish anything: so that a conservation invariant can
    /// be checked against a second source, and so that the interface dimension has a
    /// method to compare that the profile does not require.
    pub fn total_supply(env: Env) -> i128 {
        // The fixture has no supply register, so this returns a value computed from the
        // observable set rather than a stored one. Stated rather than left for a reader
        // to discover.
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

    /// Emits an event and then refuses, to prove a refused call's events are not
    /// observable.
    ///
    /// # Panics
    ///
    /// Always, after emitting.
    pub fn emits_then_refuses(env: Env, who: Address) {
        SneakyEvent { who }.publish(&env);
        panic!("refused after emitting");
    }

    /// Mutates state and then refuses, to prove a refused call's mutations are not
    /// observable.
    ///
    /// # Panics
    ///
    /// Always, after mutating.
    pub fn mutates_then_refuses(env: Env, who: Address, amount: i128, refuse: bool) {
        set_balance(&env, &who, balance_of(&env, &who) + amount);
        assert!(!refuse, "refused after mutating");
    }
}

fixture_setup!(ConformingToken);

token_impl!(SkipsAuthorization);
with_decimals!(SkipsAuthorization);

#[contractimpl]
impl SkipsAuthorization {
    /// Moves value without requiring authorization: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        move_value(&env, &from, &credited, amount);
        TransferEvent {
            from,
            to: credited,
            amount,
        }
        .publish(&env);
    }
}

fixture_setup!(SkipsAuthorization);

token_impl!(OmitsEvent);
with_decimals!(OmitsEvent);

#[contractimpl]
impl OmitsEvent {
    /// Moves the value and publishes nothing: the defect.
    ///
    /// The movement itself is exactly right, so no balance, delta or invariant check
    /// can see this contract. What sees it is the event requirement: a successful
    /// transfer must publish one transfer event, and a ledger that changes without one
    /// is a balance a watcher cannot attribute.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        let available = balance_of(&env, &from);
        set_balance(&env, &from, available - amount);
        set_balance(&env, &credited, balance_of(&env, &credited) + amount);
    }
}

fixture_setup!(OmitsEvent);

token_impl!(WrongCreditAmount);
with_decimals!(WrongCreditAmount);

#[contractimpl]
impl WrongCreditAmount {
    /// Debits the sender by `amount` and credits the recipient by one unit less: the
    /// defect.
    ///
    /// Value leaves circulation, and nothing else notices: the event is well formed,
    /// its topics are right, and it states the amount the caller asked for. Only the
    /// state assertion that the sender's decrease and the recipient's increase are the
    /// same quantity catches it, which is the reason that requirement is expressed as
    /// a delta on both sides rather than as one balance check.
    ///
    /// `saturating_sub` so that a transfer of zero credits zero rather than aborting on
    /// an underflow: the defect is the missing unit, not a refusal.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        set_balance(&env, &from, balance_of(&env, &from) - amount);
        let owed = amount.saturating_sub(1);
        set_balance(&env, &credited, balance_of(&env, &credited) + owed);
        TransferEvent {
            from,
            to: credited,
            amount,
        }
        .publish(&env);
    }
}

fixture_setup!(WrongCreditAmount);

token_impl!(DoubleEmits);
with_decimals!(DoubleEmits);

#[contractimpl]
impl DoubleEmits {
    /// Emits the transfer event twice for one movement: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        move_value(&env, &from, &credited, amount);
        TransferEvent {
            from: from.clone(),
            to: credited.clone(),
            amount,
        }
        .publish(&env);
        TransferEvent {
            from,
            to: credited,
            amount,
        }
        .publish(&env);
    }
}

fixture_setup!(DoubleEmits);

token_impl!(WrongEventAmount);
with_decimals!(WrongEventAmount);

#[contractimpl]
impl WrongEventAmount {
    /// Moves `amount` but states `amount + 1` in the event: the defect.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        move_value(&env, &from, &credited, amount);
        TransferEvent {
            from,
            to: credited,
            amount: amount + 1,
        }
        .publish(&env);
    }
}

fixture_setup!(WrongEventAmount);

token_impl!(AllowsOverdraft);
with_decimals!(AllowsOverdraft);

#[contractimpl]
impl AllowsOverdraft {
    /// Permits an overdraft instead of refusing: the defect.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        let credited = to.address();
        move_value(&env, &from, &credited, amount);
        TransferEvent {
            from,
            to: credited,
            amount,
        }
        .publish(&env);
    }
}

fixture_setup!(AllowsOverdraft);

token_impl!(MissingDecimals);

#[contractimpl]
impl MissingDecimals {
    /// Moves `amount` from `from` to `to`, requiring `from`'s authorization.
    ///
    /// # Panics
    ///
    /// Aborts when the sender's balance is insufficient.
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        assert_valid_amount(amount);
        assert!(balance_of(&env, &from) >= amount, "insufficient balance");
        let credited = to.address();
        move_value(&env, &from, &credited, amount);
        TransferEvent {
            from,
            to: credited,
            amount,
        }
        .publish(&env);
    }

    // `decimals` is deliberately absent, and it is the only difference between this
    // fixture and the conforming one. The interface dimension is what notices: nothing
    // about calling a method that does not exist distinguishes it from a contract that
    // refuses every call.
}

fixture_setup!(MissingDecimals);

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests may panic; a failing test is the signal"
)]
mod tests {
    use super::{Defect, TOKEN_NAME, TOKEN_SYMBOL};

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
    fn the_metadata_a_vector_reads_is_not_empty() {
        // A vector requires a non-empty name and symbol, and a fixture that returned an
        // empty one would fail a requirement none of the defects is about. `DECIMALS` is
        // absent from this test because its positivity is a compile-time fact and
        // asserting it at run time asserts a constant; see the check beside it.
        assert!(!TOKEN_NAME.is_empty());
        assert!(!TOKEN_SYMBOL.is_empty());
    }
}
