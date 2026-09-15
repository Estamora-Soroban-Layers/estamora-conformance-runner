//! Evaluating a requirement against an observed execution.
//!
//! A profile states what conformance means; this crate is where a statement becomes
//! a result. It interprets the specification's expression algebra — the values and
//! predicates that a profile's `behavior.yaml` and a vector's expectations are both
//! written in — against a [`World`], and produces an [`Evaluation`] that says
//! whether the requirement held and renders both sides for a reader.
//!
//! # The seven dimensions
//!
//! [`run::evaluate`] is the entry point. It evaluates interface compatibility,
//! authorization, events, behaviour, state, invariants and failure, and returns one
//! [`VectorOutcome`] holding every check that was made. The seven are deliberately
//! not collapsed into a boolean: a caller must be able to see which claim failed, and
//! a passing interface section must never be presented as evidence of correct
//! behaviour.
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

pub mod dimensions;
pub mod eval;
pub mod observation;
pub mod outcome;
pub mod run;
pub mod value;
pub mod world;

pub use dimensions::{
    InterfaceInspection, InvariantReport, ObservedMethod, ObservedParameter, canonical_type,
    observed_verifies_method,
};
pub use eval::{
    Environment, Evaluation, MAX_RESOLUTION_DEPTH, evaluate_predicate,
    evaluate_predicate_for_member, evaluate_value,
};
pub use observation::{CallResult, ObservedAuthorization, ObservedCall, ObservedEvent};
pub use outcome::{AssertionOutcome, OutcomeDiagnostic, VectorOutcome};
pub use run::{RunObservation, evaluate as evaluate_vector};
pub use value::{Comparison, Value};
pub use world::World;

#[cfg(test)]
mod tests;
