//! Foundation types shared by every crate in the Estamora conformance runner.
//!
//! This crate holds the two decisions that the rest of the runner must not make
//! for itself:
//!
//! 1. **What went wrong, and who is answerable for it** ([`errors`]). A failure
//!    is classified from a closed registry, and the blame is derived from the
//!    class rather than chosen at the call site. A runner that can blame the
//!    contract for a network outage is a runner whose verdicts cannot be
//!    trusted.
//! 2. **What a run concluded, and what CI should do about it** ([`outcomes`]).
//!    Six statuses, whose spelling is a contract with the specification layer's
//!    report schema, and one rule that reduces a set of vector results to a
//!    status and a status to a process exit code.
//! 3. **What a requirement is written in** ([`expr`]). The expression algebra
//!    shared by profile documents and vectors, chosen so that a requirement is
//!    always data to be evaluated rather than code to be run.
//! 4. **What an observation is** ([`value`] and [`world`]). The vocabulary the
//!    evaluator and the execution layer must agree on, so that the evaluator never
//!    learns what a ledger is and the execution layer never learns what a
//!    requirement is.
//!
//! Nothing here knows about Soroban, about files, or about the network. That is
//! deliberate: these are the types a test can pin exhaustively, so the semantics
//! a CI gate depends on are established without an execution environment.

#![forbid(unsafe_code)]

pub mod errors;
pub mod expr;
pub mod outcomes;
pub mod value;
pub mod world;

pub use errors::{
    Blame, Diagnostic, Diagnostics, Error, ErrorClass, Result, Severity, into_result,
};
pub use expr::{ArithmeticOp, DeltaDirection, Literal, Predicate, ValueExpr};
pub use outcomes::{AssertionStatus, ConformanceStatus, ExitCode, RunTally, VectorStatus};
