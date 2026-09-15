//! The seven conformance dimensions.
//!
//! Each module evaluates one independent claim about a contract and reports every
//! check it made. They are kept apart because collapsing them would make a caller
//! unable to see *which* claim failed — and would let a passing interface section be
//! presented as evidence of correct behaviour, which is the confusion Estamora
//! exists to remove.

pub mod authorization;
pub mod behavior;
pub mod events;
pub mod failures;
pub mod interface;
pub mod invariants;
pub mod state;

pub use interface::{
    InterfaceInspection, ObservedMethod, ObservedParameter, canonical_type,
    observed_verifies_method,
};
pub use invariants::InvariantReport;
