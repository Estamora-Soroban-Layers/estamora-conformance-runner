//! The Estamora conformance runner.
//!
//! This crate is the whole product: a library that answers *does this Soroban
//! contract actually behave according to the profile it claims to implement*, and a
//! thin command line in front of it. The split is deliberate. Everything that can
//! change a verdict — resolving a contract, preparing a fixture's world, invoking a
//! method, turning what came back into an observation, assembling a report — is in
//! the library, so it can be driven by a test rather than only by a subprocess.
//!
//! # The pipeline
//!
//! ```text
//!   profile bundle  ──▶  validate  ──▶  vector corpus  ──▶  resolve target
//!                                                                  │
//!                                                                  ▼
//!   verdict  ◀──  evaluate  ◀──  observe  ◀──  invoke  ◀──  prepare world
//!      │
//!      ├──▶  report   (JSON · Markdown · JUnit)
//!      └──▶  receipt  (optional, signed)
//! ```
//!
//! Every arrow is a module in [`engine`]. Nothing in the pipeline silently ignores a
//! requirement: a vector that cannot be executed is reported as undecided, and a
//! requirement that could not be applied never becomes a passing result.
//!
//! # What this crate does not do
//!
//! It does not decide whether a profile is *correct*. Whether a requirement is
//! faithful to the standard it claims to encode is a review question for the
//! specification repository, and a runner that second-guessed it would be running a
//! different profile from the one that was reviewed. It decides whether a profile is
//! *executable*, and whether a contract satisfies it.
//!
//! It is also not a security tool. A conformant verdict says a contract behaved as a
//! named profile requires over a named corpus; it says nothing about the absence of
//! vulnerabilities, and every report it produces states that.
//!
//! # Execution environments
//!
//! Contracts are executed in the Soroban SDK's test host. A target is either one of
//! the in-repository fixture contracts, a compiled WebAssembly artifact on disk, or a
//! contract on a network. Network resolution is not linked into this build; see
//! [`engine::target`] for how that is reported, and why it is reported as an
//! environment failure rather than as anything about the contract.

#![forbid(unsafe_code)]

pub mod commands;
pub mod config;
pub mod engine;
pub mod errors;
pub mod output;

pub use config::{DEFAULT_SPEC_ROOT_ENV, RunConfig};
pub use engine::{Inspection, RunOutcome, inspect, run};
pub use errors::exit_code;
