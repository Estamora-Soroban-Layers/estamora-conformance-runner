//! The world a requirement is evaluated against.
//!
//! This trait lives in `estamora-core` rather than beside the evaluator, because it
//! is the boundary two layers must agree on: the assertion layer reads through it,
//! and the execution layer implements it. Keeping it in the lower crate is what lets
//! the evaluator stay ignorant of Soroban while the Soroban layer stays ignorant of
//! the evaluator.
//!
//! The evaluator never touches a ledger. It reads through this trait, which is
//! what makes every rule about comparison testable without an execution
//! environment — and what keeps the two things the runner must not confuse
//! separate: *what the requirement says*, and *how the environment answers*.
//!
//! # Answers are given in the vector's vocabulary
//!
//! An implementation of [`World`] must report an address as the **fixture actor's
//! name**, not as its `StrKey`, and must accept arguments in the same vocabulary. It
//! owns the mapping, because it is the only layer that knows which fixture actor
//! was deployed as which address — and because a requirement that fails should
//! name `alice` rather than fifty-six characters of base-32 that no reader can
//! check by eye.
//!
//! # Reads are the only way state is observed
//!
//! A profile can only assert about state it can read through a method it declares.
//! That is a deliberate limitation of the interface, not of the runner: a contract
//! whose invariants are untrue in storage the interface does not expose is a
//! contract Estamora cannot measure, and saying so is better than guessing at
//! storage keys the specification never named.

use crate::Result;
use crate::value::Value;

/// The state and ledger facts a requirement is evaluated against.
///
/// One implementation backs a **before** world and another backs an **after**
/// world, because a relative requirement — "the balance fell by the amount" — is a
/// statement about a change, and a change cannot be read from a single snapshot.
pub trait World {
    /// Calls a read-only method the profile declares and returns its value.
    ///
    /// Arguments are supplied in the vector's vocabulary. The implementation maps
    /// a fixture actor's name to whatever address that actor was deployed as, and
    /// maps any address it returns back to the actor's name.
    ///
    /// # Errors
    ///
    /// Returns an execution failure when the read could not be performed. A read
    /// that could not be made is not a failed requirement: no statement about the
    /// contract can be derived from an observation that does not exist.
    fn read(&self, method: &str, args: &[Value]) -> Result<Value>;

    /// Every member of a resource set, in a deterministic order.
    ///
    /// Used by aggregates and by the invariants that range over a set. The order
    /// must be stable, because a report of a violation names the member it was
    /// found on and two runs must name the same one.
    ///
    /// # Errors
    ///
    /// Returns an execution failure when the set could not be read.
    fn resource_members(&self, resource: &str) -> Result<Vec<Value>>;

    /// The ledger sequence the operation executed at.
    fn ledger_sequence(&self) -> u32;

    /// The ledger at which a fixture allowance expires, when the fixture declared
    /// one.
    ///
    /// Expiry is not observable through the interface: once an allowance has lapsed
    /// it is indistinguishable from one that never existed. So a requirement about
    /// expiry has to be answered from the fixture that created it, and `None` here
    /// means the fixture left expiry to the implementation's default.
    fn allowance_expiry(&self, from: &str, spender: &str) -> Option<u32>;
}
