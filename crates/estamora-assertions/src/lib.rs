//! Evaluating a requirement against an observed execution.
//!
//! A profile states what conformance means; this crate is where a statement becomes
//! a result. It interprets the specification's expression algebra — the values and
//! predicates that a profile's `behavior.yaml` and a vector's expectations are both
//! written in — against a [`World`], and produces an [`Evaluation`] that says
//! whether the requirement held and renders both sides for a reader.
//!
//! # What it is deliberately not
//!
//! It is not a Soroban crate by nature, even though it sits above one in the
//! workspace layering. The evaluator reaches state only through [`world::World`],
//! so every rule about comparison is testable without a ledger, and the two things
//! the runner must not confuse stay apart: what a requirement *says*, and how an
//! environment *answers*.
//!
//! # Three outcomes, and why they are not two
//!
//! A requirement can hold, fail to hold, or fail to be *evaluated*. The third is not
//! a stricter kind of failure: a read that could not be performed, arithmetic that
//! left the representable range, or a value that is not comparable to the one the
//! requirement names all mean no statement about the contract was reached. Reporting
//! any of them as a conformance failure would blame a contract for the runner's
//! inability to measure it — which is the single most damaging mistake this system
//! could make.
#![forbid(unsafe_code)]

pub mod eval;
pub mod value;
pub mod world;

pub use eval::{
    Environment, Evaluation, MAX_RESOLUTION_DEPTH, evaluate_predicate,
    evaluate_predicate_for_member, evaluate_value,
};
pub use value::{Comparison, Value};
pub use world::World;

#[cfg(test)]
mod tests;
