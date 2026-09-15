//! Loading and resolving the vector corpus.
//!
//! A vector is the executable half of the specification. Where a profile says what
//! conformance *means*, a vector says what one call must do: against which world,
//! with which inputs, with which signatures, and with exactly which observable
//! result. This crate turns the corpus a profile declares into typed documents the
//! assertion layer can execute.
//!
//! # What this crate refuses to do
//!
//! It does not decide whether a vector is *right*. Whether the scenario it
//! constructs is the one the requirement is about is a review question for the
//! specification repository.
//!
//! It decides whether a vector is *executable against this profile*: whether the
//! identity it declares is the identity it was found under, whether the method it
//! exercises is declared, whether its inputs cover that method's arguments exactly,
//! whether every failure, event and invariant it names exists, and whether every
//! fixture actor and read it refers to is declared. A vector that fails any of
//! those would be one whose requirement is silently not applied, and a run that
//! silently does not apply a requirement reports a contract as conformant against
//! a corpus that was never fully evaluated.
//!
//! # Amounts
//!
//! Every quantity is an [`IntegerString`]: written as a string in the document and
//! parsed once, at load, into an exact `i128`. See [`integer`] for why a number
//! would not do.
#![forbid(unsafe_code)]

pub mod integer;
pub mod loader;
pub mod model;
pub mod validate;

pub use integer::IntegerString;
pub use loader::{MAX_VECTORS, MAX_WALK_DEPTH, Vector, VectorCorpus};
pub use model::{
    ActorKind, Assertion, AssertionCategory, AuthorizationExpectation, AuthorizationPlan,
    EventExpectation, EventExpectations, ExpectedOutcome, ExpectedResult, FixtureActor,
    FixtureAllowance, Fixtures, LedgerFixture, StateAssertion, StateResourceRef, VectorDocument,
    VectorKind,
};

#[cfg(test)]
mod tests;
