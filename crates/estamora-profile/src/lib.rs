//! Loading profile bundles.
//!
//! A profile is the normative half of Estamora and lives in the specification
//! repository. This crate is how the runner reads one: it finds the bundle's
//! entry point, resolves the documents the bundle declares, refuses a format
//! version it cannot execute, and hands back typed structures.
//!
//! # What this crate refuses to do
//!
//! It does not decide whether a profile is *good*. Whether a requirement is
//! correct, complete or faithful to the upstream standard is a review question
//! for the specification repository, and a runner that second-guessed it would be
//! running a different profile from the one that was reviewed.
//!
//! What it does decide is whether a profile is *executable*: whether it is
//! well-formed, whether its documents exist inside the bundle, whether its format
//! version is one this runner understands, and whether its identity matches the
//! place it is stored. An executable-looking profile that fails those checks must
//! stop the run rather than produce a verdict, because a verdict produced against
//! requirements that were partly ignored is worse than no verdict at all.
#![forbid(unsafe_code)]
pub mod spec;

pub use spec::{SUPPORTED_SPEC_VERSION, SpecVersion};
